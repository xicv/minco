//! Durable `SQLite` job storage: job rows, publication intent, execution
//! leases and overlap locks.
//!
//! The job row owns execution state and the publication row owns pending
//! transport delivery; the queue message is never authoritative. `SQLite` has
//! one writer at a time, so claims run inside `BEGIN IMMEDIATE`
//! transactions and every transition verifies affected rows.

use chrono::{DateTime, Utc};
use minco_plugin_jobs::{
    EnqueueOutcome, JobAttempt, JobError, JobPublication, JobRecord, JobStatus, PublicationStatus,
    validate_worker_claim,
};
use sqlx::{Row, SqlitePool, Transaction, sqlite::SqliteRow};
use uuid::Uuid;

/// SQLite-backed job store implementing every job port.
#[derive(Debug, Clone)]
pub struct SqliteJobStore {
    pool: SqlitePool,
}

impl SqliteJobStore {
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Insert the job and its publication intent inside the caller's
    /// transaction so the business mutation and the durable dispatch commit
    /// atomically.
    pub async fn enqueue_in(
        &self,
        transaction: &mut Transaction<'_, sqlx::Sqlite>,
        record: JobRecord,
    ) -> Result<EnqueueOutcome, JobError> {
        if record.status != JobStatus::Pending {
            return Err(JobError::InvalidJob("jobs must be enqueued pending".into()));
        }
        record.envelope.validate()?;
        let envelope_json = serde_json::to_string(&record.envelope).map_err(|error| {
            JobError::Infrastructure(format!("job envelope encode failed: {error}"))
        })?;
        let attempts = serde_json::to_string(&record.attempts)
            .map_err(|error| JobError::Infrastructure(format!("attempt encode failed: {error}")))?;
        let result = sqlx::query(
            "INSERT INTO minco_jobs (job_id, worker_profile, envelope, status, revision, \
             available_at, attempt_count, lease_owner, lease_expires_at, attempts, dedupe_key, \
             failure_code, completed_at) \
             VALUES ($1, $2, $3, 'pending', $4, $5, $6, NULL, NULL, $7, $8, NULL, NULL)",
        )
        .bind(record.envelope.job_id.to_string())
        .bind(&record.envelope.worker_profile)
        .bind(&envelope_json)
        .bind(i64::try_from(record.revision).unwrap_or(1))
        .bind(record.envelope.available_at.to_rfc3339())
        .bind(i64::from(record.attempt_count))
        .bind(&attempts)
        .bind(record.envelope.dedupe_key.as_deref())
        .execute(&mut **transaction)
        .await;
        let inserted = match result {
            Ok(_) => true,
            Err(error) => {
                let message = format!("{error}");
                if message.contains("minco_jobs.job_id") {
                    return Err(JobError::MissingJob(record.envelope.job_id));
                }
                if message.contains("minco_jobs.dedupe_key") {
                    return dedupe_outcome(&record, transaction).await;
                }
                return Err(infrastructure(&error));
            }
        };
        let _ = inserted;
        sqlx::query(
            "INSERT INTO minco_job_publications (job_id, worker_profile, status, attempt_count, \
             available_at, claimed_by, claim_expires_at, last_error) \
             VALUES ($1, $2, 'pending', 0, $3, NULL, NULL, NULL)",
        )
        .bind(record.envelope.job_id.to_string())
        .bind(&record.envelope.worker_profile)
        .bind(record.envelope.available_at.to_rfc3339())
        .execute(&mut **transaction)
        .await
        .map_err(|_| JobError::Infrastructure("job publication insert failed".into()))?;
        Ok(EnqueueOutcome::Inserted(record.envelope.job_id))
    }
}

