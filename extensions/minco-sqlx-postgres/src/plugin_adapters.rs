use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use minco_plugin_audit::{AuditError, AuditEvent, AuditSink};
use minco_plugin_events::{DomainEvent, EventError, OutboxRecord, OutboxStatus, OutboxStore};
use minco_plugin_idempotency::{
    BeginOutcome, IdempotencyError, IdempotencyKey, IdempotencyLease, IdempotencyRecord,
    IdempotencyStore, RequestFingerprint, validate_claim_timeout,
};
use minco_plugin_sessions::{
    SessionError, SessionId, SessionRecord, SessionStore, SessionTokenHash,
};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PostgresOutboxStore {
    pool: PgPool,
}

impl PostgresOutboxStore {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Inserts an outbox record into an application adapter's existing
    /// transaction so the domain mutation and publication intent commit
    /// atomically.
    pub async fn enqueue_in(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        record: OutboxRecord,
    ) -> Result<(), EventError> {
        validate_event(&record.event)?;
        if record.status != OutboxStatus::Pending {
            return Err(EventError::InvalidOutboxState);
        }
        let event_id = record.event.id;
        let attempt_count =
            i32::try_from(record.attempt_count).map_err(event_infrastructure_error)?;
        let metadata =
            serde_json::to_value(&record.event.metadata).map_err(event_infrastructure_error)?;
        let result = sqlx::query(
            "INSERT INTO minco_outbox
             (event_id, event_type, aggregate_type, aggregate_id, correlation_id, occurred_at,
              payload, metadata, status, attempt_count, available_at, claimed_by,
              claim_expires_at, last_error)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending', $9, $10, $11, $12, $13)",
        )
        .bind(event_id)
        .bind(record.event.event_type)
        .bind(record.event.aggregate_type)
        .bind(record.event.aggregate_id)
        .bind(record.event.correlation_id)
        .bind(record.event.occurred_at)
        .bind(record.event.payload)
        .bind(metadata)
        .bind(attempt_count)
        .bind(record.available_at)
        .bind(record.claimed_by)
        .bind(record.claim_expires_at)
        .bind(record.last_error)
        .execute(&mut **transaction)
        .await;
        match result {
            Ok(_) => Ok(()),
            Err(error) if is_unique_violation(&error) => Err(EventError::DuplicateEvent(event_id)),
            Err(error) => Err(event_infrastructure_error(error)),
        }
    }
}

