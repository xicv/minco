use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use minco_plugin_audit::{
    AuditAppendReport, AuditCursor, AuditJournalEntry, AuditJournalStatus, AuditJournalStore,
    AuditLedgerError, AuditLedgerWriter, AuditLifecyclePolicy, AuditPage, AuditQuery, AuditReader,
    AuditRecordV2, AuditSegmentState, AuditSegmentStatus, AuditStorageHealth,
    AuditStorageInspector, AuditStorageSnapshot, evaluate_storage_health,
};
use sqlx::{PgPool, Postgres, QueryBuilder, Row, Transaction, postgres::PgRow};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PostgresAuditJournal {
    pool: PgPool,
}

impl PostgresAuditJournal {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Inserts an audit intent into the caller's domain transaction.
    pub async fn enqueue_in(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        entry: AuditJournalEntry,
    ) -> Result<(), AuditLedgerError> {
        validate_pending_entry(&entry)?;
        let record = serde_json::to_value(&entry.record).map_err(|_| AuditLedgerError::Encoding)?;
        let encoded_bytes = i32::try_from(entry.encoded_bytes)
            .map_err(|_| AuditLedgerError::InvalidJournalEntry)?;
        let result = sqlx::query(
            "INSERT INTO minco_audit_journal
             (event_id, occurred_at, record, encoded_bytes, status, attempt_count,
              available_at, claimed_by, claim_expires_at, failure_code)
             VALUES ($1, $2, $3, $4, 'pending', $5, $6, $7, $8, $9)
             ON CONFLICT(event_id) DO NOTHING",
        )
        .bind(entry.record.event_id)
        .bind(entry.record.occurred_at)
        .bind(&record)
        .bind(encoded_bytes)
        .bind(
            i32::try_from(entry.attempt_count)
                .map_err(|_| AuditLedgerError::InvalidJournalEntry)?,
        )
        .bind(entry.available_at)
        .bind(entry.claimed_by)
        .bind(entry.claim_expires_at)
        .bind(entry.failure_code)
        .execute(&mut **transaction)
        .await
        .map_err(infrastructure)?;
        if result.rows_affected() == 1 {
            return Ok(());
        }
        let existing: serde_json::Value =
            sqlx::query_scalar("SELECT record FROM minco_audit_journal WHERE event_id = $1")
                .bind(entry.record.event_id)
                .fetch_one(&mut **transaction)
                .await
                .map_err(infrastructure)?;
        if existing == record {
            Ok(())
        } else {
            Err(AuditLedgerError::EventConflict(entry.record.event_id))
        }
    }

    async fn transition(
        &self,
        event_ids: &[Uuid],
        worker_id: &str,
        transition: JournalTransition<'_>,
    ) -> Result<(), AuditLedgerError> {
        validate_transition(event_ids, worker_id)?;
        let result = match transition {
            JournalTransition::Delivered => {
                sqlx::query(
                    "DELETE FROM minco_audit_journal
                 WHERE event_id = ANY($1) AND status = 'claimed' AND claimed_by = $2",
                )
                .bind(event_ids)
                .bind(worker_id)
                .execute(&self.pool)
                .await
            }
            JournalTransition::Retry {
                failure_code,
                retry_at,
            } => {
                validate_failure_code(failure_code)?;
                sqlx::query(
                    "UPDATE minco_audit_journal
                     SET status = 'failed', available_at = $3, failure_code = $4,
                         claimed_by = NULL, claim_expires_at = NULL
                     WHERE event_id = ANY($1) AND status = 'claimed' AND claimed_by = $2",
                )
                .bind(event_ids)
                .bind(worker_id)
                .bind(retry_at)
                .bind(failure_code)
                .execute(&self.pool)
                .await
            }
            JournalTransition::Quarantine { failure_code } => {
                validate_failure_code(failure_code)?;
                sqlx::query(
                    "UPDATE minco_audit_journal
                     SET status = 'quarantined', failure_code = $3,
                         claimed_by = NULL, claim_expires_at = NULL
                     WHERE event_id = ANY($1) AND status = 'claimed' AND claimed_by = $2",
                )
                .bind(event_ids)
                .bind(worker_id)
                .bind(failure_code)
                .execute(&self.pool)
                .await
            }
        }
        .map_err(infrastructure)?;
        if usize::try_from(result.rows_affected()).ok() != Some(event_ids.len()) {
            return Err(AuditLedgerError::JournalClaimLost);
        }
        Ok(())
    }
}