async fn dedupe_outcome(
    record: &JobRecord,
    transaction: &mut Transaction<'_, sqlx::Sqlite>,
) -> Result<EnqueueOutcome, JobError> {
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT envelope FROM minco_jobs WHERE dedupe_key = $1")
            .bind(record.envelope.dedupe_key.as_deref())
            .fetch_optional(&mut **transaction)
            .await
            .map_err(|_| JobError::Infrastructure("job dedupe lookup failed".into()))?;
    let Some((existing_envelope,)) = existing else {
        return Err(JobError::Infrastructure(
            "job dedupe conflict vanished during insert".into(),
        ));
    };
    let existing: serde_json::Value = serde_json::from_str(&existing_envelope)
        .map_err(|_| JobError::Infrastructure("existing job envelope decode failed".into()))?;
    let identical = existing.get("job_name") == Some(&serde_json::json!(record.envelope.job_name))
        && existing.get("job_version") == Some(&serde_json::json!(record.envelope.job_version))
        && existing.get("payload") == Some(&record.envelope.payload);
    let existing_id = existing
        .get("job_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse::<Uuid>().ok());
    match (identical, existing_id) {
        (true, Some(existing_job_id)) => Ok(EnqueueOutcome::Duplicate(existing_job_id)),
        (false, Some(existing_job_id)) => {
            Err(JobError::DuplicateSubmissionConflict { existing_job_id })
        }
        _ => Err(JobError::Infrastructure(
            "job dedupe conflict could not be resolved".into(),
        )),
    }
}

fn infrastructure(error: &sqlx::Error) -> JobError {
    JobError::Infrastructure(format!("sqlite job store failed: {error}"))
}

fn parse_status(status: &str) -> Result<JobStatus, JobError> {
    match status {
        "pending" => Ok(JobStatus::Pending),
        "running" => Ok(JobStatus::Running),
        "succeeded" => Ok(JobStatus::Succeeded),
        "failed_permanently" => Ok(JobStatus::FailedPermanently),
        "cancelled" => Ok(JobStatus::Cancelled),
        other => Err(JobError::Infrastructure(format!(
            "unknown job status {other}"
        ))),
    }
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, JobError> {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|_| JobError::Infrastructure("timestamp decode failed".into()))
}

fn optional_timestamp(value: Option<String>) -> Result<Option<DateTime<Utc>>, JobError> {
    value.map(|value| parse_timestamp(&value)).transpose()
}

fn decode_job_row(row: &SqliteRow) -> Result<JobRecord, JobError> {
    let envelope_text: String = row
        .try_get("envelope")
        .map_err(|_| JobError::Infrastructure("job envelope column unreadable".into()))?;
    let mut envelope: minco_plugin_jobs::JobEnvelope = serde_json::from_str(&envelope_text)
        .map_err(|error| {
            JobError::Infrastructure(format!("job envelope decode failed: {error}"))
        })?;
    let status: String = row
        .try_get("status")
        .map_err(|_| JobError::Infrastructure("job status column unreadable".into()))?;
    let attempt_count: i64 = row
        .try_get("attempt_count")
        .map_err(|_| JobError::Infrastructure("job attempt column unreadable".into()))?;
    envelope.available_at = parse_timestamp(
        row.try_get("available_at")
            .map_err(|_| JobError::Infrastructure("job availability unreadable".into()))?,
    )?;
    envelope.attempt = u32::try_from(attempt_count)
        .unwrap_or(envelope.attempt)
        .max(1);
    let attempts_text: String = row
        .try_get("attempts")
        .map_err(|_| JobError::Infrastructure("job attempts unreadable".into()))?;
    let attempts: Vec<JobAttempt> = serde_json::from_str(&attempts_text)
        .map_err(|error| JobError::Infrastructure(format!("attempt decode failed: {error}")))?;
    Ok(JobRecord {
        envelope,
        status: parse_status(&status)?,
        revision: u64::try_from(
            row.try_get::<i64, _>("revision")
                .map_err(|_| JobError::Infrastructure("job revision unreadable".into()))?,
        )
        .unwrap_or(1),
        lease_owner: row
            .try_get("lease_owner")
            .map_err(|_| JobError::Infrastructure("job lease owner unreadable".into()))?,
        lease_expires_at: optional_timestamp(
            row.try_get("lease_expires_at")
                .map_err(|_| JobError::Infrastructure("job lease expiry unreadable".into()))?,
        )?,
        attempt_count: u32::try_from(attempt_count).unwrap_or(0),
        attempts,
        failure_code: row
            .try_get("failure_code")
            .map_err(|_| JobError::Infrastructure("job failure code unreadable".into()))?,
        completed_at: optional_timestamp(
            row.try_get("completed_at")
                .map_err(|_| JobError::Infrastructure("job completion unreadable".into()))?,
        )?,
    })
}

fn attempt_entry(
    attempt: u32,
    worker_id: &str,
    now: DateTime<Utc>,
    outcome: minco_plugin_jobs::JobAttemptOutcome,
) -> Result<String, JobError> {
    serde_json::to_string(&JobAttempt {
        attempt,
        at: now,
        worker_id: worker_id.to_owned(),
        outcome,
    })
    .map_err(|error| JobError::Infrastructure(format!("attempt encode failed: {error}")))
}

fn decode_publication_row(row: &SqliteRow) -> Result<JobPublication, JobError> {
    let status: String = row
        .try_get("status")
        .map_err(|_| JobError::Infrastructure("publication status unreadable".into()))?;
    let status = match status.as_str() {
        "pending" => PublicationStatus::Pending,
        "claimed" => PublicationStatus::Claimed,
        "published" => PublicationStatus::Published,
        "failed" => PublicationStatus::Failed,
        other => {
            return Err(JobError::Infrastructure(format!(
                "unknown publication status {other}"
            )));
        }
    };
    let job_id: String = row
        .try_get("job_id")
        .map_err(|_| JobError::Infrastructure("publication id unreadable".into()))?;
    Ok(JobPublication {
        job_id: Uuid::parse_str(&job_id)
            .map_err(|_| JobError::Infrastructure("publication id decode failed".into()))?,
        worker_profile: row
            .try_get("worker_profile")
            .map_err(|_| JobError::Infrastructure("publication profile unreadable".into()))?,
        status,
        attempt_count: u32::try_from(
            row.try_get::<i64, _>("attempt_count")
                .map_err(|_| JobError::Infrastructure("publication attempts unreadable".into()))?,
        )
        .unwrap_or(0),
        available_at: parse_timestamp(row.try_get("available_at").map_err(|_| {
            JobError::Infrastructure("publication availability unreadable".into())
        })?)?,
        claimed_by: row
            .try_get("claimed_by")
            .map_err(|_| JobError::Infrastructure("publication claimant unreadable".into()))?,
        claim_expires_at: optional_timestamp(row.try_get("claim_expires_at").map_err(|_| {
            JobError::Infrastructure("publication claim expiry unreadable".into())
        })?)?,
        last_error: row
            .try_get("last_error")
            .map_err(|_| JobError::Infrastructure("publication error unreadable".into()))?,
    })
}

async fn verify_job_update(
    pool: &SqlitePool,
    job_id: Uuid,
    worker_id: &str,
    result: &sqlx::sqlite::SqliteQueryResult,
) -> Result<(), JobError> {
    if result.rows_affected() == 1 {
        return Ok(());
    }
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM minco_jobs WHERE job_id = $1)")
            .bind(job_id.to_string())
            .fetch_one(pool)
            .await
            .map_err(|_| JobError::Infrastructure("job existence probe failed".into()))?;
    if exists {
        Err(JobError::LeaseOwnership {
            job_id,
            worker_id: worker_id.to_owned(),
        })
    } else {
        Err(JobError::MissingJob(job_id))
    }
}