#[async_trait]
impl OutboxStore for PostgresOutboxStore {
    async fn enqueue(&self, record: OutboxRecord) -> Result<(), EventError> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(event_infrastructure_error)?;
        self.enqueue_in(&mut transaction, record).await?;
        transaction
            .commit()
            .await
            .map_err(event_infrastructure_error)
    }

    async fn claim_pending(
        &self,
        worker_id: &str,
        limit: usize,
        claim_expires_at: DateTime<Utc>,
    ) -> Result<Vec<OutboxRecord>, EventError> {
        validate_claim(worker_id, claim_expires_at)?;
        let now = Utc::now();
        let limit = i64::try_from(limit).map_err(|_| EventError::InvalidClaim)?;
        if limit == 0 {
            return Err(EventError::InvalidClaim);
        }
        let rows = sqlx::query(
            "WITH candidates AS (
                 SELECT event_id
                 FROM minco_outbox
                 WHERE status IN ('pending', 'failed') AND available_at <= $1
                 ORDER BY available_at, occurred_at, event_id
                 FOR UPDATE SKIP LOCKED
                 LIMIT $2
             )
             UPDATE minco_outbox AS outbox
             SET status = 'claimed',
                 claimed_by = $3,
                 claim_expires_at = $4,
                 attempt_count = outbox.attempt_count + 1
             FROM candidates
             WHERE outbox.event_id = candidates.event_id
             RETURNING outbox.*",
        )
        .bind(now)
        .bind(limit)
        .bind(worker_id)
        .bind(claim_expires_at)
        .fetch_all(&self.pool)
        .await
        .map_err(event_infrastructure_error)?;
        rows.iter().map(decode_outbox).collect()
    }

    async fn claim_event(
        &self,
        event_id: Uuid,
        worker_id: &str,
        claim_expires_at: DateTime<Utc>,
    ) -> Result<Option<OutboxRecord>, EventError> {
        validate_claim(worker_id, claim_expires_at)?;
        let now = Utc::now();
        let row = sqlx::query(
            "UPDATE minco_outbox
             SET status = 'claimed', claimed_by = $2, claim_expires_at = $3,
                 attempt_count = attempt_count + 1
             WHERE event_id = $1
               AND status IN ('pending', 'failed')
               AND available_at <= $4
             RETURNING *",
        )
        .bind(event_id)
        .bind(worker_id)
        .bind(claim_expires_at)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(event_infrastructure_error)?;
        row.as_ref().map(decode_outbox).transpose()
    }

    async fn mark_published(&self, event_id: Uuid, worker_id: &str) -> Result<(), EventError> {
        let result = sqlx::query(
            "UPDATE minco_outbox
             SET status = 'published', claimed_by = NULL, claim_expires_at = NULL,
                 last_error = NULL
             WHERE event_id = $1 AND status = 'claimed' AND claimed_by = $2",
        )
        .bind(event_id)
        .bind(worker_id)
        .execute(&self.pool)
        .await
        .map_err(event_infrastructure_error)?;
        require_claim_update(&self.pool, result.rows_affected(), event_id, worker_id).await
    }

    async fn mark_failed(
        &self,
        event_id: Uuid,
        worker_id: &str,
        error: String,
        retry_at: DateTime<Utc>,
    ) -> Result<(), EventError> {
        let result = sqlx::query(
            "UPDATE minco_outbox
             SET status = 'failed', claimed_by = NULL, claim_expires_at = NULL,
                 available_at = $3, last_error = $4
             WHERE event_id = $1 AND status = 'claimed' AND claimed_by = $2",
        )
        .bind(event_id)
        .bind(worker_id)
        .bind(retry_at)
        .bind(error)
        .execute(&self.pool)
        .await
        .map_err(event_infrastructure_error)?;
        require_claim_update(&self.pool, result.rows_affected(), event_id, worker_id).await
    }

    async fn recover_expired_claims(&self, now: DateTime<Utc>) -> Result<usize, EventError> {
        let result = sqlx::query(
            "UPDATE minco_outbox
             SET status = 'pending', claimed_by = NULL, claim_expires_at = NULL
             WHERE status = 'claimed' AND claim_expires_at <= $1",
        )
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(event_infrastructure_error)?;
        usize::try_from(result.rows_affected()).map_err(event_infrastructure_error)
    }
}

async fn require_claim_update(
    pool: &PgPool,
    rows_affected: u64,
    event_id: Uuid,
    worker_id: &str,
) -> Result<(), EventError> {
    if rows_affected == 1 {
        return Ok(());
    }
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM minco_outbox WHERE event_id = $1)")
            .bind(event_id)
            .fetch_one(pool)
            .await
            .map_err(event_infrastructure_error)?;
    if exists {
        Err(EventError::ClaimOwnership {
            event_id,
            worker_id: worker_id.to_owned(),
        })
    } else {
        Err(EventError::MissingEvent(event_id))
    }
}