enum JournalTransition<'a> {
    Delivered,
    Retry {
        failure_code: &'a str,
        retry_at: DateTime<Utc>,
    },
    Quarantine {
        failure_code: &'a str,
    },
}

#[async_trait]
impl AuditJournalStore for PostgresAuditJournal {
    async fn enqueue(&self, entry: AuditJournalEntry) -> Result<(), AuditLedgerError> {
        let mut transaction = self.pool.begin().await.map_err(infrastructure)?;
        self.enqueue_in(&mut transaction, entry).await?;
        transaction.commit().await.map_err(infrastructure)
    }

    async fn claim_pending(
        &self,
        worker_id: &str,
        limit: usize,
        claim_expires_at: DateTime<Utc>,
    ) -> Result<Vec<AuditJournalEntry>, AuditLedgerError> {
        validate_claim(worker_id, limit, claim_expires_at)?;
        let limit = i64::try_from(limit).map_err(|_| AuditLedgerError::InvalidJournalClaim)?;
        sqlx::query(
            "WITH claimable AS (
                 SELECT event_id FROM minco_audit_journal
                 WHERE status IN ('pending', 'failed') AND available_at <= $1
                 ORDER BY available_at, occurred_at, event_id
                 FOR UPDATE SKIP LOCKED LIMIT $2
             )
             UPDATE minco_audit_journal AS journal
             SET status = 'claimed', claimed_by = $3, claim_expires_at = $4,
                 attempt_count = journal.attempt_count + 1
             FROM claimable
             WHERE journal.event_id = claimable.event_id
             RETURNING journal.*",
        )
        .bind(Utc::now())
        .bind(limit)
        .bind(worker_id)
        .bind(claim_expires_at)
        .fetch_all(&self.pool)
        .await
        .map_err(infrastructure)?
        .iter()
        .map(decode_journal_entry)
        .collect()
    }

    async fn mark_delivered(
        &self,
        event_ids: &[Uuid],
        worker_id: &str,
    ) -> Result<(), AuditLedgerError> {
        self.transition(event_ids, worker_id, JournalTransition::Delivered)
            .await
    }

    async fn mark_retry(
        &self,
        event_ids: &[Uuid],
        worker_id: &str,
        failure_code: &str,
        retry_at: DateTime<Utc>,
    ) -> Result<(), AuditLedgerError> {
        self.transition(
            event_ids,
            worker_id,
            JournalTransition::Retry {
                failure_code,
                retry_at,
            },
        )
        .await
    }

    async fn quarantine(
        &self,
        event_ids: &[Uuid],
        worker_id: &str,
        failure_code: &str,
    ) -> Result<(), AuditLedgerError> {
        self.transition(
            event_ids,
            worker_id,
            JournalTransition::Quarantine { failure_code },
        )
        .await
    }

    async fn recover_expired_claims(&self, now: DateTime<Utc>) -> Result<usize, AuditLedgerError> {
        let result = sqlx::query(
            "UPDATE minco_audit_journal
             SET status = 'failed', available_at = $1, claimed_by = NULL,
                 claim_expires_at = NULL, failure_code = 'AUDIT-CLAIM-EXPIRED'
             WHERE status = 'claimed' AND claim_expires_at <= $1",
        )
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(infrastructure)?;
        usize::try_from(result.rows_affected()).map_err(|_| AuditLedgerError::Infrastructure)
    }
}