async fn verify_revision_guard(
    pool: &SqlitePool,
    job_id: Uuid,
    expected_revision: u64,
    result: &sqlx::sqlite::SqliteQueryResult,
) -> Result<(), JobError> {
    if result.rows_affected() == 1 {
        return Ok(());
    }
    let current: Option<i64> =
        sqlx::query_scalar("SELECT revision FROM minco_jobs WHERE job_id = $1")
            .bind(job_id.to_string())
            .fetch_optional(pool)
            .await
            .map_err(|error| infrastructure(&error))?;
    match current {
        Some(revision) if u64::try_from(revision).unwrap_or(0) != expected_revision => {
            Err(JobError::RevisionConflict {
                job_id,
                expected_revision,
            })
        }
        Some(_) => Err(JobError::InvalidTransition { job_id }),
        None => Err(JobError::MissingJob(job_id)),
    }
}

async fn verify_publication_update(
    pool: &SqlitePool,
    job_id: Uuid,
    worker_id: &str,
    result: &sqlx::sqlite::SqliteQueryResult,
) -> Result<(), JobError> {
    if result.rows_affected() == 1 {
        return Ok(());
    }
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM minco_job_publications WHERE job_id = $1)")
            .bind(job_id.to_string())
            .fetch_one(pool)
            .await
            .map_err(|_| JobError::Infrastructure("publication probe failed".into()))?;
    if exists {
        Err(JobError::LeaseOwnership {
            job_id,
            worker_id: worker_id.to_owned(),
        })
    } else {
        Err(JobError::MissingJob(job_id))
    }
}