fn decode_outbox(row: &sqlx::postgres::PgRow) -> Result<OutboxRecord, EventError> {
    let status: String = row.try_get("status").map_err(event_infrastructure_error)?;
    let status = match status.as_str() {
        "pending" => OutboxStatus::Pending,
        "claimed" => OutboxStatus::Claimed,
        "published" => OutboxStatus::Published,
        "failed" => OutboxStatus::Failed,
        _ => {
            return Err(event_infrastructure_error(
                "invalid outbox status in PostgreSQL",
            ));
        }
    };
    let metadata: serde_json::Value = row
        .try_get("metadata")
        .map_err(event_infrastructure_error)?;
    let attempt_count: i32 = row
        .try_get("attempt_count")
        .map_err(event_infrastructure_error)?;
    Ok(OutboxRecord {
        event: DomainEvent {
            id: row
                .try_get("event_id")
                .map_err(event_infrastructure_error)?,
            event_type: row
                .try_get("event_type")
                .map_err(event_infrastructure_error)?,
            aggregate_type: row
                .try_get("aggregate_type")
                .map_err(event_infrastructure_error)?,
            aggregate_id: row
                .try_get("aggregate_id")
                .map_err(event_infrastructure_error)?,
            correlation_id: row
                .try_get("correlation_id")
                .map_err(event_infrastructure_error)?,
            occurred_at: row
                .try_get("occurred_at")
                .map_err(event_infrastructure_error)?,
            payload: row.try_get("payload").map_err(event_infrastructure_error)?,
            metadata: serde_json::from_value(metadata).map_err(event_infrastructure_error)?,
        },
        status,
        attempt_count: u32::try_from(attempt_count).map_err(event_infrastructure_error)?,
        available_at: row
            .try_get("available_at")
            .map_err(event_infrastructure_error)?,
        claimed_by: row
            .try_get("claimed_by")
            .map_err(event_infrastructure_error)?,
        claim_expires_at: row
            .try_get("claim_expires_at")
            .map_err(event_infrastructure_error)?,
        last_error: row
            .try_get("last_error")
            .map_err(event_infrastructure_error)?,
    })
}

#[derive(Debug, Clone)]
pub struct PostgresSessionStore {
    pool: PgPool,
}