fn decode_journal_entry(row: &PgRow) -> Result<AuditJournalEntry, AuditLedgerError> {
    let value: serde_json::Value = row.try_get("record").map_err(infrastructure)?;
    let status: String = row.try_get("status").map_err(infrastructure)?;
    let record: AuditRecordV2 =
        serde_json::from_value(value).map_err(|_| AuditLedgerError::Encoding)?;
    let encoded_bytes: i32 = row.try_get("encoded_bytes").map_err(infrastructure)?;
    let attempt_count: i32 = row.try_get("attempt_count").map_err(infrastructure)?;
    Ok(AuditJournalEntry {
        record,
        status: decode_status(&status)?,
        attempt_count: u32::try_from(attempt_count)
            .map_err(|_| AuditLedgerError::Infrastructure)?,
        encoded_bytes: usize::try_from(encoded_bytes)
            .map_err(|_| AuditLedgerError::Infrastructure)?,
        available_at: row.try_get("available_at").map_err(infrastructure)?,
        claimed_by: row.try_get("claimed_by").map_err(infrastructure)?,
        claim_expires_at: row.try_get("claim_expires_at").map_err(infrastructure)?,
        failure_code: row.try_get("failure_code").map_err(infrastructure)?,
    })
}

fn decode_status(value: &str) -> Result<AuditJournalStatus, AuditLedgerError> {
    match value {
        "pending" => Ok(AuditJournalStatus::Pending),
        "claimed" => Ok(AuditJournalStatus::Claimed),
        "failed" => Ok(AuditJournalStatus::Failed),
        "quarantined" => Ok(AuditJournalStatus::Quarantined),
        _ => Err(AuditLedgerError::Infrastructure),
    }
}

#[derive(Debug, Clone)]
pub struct PostgresAuditLedger {
    pool: PgPool,
}

