use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use minco_plugin_audit::{AuditError, AuditEvent, AuditSink};
use minco_plugin_idempotency::{
    BeginOutcome, IdempotencyError, IdempotencyKey, IdempotencyLease, IdempotencyRecord,
    IdempotencyStore, RequestFingerprint, validate_claim_timeout,
};
use minco_plugin_sessions::{
    SessionError, SessionId, SessionRecord, SessionStore, SessionTokenHash,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SqliteSessionStore {
    pool: SqlitePool,
}

impl SqliteSessionStore {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SessionStore for SqliteSessionStore {
    async fn create(
        &self,
        token_hash: SessionTokenHash,
        session: SessionRecord,
    ) -> Result<(), SessionError> {
        let attributes = serde_json::to_string(&session.attributes).map_err(session_store_error)?;
        let result = sqlx::query(
            "INSERT INTO minco_sessions
             (id, token_hash, subject, created_at, expires_at, revoked_at, attributes)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
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
             FROM minco_sessions WHERE token_hash = ?",
        )
        .bind(token_hash.as_bytes().as_slice())
        .fetch_optional(&self.pool)
        .await
        .map_err(session_store_error)?;
        row.map(|row| decode_session(&row)).transpose()
    }

    async fn revoke(&self, id: SessionId, at: DateTime<Utc>) -> Result<bool, SessionError> {
        let result = sqlx::query(
            "UPDATE minco_sessions SET revoked_at = ?
             WHERE id = ? AND revoked_at IS NULL",
        )
        .bind(at)
        .bind(id.0)
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
            "UPDATE minco_sessions SET revoked_at = ?
             WHERE subject = ? AND revoked_at IS NULL",
        )
        .bind(at)
        .bind(subject)
        .execute(&self.pool)
        .await
        .map_err(session_store_error)?;
        usize::try_from(result.rows_affected())
            .map_err(|error| session_store_error(error.to_string()))
    }
}

fn decode_session(row: &sqlx::sqlite::SqliteRow) -> Result<SessionRecord, SessionError> {
    let attributes: String = row.try_get("attributes").map_err(session_store_error)?;
    Ok(SessionRecord {
        id: SessionId(row.try_get("id").map_err(session_store_error)?),
        subject: row.try_get("subject").map_err(session_store_error)?,
        created_at: row.try_get("created_at").map_err(session_store_error)?,
        expires_at: row.try_get("expires_at").map_err(session_store_error)?,
        revoked_at: row.try_get("revoked_at").map_err(session_store_error)?,
        attributes: serde_json::from_str(&attributes).map_err(session_store_error)?,
    })
}

#[derive(Debug, Clone)]
pub struct SqliteIdempotencyStore {
    pool: SqlitePool,
}

impl SqliteIdempotencyStore {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IdempotencyStore for SqliteIdempotencyStore {
    async fn get(
        &self,
        key: &IdempotencyKey,
    ) -> Result<Option<IdempotencyRecord>, IdempotencyError> {
        let row = sqlx::query(
            "SELECT fingerprint, response, completed_at
             FROM minco_idempotency WHERE key = ? AND state = 'completed'",
        )
        .bind(key.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(idempotency_store_error)?;
        row.map(|row| decode_completed(&row)).transpose()
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
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(idempotency_store_error)?;
        let inserted = sqlx::query(
            "INSERT INTO minco_idempotency
             (key, fingerprint, state, lease_id, started_at)
             VALUES (?, ?, 'in_progress', ?, ?)
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
            "SELECT fingerprint, state, lease_id, started_at, response, completed_at
             FROM minco_idempotency WHERE key = ?",
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
             SET lease_id = ?, started_at = ?, response = NULL, completed_at = NULL
             WHERE key = ? AND state = 'in_progress'",
        )
        .bind(lease_id)
        .bind(now)
        .bind(key.as_str())
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
        let encoded = serde_json::to_string(&response).map_err(idempotency_store_error)?;
        let result = sqlx::query(
            "UPDATE minco_idempotency
             SET state = 'completed', response = ?, completed_at = ?, lease_id = NULL
             WHERE key = ? AND fingerprint = ? AND state = 'in_progress' AND lease_id = ?",
        )
        .bind(encoded)
        .bind(completed_at)
        .bind(lease.key.as_str())
        .bind(lease.fingerprint.as_str())
        .bind(lease.lease_id)
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
             WHERE key = ? AND fingerprint = ? AND state = 'in_progress' AND lease_id = ?",
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

fn decode_completed(row: &sqlx::sqlite::SqliteRow) -> Result<IdempotencyRecord, IdempotencyError> {
    let response: String = row.try_get("response").map_err(idempotency_store_error)?;
    Ok(IdempotencyRecord {
        fingerprint: RequestFingerprint::parse(
            row.try_get::<String, _>("fingerprint")
                .map_err(idempotency_store_error)?,
        )?,
        response: serde_json::from_str(&response).map_err(idempotency_store_error)?,
        created_at: row
            .try_get("completed_at")
            .map_err(idempotency_store_error)?,
    })
}

#[derive(Debug, Clone)]
pub struct SqliteAuditSink {
    pool: SqlitePool,
}

impl SqliteAuditSink {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditSink for SqliteAuditSink {
    async fn append(&self, event: AuditEvent) -> Result<(), AuditError> {
        if event.action.trim().is_empty() || event.resource_id.trim().is_empty() {
            return Err(AuditError::InvalidEvent);
        }
        let metadata = serde_json::to_string(&event.metadata)
            .map_err(|error| AuditError::Append(error.to_string()))?;
        sqlx::query(
            "INSERT INTO minco_audit
             (id, action, resource_type, resource_id, actor_subject, correlation_id,
              occurred_at, metadata)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
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

pub async fn migrate_plugin_storage(pool: &SqlitePool) -> Result<(), sqlx::migrate::MigrateError> {
    let mut migrator = sqlx::migrate!("migrations/plugins");
    migrator.dangerous_set_table_name("_minco_plugin_storage_migrations");
    migrator.run(pool).await
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
    use minco_plugin_audit::AuditEvent;
    use minco_plugin_idempotency::RequestFingerprint;
    use minco_plugin_sessions::{CreateSession, SessionService};
    use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

    struct TestDatabasePath(PathBuf);

    impl Drop for TestDatabasePath {
        fn drop(&mut self) {
            for suffix in ["", "-shm", "-wal"] {
                let path = PathBuf::from(format!("{}{suffix}", self.0.display()));
                let _ = std::fs::remove_file(path);
            }
        }
    }

    async fn pool() -> SqlitePool {
        let pool = crate::connect(&crate::SqlitePoolConfig::memory())
            .await
            .unwrap();
        migrate_plugin_storage(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn persistent_sessions_resolve_and_revoke() {
        let pool = pool().await;
        let migration_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _minco_plugin_storage_migrations")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(migration_count, 2);
        let service = SessionService::new(Arc::new(SqliteSessionStore::new(pool)));
        let issued = service
            .issue(CreateSession {
                subject: "subject-1".into(),
                ttl: TimeDelta::minutes(5),
                attributes: BTreeMap::new(),
            })
            .await
            .unwrap();
        assert_eq!(
            service.resolve(&issued.token).await.unwrap().subject,
            "subject-1"
        );
        assert!(service.revoke(issued.session.id).await.unwrap());
        assert!(matches!(
            service.resolve(&issued.token).await,
            Err(SessionError::Unauthenticated)
        ));
    }

    #[tokio::test]
    async fn idempotency_leases_replay_and_reject_stale_completion() {
        let store = SqliteIdempotencyStore::new(pool().await);
        let key = IdempotencyKey::parse("sqlite-request").unwrap();
        let fingerprint =
            RequestFingerprint::from_serializable(&serde_json::json!({"request": 1})).unwrap();
        let now = Utc::now();
        let BeginOutcome::Started(stale_lease) = store
            .begin(
                key.clone(),
                fingerprint.clone(),
                now - TimeDelta::minutes(10),
                TimeDelta::minutes(5),
            )
            .await
            .unwrap()
        else {
            panic!("expected a lease");
        };
        let BeginOutcome::Started(current_lease) = store
            .begin(key.clone(), fingerprint.clone(), now, TimeDelta::minutes(5))
            .await
            .unwrap()
        else {
            panic!("expected replacement lease");
        };
        assert!(matches!(
            store
                .complete(stale_lease, serde_json::json!({"status": 409}), now)
                .await,
            Err(IdempotencyError::InvalidLease)
        ));
        store
            .complete(current_lease, serde_json::json!({"status": 201}), now)
            .await
            .unwrap();
        let BeginOutcome::Replay(record) = store
            .begin(key, fingerprint, now, TimeDelta::minutes(5))
            .await
            .unwrap()
        else {
            panic!("expected completed response replay");
        };
        assert_eq!(record.response, serde_json::json!({"status": 201}));
    }

    #[tokio::test]
    async fn file_backed_idempotency_serializes_concurrent_begin() {
        let database_path = TestDatabasePath(
            std::env::temp_dir().join(format!("minco-idempotency-{}.sqlite", Uuid::now_v7())),
        );
        let mut config = crate::SqlitePoolConfig::file(&database_path.0);
        config.max_connections = 2;
        config.acquire_timeout_seconds = 2;
        let pool = crate::connect(&config).await.unwrap();
        migrate_plugin_storage(&pool).await.unwrap();

        let store = SqliteIdempotencyStore::new(pool);
        let key = IdempotencyKey::parse("sqlite-concurrent-request").unwrap();
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

    #[tokio::test]
    async fn audit_is_append_only_with_database_ordering() {
        let pool = pool().await;
        let sink = SqliteAuditSink::new(pool.clone());
        sink.append(AuditEvent::new(
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
}