#[async_trait::async_trait]
impl minco_plugin_jobs::JobStore for SqliteJobStore {
    async fn enqueue_with_intent(&self, record: JobRecord) -> Result<EnqueueOutcome, JobError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| infrastructure(&error))?;
        let outcome = self.enqueue_in(&mut transaction, record).await?;
        transaction
            .commit()
            .await
            .map_err(|error| infrastructure(&error))?;
        Ok(outcome)
    }

    async fn claim_execution(
        &self,
        job_id: Uuid,
        worker_id: &str,
        lease_expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Option<JobRecord>, JobError> {
        validate_worker_claim(worker_id, 1, lease_expires_at, now)?;
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| infrastructure(&error))?;
        let claimable: Option<i64> = sqlx::query_scalar(
            "SELECT CASE WHEN (status = 'pending' AND available_at <= $2) \
             OR (status = 'running' AND lease_expires_at IS NOT NULL AND lease_expires_at <= $2) \
             THEN 1 ELSE 0 END FROM minco_jobs WHERE job_id = $1",
        )
        .bind(job_id.to_string())
        .bind(now.to_rfc3339())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| infrastructure(&error))?;
        if claimable != Some(1) {
            transaction
                .commit()
                .await
                .map_err(|error| infrastructure(&error))?;
            return Ok(None);
        }
        sqlx::query(
            "UPDATE minco_jobs SET status = 'running', lease_owner = $2, lease_expires_at = $3, \
             attempt_count = attempt_count + 1, revision = revision + 1 WHERE job_id = $1",
        )
        .bind(job_id.to_string())
        .bind(worker_id)
        .bind(lease_expires_at.to_rfc3339())
        .execute(&mut *transaction)
        .await
        .map_err(|error| infrastructure(&error))?;
        let row = sqlx::query(
            "SELECT job_id, worker_profile, envelope, status, revision, available_at, \
             attempt_count, lease_owner, lease_expires_at, attempts, dedupe_key, failure_code, \
             completed_at FROM minco_jobs WHERE job_id = $1",
        )
        .bind(job_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| infrastructure(&error))?;
        transaction
            .commit()
            .await
            .map_err(|error| infrastructure(&error))?;
        decode_job_row(&row).map(Some)
    }

    async fn complete(
        &self,
        job_id: Uuid,
        worker_id: &str,
        now: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let attempt = self.current_attempt(job_id).await?;
        let entry = attempt_entry(
            attempt,
            worker_id,
            now,
            minco_plugin_jobs::JobAttemptOutcome::Succeeded,
        )?;
        let result = sqlx::query(
            "UPDATE minco_jobs SET status = 'succeeded', lease_owner = NULL, lease_expires_at = \
             NULL, failure_code = NULL, completed_at = $3, revision = revision + 1, \
             attempts = json_insert(CASE WHEN json_array_length(attempts) >= 25 \
             THEN json_remove(attempts, '$[0]') ELSE attempts END, '$[#]', json($4)) \
             WHERE job_id = $1 AND status = 'running' AND lease_owner = $2",
        )
        .bind(job_id.to_string())
        .bind(worker_id)
        .bind(now.to_rfc3339())
        .bind(entry)
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        verify_job_update(&self.pool, job_id, worker_id, &result).await
    }

    async fn schedule_retry(
        &self,
        job_id: Uuid,
        worker_id: &str,
        failure_code: &str,
        next_available_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let attempt = self.current_attempt(job_id).await?;
        let entry = attempt_entry(
            attempt,
            worker_id,
            now,
            minco_plugin_jobs::JobAttemptOutcome::Retried {
                code: failure_code.to_owned(),
            },
        )?;
        let result = sqlx::query(
            "UPDATE minco_jobs SET status = 'pending', lease_owner = NULL, lease_expires_at = \
             NULL, available_at = $4, failure_code = $3, revision = revision + 1, \
             attempts = json_insert(CASE WHEN json_array_length(attempts) >= 25 \
             THEN json_remove(attempts, '$[0]') ELSE attempts END, '$[#]', json($5)) \
             WHERE job_id = $1 AND status = 'running' AND lease_owner = $2",
        )
        .bind(job_id.to_string())
        .bind(worker_id)
        .bind(failure_code)
        .bind(next_available_at.to_rfc3339())
        .bind(entry)
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        verify_job_update(&self.pool, job_id, worker_id, &result).await
    }

    async fn fail_permanently(
        &self,
        job_id: Uuid,
        worker_id: &str,
        failure_code: &str,
        now: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let attempt = self.current_attempt(job_id).await?;
        let entry = attempt_entry(
            attempt,
            worker_id,
            now,
            minco_plugin_jobs::JobAttemptOutcome::FailedPermanently {
                code: failure_code.to_owned(),
            },
        )?;
        let result = sqlx::query(
            "UPDATE minco_jobs SET status = 'failed_permanently', lease_owner = NULL, \
             lease_expires_at = NULL, failure_code = $3, completed_at = $4, revision = revision + 1, \
             attempts = json_insert(CASE WHEN json_array_length(attempts) >= 25 \
             THEN json_remove(attempts, '$[0]') ELSE attempts END, '$[#]', json($5)) \
             WHERE job_id = $1 AND status = 'running' AND lease_owner = $2",
        )
        .bind(job_id.to_string())
        .bind(worker_id)
        .bind(failure_code)
        .bind(now.to_rfc3339())
        .bind(entry)
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        verify_job_update(&self.pool, job_id, worker_id, &result).await
    }

    async fn cancel(
        &self,
        job_id: Uuid,
        expected_revision: u64,
        now: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let result = sqlx::query(
            "UPDATE minco_jobs SET status = 'cancelled', completed_at = $3, revision = revision + 1 \
             WHERE job_id = $1 AND revision = $2 AND status = 'pending'",
        )
        .bind(job_id.to_string())
        .bind(i64::try_from(expected_revision).unwrap_or(-1))
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        verify_revision_guard(&self.pool, job_id, expected_revision, &result).await
    }

    async fn retry_failed(
        &self,
        job_id: Uuid,
        expected_revision: u64,
        now: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, JobError> {
        let result = sqlx::query(
            "UPDATE minco_jobs SET status = 'pending', failure_code = NULL, completed_at = NULL, \
             available_at = $3, attempt_count = 0, lease_owner = NULL, lease_expires_at = NULL, \
             revision = revision + 1 WHERE job_id = $1 AND revision = $2 AND status = \
             'failed_permanently'",
        )
        .bind(job_id.to_string())
        .bind(i64::try_from(expected_revision).unwrap_or(-1))
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        verify_revision_guard(&self.pool, job_id, expected_revision, &result).await?;
        Ok(now)
    }

    async fn recover_expired_leases(&self, now: DateTime<Utc>) -> Result<usize, JobError> {
        let result = sqlx::query(
            "UPDATE minco_jobs SET status = 'pending', lease_owner = NULL, lease_expires_at = \
             NULL, available_at = $1, revision = revision + 1 \
             WHERE status = 'running' AND lease_expires_at IS NOT NULL AND lease_expires_at <= $1",
        )
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        Ok(usize::try_from(result.rows_affected()).unwrap_or(usize::MAX))
    }

    async fn get(&self, job_id: Uuid) -> Result<Option<JobRecord>, JobError> {
        let row = sqlx::query(
            "SELECT job_id, worker_profile, envelope, status, revision, available_at, \
             attempt_count, lease_owner, lease_expires_at, attempts, dedupe_key, failure_code, \
             completed_at FROM minco_jobs WHERE job_id = $1",
        )
        .bind(job_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        row.map(|row| decode_job_row(&row)).transpose()
    }

    async fn list_failed(&self, limit: usize) -> Result<Vec<JobRecord>, JobError> {
        let rows = sqlx::query(
            "SELECT job_id, worker_profile, envelope, status, revision, available_at, \
             attempt_count, lease_owner, lease_expires_at, attempts, dedupe_key, failure_code, \
             completed_at FROM minco_jobs WHERE status = 'failed_permanently' \
             ORDER BY completed_at, job_id LIMIT $1",
        )
        .bind(i64::try_from(limit.min(100)).unwrap_or(100))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        rows.iter().map(decode_job_row).collect()
    }
}

impl SqliteJobStore {
    async fn current_attempt(&self, job_id: Uuid) -> Result<u32, JobError> {
        let attempt: Option<i64> =
            sqlx::query_scalar("SELECT attempt_count FROM minco_jobs WHERE job_id = $1")
                .bind(job_id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| infrastructure(&error))?;
        Ok(attempt.map_or(1, |value| u32::try_from(value).unwrap_or(1).max(1)))
    }
}

#[async_trait::async_trait]
impl minco_plugin_jobs::JobPublicationStore for SqliteJobStore {
    async fn enqueue_intent(
        &self,
        job_id: Uuid,
        worker_profile: &str,
        available_at: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let result = sqlx::query(
            "INSERT INTO minco_job_publications (job_id, worker_profile, status, attempt_count, \
             available_at, claimed_by, claim_expires_at, last_error) \
             VALUES ($1, $2, 'pending', 0, $3, NULL, NULL, NULL) ON CONFLICT (job_id) DO NOTHING",
        )
        .bind(job_id.to_string())
        .bind(worker_profile)
        .bind(available_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        if result.rows_affected() == 0 {
            return Err(JobError::InvalidTransition { job_id });
        }
        Ok(())
    }

    async fn claim_due(
        &self,
        worker_id: &str,
        limit: usize,
        claim_expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Vec<JobPublication>, JobError> {
        validate_worker_claim(worker_id, limit, claim_expires_at, now)?;
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| infrastructure(&error))?;
        let ids: Vec<(String,)> = sqlx::query_as(
            "SELECT job_id FROM minco_job_publications \
             WHERE (status IN ('pending', 'failed') AND available_at <= $1) \
             OR (status = 'claimed' AND claim_expires_at IS NOT NULL AND claim_expires_at <= $1) \
             ORDER BY available_at, job_id LIMIT $2",
        )
        .bind(now.to_rfc3339())
        .bind(i64::try_from(limit.min(100)).unwrap_or(100))
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| infrastructure(&error))?;
        let claimed: Vec<JobPublication> = if ids.is_empty() {
            Vec::new()
        } else {
            let mut builder = sqlx::QueryBuilder::new(
                "UPDATE minco_job_publications SET status = 'claimed', claimed_by = ",
            );
            builder.push_bind(worker_id.to_owned());
            builder.push(", claim_expires_at = ");
            builder.push_bind(claim_expires_at.to_rfc3339());
            builder.push(", attempt_count = attempt_count + 1 WHERE job_id IN (");
            let mut separated = builder.separated(", ");
            for (job_id,) in &ids {
                separated.push_bind(job_id.clone());
            }
            builder.push(
                ") RETURNING job_id, worker_profile, status, attempt_count, \
                 available_at, claimed_by, claim_expires_at, last_error",
            );
            let rows = builder
                .build()
                .fetch_all(&mut *transaction)
                .await
                .map_err(|error| infrastructure(&error))?;
            rows.iter()
                .map(decode_publication_row)
                .collect::<Result<Vec<_>, _>>()?
        };
        transaction
            .commit()
            .await
            .map_err(|error| infrastructure(&error))?;
        Ok(claimed)
    }

    async fn mark_published(&self, job_id: Uuid, worker_id: &str) -> Result<(), JobError> {
        let result = sqlx::query(
            "UPDATE minco_job_publications SET status = 'published', claimed_by = NULL, \
             claim_expires_at = NULL, last_error = NULL \
             WHERE job_id = $1 AND status = 'claimed' AND claimed_by = $2",
        )
        .bind(job_id.to_string())
        .bind(worker_id)
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        verify_publication_update(&self.pool, job_id, worker_id, &result).await
    }

    async fn mark_failed(
        &self,
        job_id: Uuid,
        worker_id: &str,
        error: &str,
        retry_at: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let result = sqlx::query(
            "UPDATE minco_job_publications SET status = 'failed', claimed_by = NULL, \
             claim_expires_at = NULL, available_at = $3, last_error = $4 \
             WHERE job_id = $1 AND status = 'claimed' AND claimed_by = $2",
        )
        .bind(job_id.to_string())
        .bind(worker_id)
        .bind(retry_at.to_rfc3339())
        .bind(error)
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        verify_publication_update(&self.pool, job_id, worker_id, &result).await
    }

    async fn republish(
        &self,
        job_id: Uuid,
        worker_id: &str,
        available_at: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let result = sqlx::query(
            "UPDATE minco_job_publications SET status = 'pending', claimed_by = NULL, \
             claim_expires_at = NULL, available_at = $3 \
             WHERE job_id = $1 AND (status IN ('published', 'failed', 'pending') \
             OR (status = 'claimed' AND claimed_by = $2))",
        )
        .bind(job_id.to_string())
        .bind(worker_id)
        .bind(available_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        verify_publication_update(&self.pool, job_id, worker_id, &result).await
    }

    async fn recover_expired_claims(&self, now: DateTime<Utc>) -> Result<usize, JobError> {
        let result = sqlx::query(
            "UPDATE minco_job_publications SET status = 'pending', claimed_by = NULL, \
             claim_expires_at = NULL \
             WHERE status = 'claimed' AND claim_expires_at IS NOT NULL AND claim_expires_at <= $1",
        )
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        Ok(usize::try_from(result.rows_affected()).unwrap_or(usize::MAX))
    }
}

#[async_trait::async_trait]
impl minco_plugin_jobs::OverlapLockStore for SqliteJobStore {
    async fn acquire(
        &self,
        overlap_key: &str,
        owner: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool, JobError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| infrastructure(&error))?;
        let expired: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM minco_job_locks WHERE overlap_key = $1 AND expires_at <= $2)",
        )
        .bind(overlap_key)
        .bind(now.to_rfc3339())
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| infrastructure(&error))?;
        let acquired = if expired {
            sqlx::query(
                "UPDATE minco_job_locks SET owner = $2, expires_at = $3 \
                 WHERE overlap_key = $1 AND expires_at <= $4",
            )
            .bind(overlap_key)
            .bind(owner)
            .bind(expires_at.to_rfc3339())
            .bind(now.to_rfc3339())
            .execute(&mut *transaction)
            .await
            .map_err(|error| infrastructure(&error))?
            .rows_affected()
                == 1
        } else {
            sqlx::query("INSERT OR IGNORE INTO minco_job_locks (overlap_key, owner, expires_at) VALUES ($1, $2, $3)")
                .bind(overlap_key)
                .bind(owner)
                .bind(expires_at.to_rfc3339())
                .execute(&mut *transaction)
                .await
                .map_err(|error| infrastructure(&error))?
                .rows_affected()
                == 1
        };
        transaction
            .commit()
            .await
            .map_err(|error| infrastructure(&error))?;
        Ok(acquired)
    }

    async fn refresh(
        &self,
        overlap_key: &str,
        owner: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool, JobError> {
        let result = sqlx::query(
            "UPDATE minco_job_locks SET expires_at = $3 \
             WHERE overlap_key = $1 AND owner = $2 AND expires_at > $4",
        )
        .bind(overlap_key)
        .bind(owner)
        .bind(expires_at.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        Ok(result.rows_affected() == 1)
    }

    async fn release(&self, overlap_key: &str, owner: &str) -> Result<(), JobError> {
        sqlx::query("DELETE FROM minco_job_locks WHERE overlap_key = $1 AND owner = $2")
            .bind(overlap_key)
            .bind(owner)
            .execute(&self.pool)
            .await
            .map_err(|error| infrastructure(&error))?;
        Ok(())
    }

    async fn recover_expired(&self, now: DateTime<Utc>) -> Result<usize, JobError> {
        let result = sqlx::query("DELETE FROM minco_job_locks WHERE expires_at <= $1")
            .bind(now.to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(|error| infrastructure(&error))?;
        Ok(usize::try_from(result.rows_affected()).unwrap_or(usize::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_plugin_jobs::{
        JobEnvelope, JobOptions, JobPublicationStore as _, JobStore as _, RetryPolicy,
    };
    use std::sync::Arc;

    async fn pool() -> SqlitePool {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("jobs.sqlite");
        std::mem::forget(directory);
        let url = format!("sqlite://{}?mode=rwc", path.display());
        crate::plugin_adapters::migrate_plugin_storage(
            &SqlitePool::connect(&url).await.expect("sqlite pool"),
        )
        .await
        .expect("migrations");
        SqlitePool::connect(&format!("sqlite://{}", path.display()))
            .await
            .expect("reconnect")
    }

    fn record(dedupe_key: Option<&str>) -> JobRecord {
        let mut options = JobOptions::default().with_retry(RetryPolicy::fixed(5, 1));
        if let Some(key) = dedupe_key {
            options = options.with_dedupe_key(key);
        }
        let envelope = JobEnvelope::for_parts(
            "orders.send-confirmation",
            1,
            serde_json::json!({ "order_id": "o-1" }),
            "orders-notifications",
            Uuid::now_v7(),
        )
        .expect("valid envelope")
        .with(options);
        minco_plugin_jobs::pending_record(envelope)
    }

    #[tokio::test]
    async fn enqueue_in_rolls_back_with_the_callers_transaction() {
        let pool = pool().await;
        let store = SqliteJobStore::new(pool.clone());
        let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await.expect("begin");
        let record = record(None);
        store
            .enqueue_in(&mut transaction, record.clone())
            .await
            .expect("enqueue in tx");
        transaction.rollback().await.expect("rollback");
        let job = store.get(record.envelope.job_id).await.expect("get");
        assert!(job.is_none(), "rollback leaves no durable job");
        let publications = store
            .claim_due(
                "probe",
                10,
                Utc::now() + chrono::TimeDelta::minutes(1),
                Utc::now(),
            )
            .await
            .expect("probe");
        assert!(publications.is_empty(), "rollback leaves no intent");
    }

    #[tokio::test]
    async fn enqueue_in_commits_exactly_one_recoverable_intent() {
        let pool = pool().await;
        let store = SqliteJobStore::new(pool.clone());
        let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await.expect("begin");
        let record = record(None);
        match store
            .enqueue_in(&mut transaction, record.clone())
            .await
            .expect("enqueue")
        {
            EnqueueOutcome::Inserted(job_id) => assert_eq!(job_id, record.envelope.job_id),
            EnqueueOutcome::Duplicate(existing) => {
                panic!("expected insertion, got duplicate {existing}")
            }
        }
        transaction.commit().await.expect("commit");
        let publications = store
            .claim_due(
                "probe",
                10,
                Utc::now() + chrono::TimeDelta::minutes(1),
                Utc::now(),
            )
            .await
            .expect("claim");
        assert_eq!(publications.len(), 1);
        assert_eq!(publications[0].job_id, record.envelope.job_id);
    }

    #[tokio::test]
    async fn execution_claims_admit_exactly_one_owner() {
        let pool = pool().await;
        let store = Arc::new(SqliteJobStore::new(pool));
        let record = record(None);
        store
            .enqueue_with_intent(record.clone())
            .await
            .expect("enqueue");
        let now = Utc::now();
        let first = store
            .claim_execution(
                record.envelope.job_id,
                "worker-a",
                now + chrono::TimeDelta::minutes(10),
                now,
            )
            .await
            .expect("first claim");
        assert!(first.is_some());
        let second = store
            .claim_execution(
                record.envelope.job_id,
                "worker-b",
                now + chrono::TimeDelta::minutes(10),
                now,
            )
            .await
            .expect("second claim");
        assert!(second.is_none(), "a live lease blocks other owners");
        let error = store
            .complete(record.envelope.job_id, "worker-b", now)
            .await
            .expect_err("wrong owner");
        assert!(matches!(error, JobError::LeaseOwnership { .. }));
        store
            .complete(record.envelope.job_id, "worker-a", now)
            .await
            .expect("owner completes");
    }

    #[tokio::test]
    async fn expired_leases_recover_and_stale_owners_lose() {
        let pool = pool().await;
        let store = SqliteJobStore::new(pool);
        let record = record(None);
        store
            .enqueue_with_intent(record.clone())
            .await
            .expect("enqueue");
        let now = Utc::now();
        store
            .claim_execution(
                record.envelope.job_id,
                "ghost",
                now + chrono::TimeDelta::minutes(1),
                now,
            )
            .await
            .expect("claim")
            .expect("claimed");
        let recovered = store
            .recover_expired_leases(now + chrono::TimeDelta::minutes(2))
            .await
            .expect("recover");
        assert_eq!(recovered, 1);
        let reclaimed = store
            .claim_execution(
                record.envelope.job_id,
                "worker-b",
                now + chrono::TimeDelta::minutes(30),
                now + chrono::TimeDelta::minutes(2),
            )
            .await
            .expect("reclaim")
            .expect("reclaimed");
        assert_eq!(reclaimed.attempt_count, 2);
        let error = store
            .complete(record.envelope.job_id, "ghost", now)
            .await
            .expect_err("stale owner");
        assert!(matches!(error, JobError::LeaseOwnership { .. }));
    }

    #[tokio::test]
    async fn duplicate_dedupe_submissions_are_deterministic() {
        let pool = pool().await;
        let store = SqliteJobStore::new(pool);
        store
            .enqueue_with_intent(record(Some("orders.confirm:o-1")))
            .await
            .expect("first");
        match store
            .enqueue_with_intent(record(Some("orders.confirm:o-1")))
            .await
            .expect("second")
        {
            EnqueueOutcome::Duplicate(_) => {}
            EnqueueOutcome::Inserted(inserted) => {
                panic!("identical resubmission must be idempotent, got {inserted}")
            }
        }
        let conflicting = minco_plugin_jobs::pending_record(
            JobEnvelope::for_parts(
                "orders.send-confirmation",
                1,
                serde_json::json!({ "order_id": "o-2" }),
                "orders-notifications",
                Uuid::now_v7(),
            )
            .unwrap()
            .with(JobOptions::default().with_dedupe_key("orders.confirm:o-1")),
        );
        let error = store
            .enqueue_with_intent(conflicting)
            .await
            .expect_err("conflict");
        assert!(matches!(
            error,
            JobError::DuplicateSubmissionConflict { .. }
        ));
    }

    #[tokio::test]
    async fn retry_state_round_trips_through_publication_republish() {
        let pool = pool().await;
        let store = SqliteJobStore::new(pool);
        let record = record(None);
        store
            .enqueue_with_intent(record.clone())
            .await
            .expect("enqueue");
        let now = Utc::now();
        store
            .claim_execution(
                record.envelope.job_id,
                "w",
                now + chrono::TimeDelta::minutes(5),
                now,
            )
            .await
            .expect("claim")
            .expect("claimed");
        let retry_at = now + chrono::TimeDelta::seconds(60);
        store
            .schedule_retry(
                record.envelope.job_id,
                "w",
                "notification-unavailable",
                retry_at,
                now,
            )
            .await
            .expect("retry");
        store
            .republish(record.envelope.job_id, "w", retry_at)
            .await
            .expect("republish");
        let early = store
            .claim_due("d", 10, now + chrono::TimeDelta::minutes(1), now)
            .await
            .expect("early");
        assert!(early.is_empty());
        let due = store
            .claim_due("d", 10, retry_at + chrono::TimeDelta::minutes(1), retry_at)
            .await
            .expect("due");
        assert_eq!(due.len(), 1);
    }

    #[tokio::test]
    async fn attempt_history_stays_bounded() {
        let pool = pool().await;
        let store = SqliteJobStore::new(pool);
        let record = record(None);
        store
            .enqueue_with_intent(record.clone())
            .await
            .expect("enqueue");
        let mut now = Utc::now();
        for _ in 0..30 {
            store
                .claim_execution(
                    record.envelope.job_id,
                    "w",
                    now + chrono::TimeDelta::minutes(5),
                    now,
                )
                .await
                .expect("claim")
                .expect("claimed");
            store
                .schedule_retry(
                    record.envelope.job_id,
                    "w",
                    "always-busy",
                    now + chrono::TimeDelta::seconds(1),
                    now,
                )
                .await
                .expect("retry");
            now += chrono::TimeDelta::seconds(2);
        }
        let final_record = store
            .get(record.envelope.job_id)
            .await
            .expect("get")
            .expect("present");
        assert_eq!(final_record.attempts.len(), 25, "history is bounded");
        assert_eq!(final_record.attempt_count, 30);
        assert!(
            final_record.attempts.first().expect("oldest").attempt
                < final_record.attempts.last().expect("newest").attempt
        );
    }

    #[tokio::test]
    async fn operator_transitions_are_revision_guarded() {
        let pool = pool().await;
        let store = SqliteJobStore::new(pool);
        let record = record(None);
        store
            .enqueue_with_intent(record.clone())
            .await
            .expect("enqueue");
        let job_id = record.envelope.job_id;
        let current = store.get(job_id).await.unwrap().unwrap();
        let error = store
            .cancel(job_id, current.revision + 1, Utc::now())
            .await
            .unwrap_err();
        assert!(matches!(error, JobError::RevisionConflict { .. }));
        store
            .cancel(job_id, current.revision, Utc::now())
            .await
            .unwrap();
        let failed = store.list_failed(10).await.unwrap();
        assert!(failed.is_empty(), "cancelled jobs are not failed jobs");
    }
}