impl PostgresSessionStore {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SessionStore for PostgresSessionStore {
    async fn create(
        &self,
        token_hash: SessionTokenHash,
        session: SessionRecord,
    ) -> Result<(), SessionError> {
        let attributes = serde_json::to_value(&session.attributes).map_err(session_store_error)?;
        let result = sqlx::query(
            "INSERT INTO minco_sessions
             (id, token_hash, subject, created_at, expires_at, revoked_at, attributes)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(session.id.0)
        .bind(token_hash.as_bytes().as_slice())
        .bind(session.subject)
        .bind(session.created_at)
        .bind(session.expires_at)
        .bind(session.revoked_at)
        .bind(attributes)
        .execute(&self.pool)
        .await;
        match result {
            Ok(_) => Ok(()),
            Err(error) if is_unique_violation(&error) => Err(SessionError::Duplicate),
            Err(error) => Err(session_store_error(error)),
        }
    }

    async fn find_by_token_hash(
        &self,
        token_hash: SessionTokenHash,
    ) -> Result<Option<SessionRecord>, SessionError> {
        let row = sqlx::query(
            "SELECT id, subject, created_at, expires_at, revoked_at, attributes
             FROM minco_sessions WHERE token_hash = $1",
        )
        .bind(token_hash.as_bytes().as_slice())
        .fetch_optional(&self.pool)
        .await
        .map_err(session_store_error)?;
        row.as_ref().map(decode_session).transpose()
    }

    async fn revoke(&self, id: SessionId, at: DateTime<Utc>) -> Result<bool, SessionError> {
        let result = sqlx::query(
            "UPDATE minco_sessions SET revoked_at = $2
             WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id.0)
        .bind(at)
        .execute(&self.pool)
        .await
        .map_err(session_store_error)?;
        Ok(result.rows_affected() == 1)
    }

    async fn revoke_subject(
        &self,
        subject: &str,
        at: DateTime<Utc>,
    ) -> Result<usize, SessionError> {
        let result = sqlx::query(
            "UPDATE minco_sessions SET revoked_at = $2
             WHERE subject = $1 AND revoked_at IS NULL",
        )
        .bind(subject)
        .bind(at)
        .execute(&self.pool)
        .await
        .map_err(session_store_error)?;
        usize::try_from(result.rows_affected())
            .map_err(|error| session_store_error(error.to_string()))
    }
}

fn decode_session(row: &sqlx::postgres::PgRow) -> Result<SessionRecord, SessionError> {
    let attributes: serde_json::Value = row.try_get("attributes").map_err(session_store_error)?;
    Ok(SessionRecord {
        id: SessionId(row.try_get("id").map_err(session_store_error)?),
        subject: row.try_get("subject").map_err(session_store_error)?,
        created_at: row.try_get("created_at").map_err(session_store_error)?,
        expires_at: row.try_get("expires_at").map_err(session_store_error)?,
        revoked_at: row.try_get("revoked_at").map_err(session_store_error)?,
        attributes: serde_json::from_value(attributes).map_err(session_store_error)?,
    })
}

#[derive(Debug, Clone)]
pub struct PostgresIdempotencyStore {
    pool: PgPool,
}

impl PostgresIdempotencyStore {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IdempotencyStore for PostgresIdempotencyStore {
    async fn get(
        &self,
        key: &IdempotencyKey,
    ) -> Result<Option<IdempotencyRecord>, IdempotencyError> {
        let row = sqlx::query(
            "SELECT fingerprint, response, completed_at
             FROM minco_idempotency WHERE key = $1 AND state = 'completed'",
        )
        .bind(key.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(idempotency_store_error)?;
        row.as_ref().map(decode_completed).transpose()
    }

    async fn begin(
        &self,
        key: IdempotencyKey,
        fingerprint: RequestFingerprint,
        now: DateTime<Utc>,
        stale_after: TimeDelta,
    ) -> Result<BeginOutcome, IdempotencyError> {
        validate_claim_timeout(stale_after)?;
        let lease_id = Uuid::now_v7();
        let mut transaction = self.pool.begin().await.map_err(idempotency_store_error)?;
        let inserted = sqlx::query(
            "INSERT INTO minco_idempotency
             (key, fingerprint, state, lease_id, started_at)
             VALUES ($1, $2, 'in_progress', $3, $4)
             ON CONFLICT(key) DO NOTHING",
        )
        .bind(key.as_str())
        .bind(fingerprint.as_str())
        .bind(lease_id)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(idempotency_store_error)?
        .rows_affected()
            == 1;
        if inserted {
            transaction
                .commit()
                .await
                .map_err(idempotency_store_error)?;
            return Ok(BeginOutcome::Started(IdempotencyLease {
                key,
                fingerprint,
                lease_id,
                started_at: now,
            }));
        }
        let row = sqlx::query(
            "SELECT fingerprint, state, started_at, response, completed_at
             FROM minco_idempotency WHERE key = $1 FOR UPDATE",
        )
        .bind(key.as_str())
        .fetch_one(&mut *transaction)
        .await
        .map_err(idempotency_store_error)?;
        let stored_fingerprint = RequestFingerprint::parse(
            row.try_get::<String, _>("fingerprint")
                .map_err(idempotency_store_error)?,
        )?;
        if stored_fingerprint != fingerprint {
            transaction
                .commit()
                .await
                .map_err(idempotency_store_error)?;
            return Ok(BeginOutcome::Conflict);
        }
        let state: String = row.try_get("state").map_err(idempotency_store_error)?;
        if state == "completed" {
            let record = decode_completed(&row)?;
            transaction
                .commit()
                .await
                .map_err(idempotency_store_error)?;
            return Ok(BeginOutcome::Replay(record));
        }
        let started_at: DateTime<Utc> =
            row.try_get("started_at").map_err(idempotency_store_error)?;
        if started_at > now - stale_after {
            transaction
                .commit()
                .await
                .map_err(idempotency_store_error)?;
            return Ok(BeginOutcome::InProgress { started_at });
        }
        sqlx::query(
            "UPDATE minco_idempotency
             SET lease_id = $2, started_at = $3, response = NULL, completed_at = NULL
             WHERE key = $1 AND state = 'in_progress'",
        )
        .bind(key.as_str())
        .bind(lease_id)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(idempotency_store_error)?;
        transaction
            .commit()
            .await
            .map_err(idempotency_store_error)?;
        Ok(BeginOutcome::Started(IdempotencyLease {
            key,
            fingerprint,
            lease_id,
            started_at: now,
        }))
    }

    async fn complete(
        &self,
        lease: IdempotencyLease,
        response: serde_json::Value,
        completed_at: DateTime<Utc>,
    ) -> Result<IdempotencyRecord, IdempotencyError> {
        let result = sqlx::query(
            "UPDATE minco_idempotency
             SET state = 'completed', response = $4, completed_at = $5, lease_id = NULL
             WHERE key = $1 AND fingerprint = $2 AND state = 'in_progress' AND lease_id = $3",
        )
        .bind(lease.key.as_str())
        .bind(lease.fingerprint.as_str())
        .bind(lease.lease_id)
        .bind(response.clone())
        .bind(completed_at)
        .execute(&self.pool)
        .await
        .map_err(idempotency_store_error)?;
        if result.rows_affected() != 1 {
            return Err(IdempotencyError::InvalidLease);
        }
        Ok(IdempotencyRecord {
            fingerprint: lease.fingerprint,
            response,
            created_at: completed_at,
        })
    }

    async fn abort(&self, lease: &IdempotencyLease) -> Result<bool, IdempotencyError> {
        let result = sqlx::query(
            "DELETE FROM minco_idempotency
             WHERE key = $1 AND fingerprint = $2 AND state = 'in_progress' AND lease_id = $3",
        )
        .bind(lease.key.as_str())
        .bind(lease.fingerprint.as_str())
        .bind(lease.lease_id)
        .execute(&self.pool)
        .await
        .map_err(idempotency_store_error)?;
        Ok(result.rows_affected() == 1)
    }
}

fn decode_completed(row: &sqlx::postgres::PgRow) -> Result<IdempotencyRecord, IdempotencyError> {
    Ok(IdempotencyRecord {
        fingerprint: RequestFingerprint::parse(
            row.try_get::<String, _>("fingerprint")
                .map_err(idempotency_store_error)?,
        )?,
        response: row.try_get("response").map_err(idempotency_store_error)?,
        created_at: row
            .try_get("completed_at")
            .map_err(idempotency_store_error)?,
    })
}

#[derive(Debug, Clone)]
pub struct PostgresAuditSink {
    pool: PgPool,
}

impl PostgresAuditSink {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditSink for PostgresAuditSink {
    async fn append(&self, event: AuditEvent) -> Result<(), AuditError> {
        if event.action.trim().is_empty() || event.resource_id.trim().is_empty() {
            return Err(AuditError::InvalidEvent);
        }
        let metadata = serde_json::to_value(event.metadata)
            .map_err(|error| AuditError::Append(error.to_string()))?;
        sqlx::query(
            "INSERT INTO minco_audit
             (id, action, resource_type, resource_id, actor_subject, correlation_id,
              occurred_at, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(event.id)
        .bind(event.action)
        .bind(event.resource_type)
        .bind(event.resource_id)
        .bind(event.actor_subject)
        .bind(event.correlation_id)
        .bind(event.occurred_at)
        .bind(metadata)
        .execute(&self.pool)
        .await
        .map_err(|error| AuditError::Append(error.to_string()))?;
        Ok(())
    }
}

pub async fn migrate_plugin_storage(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    let mut migrator = sqlx::migrate!("migrations/plugins");
    migrator.dangerous_set_table_name("_minco_plugin_storage_migrations");
    migrator.run(pool).await
}

fn validate_event(event: &DomainEvent) -> Result<(), EventError> {
    if event.event_type.trim().is_empty()
        || event.aggregate_type.trim().is_empty()
        || event.aggregate_id.trim().is_empty()
    {
        Err(EventError::InvalidEvent)
    } else {
        Ok(())
    }
}

fn validate_claim(worker_id: &str, claim_expires_at: DateTime<Utc>) -> Result<(), EventError> {
    if worker_id.trim().is_empty() || claim_expires_at <= Utc::now() {
        Err(EventError::InvalidClaim)
    } else {
        Ok(())
    }
}

fn event_infrastructure_error(error: impl std::fmt::Display) -> EventError {
    EventError::Infrastructure(error.to_string())
}

fn session_store_error(error: impl std::fmt::Display) -> SessionError {
    SessionError::Store(error.to_string())
}

fn idempotency_store_error(error: impl std::fmt::Display) -> IdempotencyError {
    IdempotencyError::Store(error.to_string())
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_plugin_sessions::{CreateSession, SessionService};
    use std::{
        collections::{BTreeMap, BTreeSet},
        sync::{Arc, OnceLock},
    };

    fn test_lock() -> &'static tokio::sync::Mutex<()> {
        static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
    }

    async fn pool() -> Option<PgPool> {
        let url = std::env::var("MINCO_TEST_POSTGRES_URL").ok()?;
        let pool = PgPool::connect(&url).await.ok()?;
        migrate_plugin_storage(&pool).await.ok()?;
        Some(pool)
    }

    #[tokio::test]
    async fn postgres_plugin_stores_are_behavioral_when_database_is_configured() {
        let _guard = test_lock().lock().await;
        let Some(pool) = pool().await else {
            eprintln!("MINCO_TEST_POSTGRES_URL not set; PostgreSQL adapter proof skipped");
            return;
        };
        sqlx::raw_sql(
            "TRUNCATE minco_outbox, minco_sessions, minco_idempotency, minco_audit, \
             minco_job_publications, minco_jobs, minco_job_locks RESTART IDENTITY",
        )
        .execute(&pool)
        .await
        .unwrap();
        let migration_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _minco_plugin_storage_migrations")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(migration_count, 3);

        let outbox = PostgresOutboxStore::new(pool.clone());
        let event = DomainEvent::new(
            "feedback.created",
            "feedback",
            "one",
            Uuid::now_v7(),
            serde_json::json!({"id": "one"}),
        );
        outbox
            .enqueue(OutboxRecord::pending(event.clone()))
            .await
            .unwrap();
        let claimed = outbox
            .claim_pending("worker-a", 10, Utc::now() + TimeDelta::minutes(1))
            .await
            .unwrap();
        assert_eq!(claimed.len(), 1);
        outbox.mark_published(event.id, "worker-a").await.unwrap();

        let session_store = Arc::new(PostgresSessionStore::new(pool.clone()));
        let sessions = SessionService::new(session_store.clone());
        let issued = sessions
            .issue(CreateSession {
                subject: "postgres-subject".into(),
                ttl: TimeDelta::minutes(5),
                attributes: BTreeMap::new(),
            })
            .await
            .unwrap();
        assert_eq!(
            sessions.resolve(&issued.token).await.unwrap().subject,
            "postgres-subject"
        );
        assert_eq!(
            session_store
                .revoke_subject("postgres-subject", Utc::now())
                .await
                .unwrap(),
            1
        );
        assert!(matches!(
            sessions.resolve(&issued.token).await,
            Err(SessionError::Unauthenticated)
        ));

        let idempotency = PostgresIdempotencyStore::new(pool.clone());
        let key = IdempotencyKey::parse("postgres-request").unwrap();
        let fingerprint =
            RequestFingerprint::from_serializable(&serde_json::json!({"request": 1})).unwrap();
        let BeginOutcome::Started(lease) = idempotency
            .begin(
                key.clone(),
                fingerprint.clone(),
                Utc::now(),
                TimeDelta::minutes(5),
            )
            .await
            .unwrap()
        else {
            panic!("expected idempotency lease");
        };
        idempotency
            .complete(lease, serde_json::json!({"status": 201}), Utc::now())
            .await
            .unwrap();
        assert!(matches!(
            idempotency
                .begin(key, fingerprint, Utc::now(), TimeDelta::minutes(5))
                .await
                .unwrap(),
            BeginOutcome::Replay(_)
        ));

        PostgresAuditSink::new(pool.clone())
            .append(AuditEvent::new(
                "feedback.created",
                "feedback",
                "one",
                Uuid::now_v7(),
            ))
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM minco_audit")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn enqueue_in_rolls_back_with_the_callers_transaction() {
        let _guard = test_lock().lock().await;
        let Some(pool) = pool().await else {
            eprintln!("MINCO_TEST_POSTGRES_URL not set; PostgreSQL adapter proof skipped");
            return;
        };
        sqlx::query("TRUNCATE minco_outbox")
            .execute(&pool)
            .await
            .unwrap();

        let store = PostgresOutboxStore::new(pool.clone());
        let event = DomainEvent::new(
            "feedback.created",
            "feedback",
            "rollback",
            Uuid::now_v7(),
            serde_json::json!({"id": "rollback"}),
        );
        let mut transaction = pool.begin().await.unwrap();
        store
            .enqueue_in(&mut transaction, OutboxRecord::pending(event.clone()))
            .await
            .unwrap();
        transaction.rollback().await.unwrap();

        let persisted: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM minco_outbox WHERE event_id = $1)")
                .bind(event.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!persisted);
    }