impl PostgresAuditLedger {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditLedgerWriter for PostgresAuditLedger {
    async fn append_batch(
        &self,
        records: &[AuditRecordV2],
    ) -> Result<AuditAppendReport, AuditLedgerError> {
        let prepared = prepare_batch(records)?;
        let mut transaction = self.pool.begin().await.map_err(infrastructure)?;
        let ids = prepared.keys().copied().collect::<Vec<_>>();
        let existing = fetch_existing_records(&mut transaction, &ids).await?;
        let mut new = Vec::new();
        let mut duplicates = records.len().saturating_sub(prepared.len());
        for (event_id, item) in &prepared {
            match existing.get(event_id) {
                Some(value) if value == &item.1 => duplicates += 1,
                Some(_) => return Err(AuditLedgerError::EventConflict(*event_id)),
                None => new.push(item),
            }
        }
        let inserted = if new.is_empty() {
            BTreeSet::new()
        } else {
            let inserted = insert_records(&mut transaction, &new).await?;
            if inserted.len() != new.len() {
                let raced_ids = new
                    .iter()
                    .filter(|item| !inserted.contains(&item.0.event_id))
                    .map(|item| item.0.event_id)
                    .collect::<Vec<_>>();
                let raced = fetch_existing_records(&mut transaction, &raced_ids).await?;
                for item in &new {
                    if !inserted.contains(&item.0.event_id)
                        && raced.get(&item.0.event_id) != Some(&item.1)
                    {
                        return Err(AuditLedgerError::EventConflict(item.0.event_id));
                    }
                }
                duplicates += raced_ids.len();
            }
            insert_related_records(&mut transaction, &new, &inserted).await?;
            inserted
        };
        transaction.commit().await.map_err(infrastructure)?;
        Ok(AuditAppendReport {
            requested: records.len(),
            inserted: inserted.len(),
            duplicates,
        })
    }
}

type PreparedBatch = BTreeMap<Uuid, (AuditRecordV2, serde_json::Value, usize)>;

fn prepare_batch(records: &[AuditRecordV2]) -> Result<PreparedBatch, AuditLedgerError> {
    if records.is_empty() || records.len() > minco_plugin_audit::MAX_AUDIT_BATCH_RECORDS {
        return Err(AuditLedgerError::InvalidBatch(
            "invalid record count".into(),
        ));
    }
    let mut prepared = BTreeMap::new();
    let mut bytes = 0usize;
    for record in records {
        let encoded_bytes = record.validate()?;
        bytes = bytes
            .checked_add(encoded_bytes)
            .ok_or_else(|| AuditLedgerError::InvalidBatch("batch bytes overflow".into()))?;
        if bytes > minco_plugin_audit::MAX_AUDIT_BATCH_BYTES {
            return Err(AuditLedgerError::BatchTooLarge {
                bytes,
                maximum: minco_plugin_audit::MAX_AUDIT_BATCH_BYTES,
            });
        }
        let value = serde_json::to_value(record).map_err(|_| AuditLedgerError::Encoding)?;
        if let Some(existing) =
            prepared.insert(record.event_id, (record.clone(), value, encoded_bytes))
            && existing.0 != *record
        {
            return Err(AuditLedgerError::EventConflict(record.event_id));
        }
    }
    Ok(prepared)
}

async fn fetch_existing_records(
    transaction: &mut Transaction<'_, Postgres>,
    ids: &[Uuid],
) -> Result<BTreeMap<Uuid, serde_json::Value>, AuditLedgerError> {
    sqlx::query("SELECT event_id, record FROM minco_audit_records WHERE event_id = ANY($1)")
        .bind(ids)
        .fetch_all(&mut **transaction)
        .await
        .map_err(infrastructure)?
        .iter()
        .map(|row| {
            Ok((
                row.try_get("event_id").map_err(infrastructure)?,
                row.try_get("record").map_err(infrastructure)?,
            ))
        })
        .collect()
}

async fn insert_records(
    transaction: &mut Transaction<'_, Postgres>,
    records: &[&(AuditRecordV2, serde_json::Value, usize)],
) -> Result<BTreeSet<Uuid>, AuditLedgerError> {
    let mut insert = QueryBuilder::<Postgres>::new(
        "INSERT INTO minco_audit_records
         (event_id, tenant_scope, resource_type, resource_id, occurred_at,
          recorded_at, encoded_bytes, record) ",
    );
    insert.push_values(records, |mut row, item| {
        row.push_bind(item.0.event_id)
            .push_bind(&item.0.tenant_scope)
            .push_bind(&item.0.resource.resource_type)
            .push_bind(&item.0.resource.resource_id)
            .push_bind(item.0.occurred_at)
            .push_bind(item.0.recorded_at)
            .push_bind(i32::try_from(item.2).expect("validated audit record size"))
            .push_bind(&item.1);
    });
    insert.push(" ON CONFLICT(event_id) DO NOTHING RETURNING event_id");
    insert
        .build()
        .fetch_all(&mut **transaction)
        .await
        .map_err(infrastructure)?
        .iter()
        .map(|row| row.try_get("event_id").map_err(infrastructure))
        .collect()
}

async fn insert_related_records(
    transaction: &mut Transaction<'_, Postgres>,
    records: &[&(AuditRecordV2, serde_json::Value, usize)],
    inserted: &BTreeSet<Uuid>,
) -> Result<(), AuditLedgerError> {
    let related = records
        .iter()
        .filter(|item| inserted.contains(&item.0.event_id))
        .flat_map(|item| {
            item.0
                .related_resources
                .iter()
                .map(move |related| (&item.0, related))
        })
        .collect::<Vec<_>>();
    if related.is_empty() {
        return Ok(());
    }
    let mut insert = QueryBuilder::<Postgres>::new(
        "INSERT INTO minco_audit_related_resources
         (event_id, tenant_scope, relation, resource_type, resource_id, occurred_at) ",
    );
    insert.push_values(related, |mut row, (record, related)| {
        row.push_bind(record.event_id)
            .push_bind(&record.tenant_scope)
            .push_bind(&related.relation)
            .push_bind(&related.resource.resource_type)
            .push_bind(&related.resource.resource_id)
            .push_bind(record.occurred_at);
    });
    insert
        .build()
        .execute(&mut **transaction)
        .await
        .map_err(infrastructure)?;
    Ok(())
}

#[async_trait]
impl AuditReader for PostgresAuditLedger {
    async fn list_resource_history(
        &self,
        query: &AuditQuery,
    ) -> Result<AuditPage, AuditLedgerError> {
        query.validate()?;
        let mut statement = QueryBuilder::<Postgres>::new(
            "SELECT record, occurred_at, event_id FROM minco_audit_records AS audit WHERE tenant_scope = ",
        );
        statement
            .push_bind(&query.tenant_scope)
            .push(" AND ((resource_type = ")
            .push_bind(&query.resource.resource_type)
            .push(" AND resource_id = ")
            .push_bind(&query.resource.resource_id)
            .push(")");
        if query.include_related {
            statement.push(
                " OR EXISTS (SELECT 1 FROM minco_audit_related_resources AS related
                 WHERE related.event_id = audit.event_id AND related.tenant_scope = ",
            );
            statement
                .push_bind(&query.tenant_scope)
                .push(" AND related.resource_type = ")
                .push_bind(&query.resource.resource_type)
                .push(" AND related.resource_id = ")
                .push_bind(&query.resource.resource_id);
            if let Some(relation) = &query.relation {
                statement
                    .push(" AND related.relation = ")
                    .push_bind(relation);
            }
            statement.push(")");
        }
        statement.push(")");
        if let Some(after) = query.after {
            let comparator = match query.direction {
                minco_plugin_audit::AuditSortDirection::OldestFirst => ">",
                minco_plugin_audit::AuditSortDirection::NewestFirst => "<",
            };
            statement
                .push(" AND (occurred_at, event_id) ")
                .push(comparator)
                .push(" (")
                .push_bind(after.occurred_at)
                .push(", ")
                .push_bind(after.event_id)
                .push(")");
        }
        let direction = match query.direction {
            minco_plugin_audit::AuditSortDirection::OldestFirst => "ASC",
            minco_plugin_audit::AuditSortDirection::NewestFirst => "DESC",
        };
        statement
            .push(" ORDER BY occurred_at ")
            .push(direction)
            .push(", event_id ")
            .push(direction)
            .push(" LIMIT ")
            .push_bind(
                i64::try_from(query.limit + 1)
                    .map_err(|_| AuditLedgerError::InvalidQuery("limit".into()))?,
            );
        let rows = statement
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(infrastructure)?;
        decode_page(rows, query.limit)
    }
}

fn decode_page(rows: Vec<PgRow>, limit: usize) -> Result<AuditPage, AuditLedgerError> {
    let has_more = rows.len() > limit;
    let records = rows
        .into_iter()
        .take(limit)
        .map(|row| {
            let value: serde_json::Value = row.try_get("record").map_err(infrastructure)?;
            let record: AuditRecordV2 =
                serde_json::from_value(value).map_err(|_| AuditLedgerError::Encoding)?;
            record.validate()?;
            Ok(record)
        })
        .collect::<Result<Vec<_>, AuditLedgerError>>()?;
    let next_cursor = has_more.then(|| {
        records
            .last()
            .map(AuditCursor::from)
            .expect("positive validated query limit")
    });
    Ok(AuditPage {
        records,
        next_cursor,
    })
}

#[derive(Debug, Clone)]
pub struct PostgresAuditStorageInspector {
    source: PgPool,
    ledger: PgPool,
    policy: AuditLifecyclePolicy,
}

impl PostgresAuditStorageInspector {
    pub fn new(
        source: PgPool,
        ledger: PgPool,
        policy: AuditLifecyclePolicy,
    ) -> Result<Self, AuditLedgerError> {
        policy.validate()?;
        Ok(Self {
            source,
            ledger,
            policy,
        })
    }
}

#[async_trait]
impl AuditStorageInspector for PostgresAuditStorageInspector {
    async fn storage_health(&self) -> Result<AuditStorageHealth, AuditLedgerError> {
        let hot_bytes: i64 = sqlx::query_scalar(
            "SELECT pg_total_relation_size('minco_audit_records')
                    + pg_total_relation_size('minco_audit_related_resources')",
        )
        .fetch_one(&self.ledger)
        .await
        .map_err(infrastructure)?;
        let row = sqlx::query(
            "SELECT COUNT(*) AS pending_records,
                    COALESCE(SUM(encoded_bytes), 0) AS pending_bytes,
                    MIN(occurred_at) AS oldest_pending
             FROM minco_audit_journal WHERE status IN ('pending', 'failed', 'claimed')",
        )
        .fetch_one(&self.source)
        .await
        .map_err(infrastructure)?;
        let pending_records: i64 = row.try_get("pending_records").map_err(infrastructure)?;
        let pending_bytes: i64 = row.try_get("pending_bytes").map_err(infrastructure)?;
        let oldest_pending: Option<DateTime<Utc>> =
            row.try_get("oldest_pending").map_err(infrastructure)?;
        let quarantined: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM minco_audit_journal WHERE status = 'quarantined'",
        )
        .fetch_one(&self.source)
        .await
        .map_err(infrastructure)?;
        let record_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM minco_audit_records")
            .fetch_one(&self.ledger)
            .await
            .map_err(infrastructure)?;
        let bounds = ledger_bounds(&self.ledger).await?;
        let hot_bytes = u64::try_from(hot_bytes).map_err(|_| AuditLedgerError::Infrastructure)?;
        let snapshot = AuditStorageSnapshot {
            provider: "postgresql".into(),
            hot_bytes,
            free_bytes: None,
            pending_records: u64::try_from(pending_records)
                .map_err(|_| AuditLedgerError::Infrastructure)?,
            pending_bytes: u64::try_from(pending_bytes)
                .map_err(|_| AuditLedgerError::Infrastructure)?,
            oldest_pending_seconds: oldest_pending.map(|time| {
                u64::try_from((Utc::now() - time).num_seconds().max(0)).unwrap_or(u64::MAX)
            }),
            quarantined_records: u64::try_from(quarantined)
                .map_err(|_| AuditLedgerError::Infrastructure)?,
            archive_watermark: None,
            segments: vec![AuditSegmentStatus {
                segment_id: 1,
                state: AuditSegmentState::Active,
                record_count: u64::try_from(record_count)
                    .map_err(|_| AuditLedgerError::Infrastructure)?,
                encoded_bytes: hot_bytes,
                first: bounds.0,
                last: bounds.1,
                archive_receipt: None,
            }],
        };
        evaluate_storage_health(self.policy, snapshot)
    }
}