    #[tokio::test]
    async fn concurrent_outbox_claims_are_disjoint() {
        let _guard = test_lock().lock().await;
        let Some(pool) = pool().await else {
            eprintln!("MINCO_TEST_POSTGRES_URL not set; PostgreSQL adapter proof skipped");
            return;
        };
        sqlx::query("TRUNCATE minco_outbox")
            .execute(&pool)
            .await
            .unwrap();

        let store = PostgresOutboxStore::new(pool.clone());
        for aggregate_id in ["claim-one", "claim-two"] {
            store
                .enqueue(OutboxRecord::pending(DomainEvent::new(
                    "feedback.created",
                    "feedback",
                    aggregate_id,
                    Uuid::now_v7(),
                    serde_json::json!({"id": aggregate_id}),
                )))
                .await
                .unwrap();
        }
        let expires_at = Utc::now() + TimeDelta::minutes(1);
        let (first, second) = tokio::join!(
            store.claim_pending("worker-one", 1, expires_at),
            store.claim_pending("worker-two", 1, expires_at),
        );
        let first = first.unwrap();
        let second = second.unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        let claimed = first
            .into_iter()
            .chain(second)
            .map(|record| record.event.id)
            .collect::<BTreeSet<_>>();
        assert_eq!(claimed.len(), 2);
    }

    #[tokio::test]
    async fn concurrent_idempotency_begin_has_one_owner() {
        let _guard = test_lock().lock().await;
        let Some(pool) = pool().await else {
            eprintln!("MINCO_TEST_POSTGRES_URL not set; PostgreSQL adapter proof skipped");
            return;
        };
        sqlx::query("TRUNCATE minco_idempotency")
            .execute(&pool)
            .await
            .unwrap();

        let store = PostgresIdempotencyStore::new(pool);
        let key = IdempotencyKey::parse("postgres-concurrent-request").unwrap();
        let fingerprint =
            RequestFingerprint::from_serializable(&serde_json::json!({"request": 2})).unwrap();
        let now = Utc::now();
        let (first, second) = tokio::join!(
            store.begin(key.clone(), fingerprint.clone(), now, TimeDelta::minutes(5),),
            store.begin(key, fingerprint, now, TimeDelta::minutes(5)),
        );
        let outcomes = [first.unwrap(), second.unwrap()];
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(outcome, BeginOutcome::Started(_)))
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(outcome, BeginOutcome::InProgress { .. }))
                .count(),
            1
        );
    }
}