async fn ledger_bounds(
    pool: &PgPool,
) -> Result<(Option<AuditCursor>, Option<AuditCursor>), AuditLedgerError> {
    let first = sqlx::query(
        "SELECT occurred_at, event_id FROM minco_audit_records ORDER BY occurred_at, event_id LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(infrastructure)?
    .map(|row| decode_cursor(&row))
    .transpose()?;
    let last = sqlx::query(
        "SELECT occurred_at, event_id FROM minco_audit_records ORDER BY occurred_at DESC, event_id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(infrastructure)?
    .map(|row| decode_cursor(&row))
    .transpose()?;
    Ok((first, last))
}

fn decode_cursor(row: &PgRow) -> Result<AuditCursor, AuditLedgerError> {
    Ok(AuditCursor {
        occurred_at: row.try_get("occurred_at").map_err(infrastructure)?,
        event_id: row.try_get("event_id").map_err(infrastructure)?,
    })
}

/// Rejects accidental use of the operational database as the permanent ledger.
pub async fn validate_separate_audit_pools(
    source: &PgPool,
    ledger: &PgPool,
) -> Result<(), AuditLedgerError> {
    let source_identity = database_identity(source).await?;
    let ledger_identity = database_identity(ledger).await?;
    if source_identity == ledger_identity {
        return Err(AuditLedgerError::InvalidLifecycle(
            "PostgreSQL audit ledger requires a distinct database".into(),
        ));
    }
    Ok(())
}

async fn database_identity(
    pool: &PgPool,
) -> Result<(String, Option<String>, Option<i32>), AuditLedgerError> {
    let row = sqlx::query(
        "SELECT current_database() AS database,
                inet_server_addr()::text AS server_address,
                inet_server_port() AS server_port",
    )
    .fetch_one(pool)
    .await
    .map_err(infrastructure)?;
    Ok((
        row.try_get("database").map_err(infrastructure)?,
        row.try_get("server_address").map_err(infrastructure)?,
        row.try_get("server_port").map_err(infrastructure)?,
    ))
}

pub async fn migrate_audit_ledger(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    let mut migrator = sqlx::migrate!("migrations/audit-ledger");
    migrator.dangerous_set_table_name("_minco_audit_ledger_migrations");
    migrator.run(pool).await
}

fn validate_pending_entry(entry: &AuditJournalEntry) -> Result<(), AuditLedgerError> {
    if entry.status != AuditJournalStatus::Pending
        || entry.encoded_bytes != entry.record.validate()?
    {
        Err(AuditLedgerError::InvalidJournalEntry)
    } else {
        Ok(())
    }
}

fn validate_claim(
    worker_id: &str,
    limit: usize,
    claim_expires_at: DateTime<Utc>,
) -> Result<(), AuditLedgerError> {
    let now = Utc::now();
    if worker_id.trim().is_empty()
        || worker_id.len() > 128
        || worker_id.chars().any(char::is_control)
        || limit == 0
        || limit > minco_plugin_audit::MAX_AUDIT_BATCH_RECORDS
        || claim_expires_at <= now
        || claim_expires_at > now + TimeDelta::hours(1)
    {
        Err(AuditLedgerError::InvalidJournalClaim)
    } else {
        Ok(())
    }
}

fn validate_transition(event_ids: &[Uuid], worker_id: &str) -> Result<(), AuditLedgerError> {
    if event_ids.is_empty()
        || event_ids.len() > minco_plugin_audit::MAX_AUDIT_BATCH_RECORDS
        || worker_id.trim().is_empty()
        || worker_id.len() > 128
        || worker_id.chars().any(char::is_control)
    {
        Err(AuditLedgerError::InvalidJournalClaim)
    } else {
        Ok(())
    }
}

fn validate_failure_code(value: &str) -> Result<(), AuditLedgerError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'-')
    {
        Err(AuditLedgerError::InvalidJournalEntry)
    } else {
        Ok(())
    }
}

fn infrastructure(_: impl std::fmt::Display) -> AuditLedgerError {
    AuditLedgerError::Infrastructure
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_plugin_audit::{
        AuditActor, AuditRelatedResource, AuditRelay, AuditResourceRef, AuditSortDirection,
    };
    use std::sync::{Arc, OnceLock};

    fn test_lock() -> &'static tokio::sync::Mutex<()> {
        static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
    }

    async fn databases() -> Option<(PgPool, PgPool)> {
        let source_url = std::env::var("MINCO_TEST_POSTGRES_URL").ok()?;
        let ledger_url = std::env::var("MINCO_TEST_POSTGRES_AUDIT_URL").ok()?;
        let source = PgPool::connect(&source_url).await.ok()?;
        let ledger = PgPool::connect(&ledger_url).await.ok()?;
        crate::plugin_adapters::migrate_plugin_storage(&source)
            .await
            .ok()?;
        migrate_audit_ledger(&ledger).await.ok()?;
        validate_separate_audit_pools(&source, &ledger).await.ok()?;
        Some((source, ledger))
    }

    fn record() -> AuditRecordV2 {
        let mut record = AuditRecordV2::new(
            "tenant",
            "order.status_changed",
            AuditResourceRef::new("order", "one"),
            AuditActor::human("subject"),
            "updateOrder",
            Uuid::now_v7(),
        );
        record.event_id = Uuid::now_v7();
        record
    }

    #[tokio::test]
    async fn transaction_and_crash_retry_are_behavioral_when_two_databases_are_configured() {
        let _guard = test_lock().lock().await;
        let Some((source, ledger)) = databases().await else {
            eprintln!(
                "MINCO_TEST_POSTGRES_URL and MINCO_TEST_POSTGRES_AUDIT_URL must name distinct databases; PostgreSQL audit proof skipped"
            );
            return;
        };
        sqlx::query("DELETE FROM minco_audit_journal")
            .execute(&source)
            .await
            .unwrap();
        sqlx::query("DELETE FROM minco_audit_related_resources")
            .execute(&ledger)
            .await
            .unwrap();
        sqlx::query("DELETE FROM minco_audit_records")
            .execute(&ledger)
            .await
            .unwrap();

        let journal = Arc::new(PostgresAuditJournal::new(source));
        let ledger = Arc::new(PostgresAuditLedger::new(ledger));

        let rolled_back = record();
        let mut transaction = journal.pool.begin().await.unwrap();
        journal
            .enqueue_in(
                &mut transaction,
                AuditJournalEntry::pending(rolled_back.clone()).unwrap(),
            )
            .await
            .unwrap();
        transaction.rollback().await.unwrap();
        let rollback_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM minco_audit_journal WHERE event_id = $1")
                .bind(rolled_back.event_id)
                .fetch_one(&journal.pool)
                .await
                .unwrap();
        assert_eq!(rollback_count, 0);

        let record = record();
        journal
            .enqueue(AuditJournalEntry::pending(record).unwrap())
            .await
            .unwrap();
        let claimed = journal
            .claim_pending(
                "crashed-worker",
                10,
                Utc::now() + TimeDelta::milliseconds(1),
            )
            .await
            .unwrap();
        ledger
            .append_batch(&[claimed[0].record.clone()])
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let report = AuditRelay::new(journal, ledger)
            .dispatch_once("recovery-worker", 10, TimeDelta::minutes(1))
            .await
            .unwrap();
        assert_eq!((report.inserted, report.duplicates), (0, 1));
    }

    #[tokio::test]
    async fn concurrent_claims_and_relationship_query_are_behavioral_when_configured() {
        let _guard = test_lock().lock().await;
        let Some((source, ledger_pool)) = databases().await else {
            eprintln!(
                "MINCO_TEST_POSTGRES_URL and MINCO_TEST_POSTGRES_AUDIT_URL must name distinct databases; PostgreSQL audit proof skipped"
            );
            return;
        };
        sqlx::query("DELETE FROM minco_audit_journal")
            .execute(&source)
            .await
            .unwrap();
        sqlx::query("DELETE FROM minco_audit_related_resources")
            .execute(&ledger_pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM minco_audit_records")
            .execute(&ledger_pool)
            .await
            .unwrap();
        let journal = Arc::new(PostgresAuditJournal::new(source));
        let ledger = PostgresAuditLedger::new(ledger_pool);
        let first = record();
        let second = record();
        journal
            .enqueue(AuditJournalEntry::pending(first).unwrap())
            .await
            .unwrap();
        journal
            .enqueue(AuditJournalEntry::pending(second).unwrap())
            .await
            .unwrap();
        let lease = Utc::now() + TimeDelta::minutes(1);
        let (claim_a, claim_b) = tokio::join!(
            journal.claim_pending("worker-a", 1, lease),
            journal.claim_pending("worker-b", 1, lease)
        );
        let claim_a = claim_a.unwrap();
        let claim_b = claim_b.unwrap();
        assert_eq!((claim_a.len(), claim_b.len()), (1, 1));
        assert_ne!(claim_a[0].record.event_id, claim_b[0].record.event_id);

        let records = claim_a
            .iter()
            .chain(&claim_b)
            .map(|entry| entry.record.clone())
            .collect::<Vec<_>>();
        ledger.append_batch(&records).await.unwrap();
        journal
            .mark_delivered(&[claim_a[0].record.event_id], "worker-a")
            .await
            .unwrap();
        journal
            .mark_delivered(&[claim_b[0].record.event_id], "worker-b")
            .await
            .unwrap();

        let mut related = record();
        related.resource = AuditResourceRef::new("shift", "one");
        related.related_resources.push(AuditRelatedResource {
            relation: "order".into(),
            resource: AuditResourceRef::new("order", "related-one"),
        });
        ledger.append_batch(&[related]).await.unwrap();
        let mut query =
            AuditQuery::for_resource("tenant", AuditResourceRef::new("order", "related-one"));
        query.include_related = true;
        query.relation = Some("order".into());
        query.direction = AuditSortDirection::OldestFirst;
        let page = ledger.list_resource_history(&query).await.unwrap();
        assert_eq!(page.records.len(), 1);
        assert_eq!(page.records[0].resource.resource_type, "shift");
    }
}
