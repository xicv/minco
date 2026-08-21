//! Durable `PostgreSQL` job storage: job rows, publication intent, execution
//! leases and overlap locks.
//!
//! The job row owns execution state and the publication row owns pending
//! transport delivery; the queue message is never authoritative. The
//! execution claim is a single compare-and-set `UPDATE`, publications are
//! claimed with `FOR UPDATE SKIP LOCKED`, and every transition verifies
//! affected rows before reporting success.

use chrono::{DateTime, Utc};
use minco_plugin_jobs::{
    JobAttempt, JobError, JobPublication, JobRecord, JobStatus, PublicationStatus,
    validate_worker_claim,
};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;

/// PostgreSQL-backed job store implementing every job port.
#[derive(Debug, Clone)]
pub struct PostgresJobStore {
    pool: PgPool,
}

impl PostgresJobStore {
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert the job and its publication intent inside the caller's
    /// transaction so the business mutation and the durable dispatch commit
    /// atomically. Rolling the caller's transaction back leaves neither row.
    pub async fn enqueue_in(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        record: JobRecord,
    ) -> Result<minco_plugin_jobs::EnqueueOutcome, JobError> {
        if record.status != JobStatus::Pending {
            return Err(JobError::InvalidJob("jobs must be enqueued pending".into()));
        }
        record.envelope.validate()?;
        let envelope_json = serde_json::to_value(&record.envelope).map_err(|error| {
            JobError::Infrastructure(format!("job envelope encode failed: {error}"))
        })?;
        let attempts = serde_json::to_value(&record.attempts)
            .map_err(|error| JobError::Infrastructure(format!("attempt encode failed: {error}")))?;
        // The insert runs inside a savepoint so a dedupe-key conflict can be
        // recovered without aborting the caller's transaction.
        sqlx::query("SAVEPOINT minco_job_enqueue")
            .execute(&mut **transaction)
            .await
            .map_err(|_| JobError::Infrastructure("job savepoint failed".into()))?;
        let result = sqlx::query(
            "INSERT INTO minco_jobs (job_id, worker_profile, envelope, status, revision, \
             available_at, attempt_count, lease_owner, lease_expires_at, attempts, dedupe_key, \
             failure_code, completed_at) \
             VALUES ($1, $2, $3, 'pending', $4, $5, $6, NULL, NULL, $7, $8, NULL, NULL)",
        )
        .bind(record.envelope.job_id)
        .bind(&record.envelope.worker_profile)
        .bind(&envelope_json)
        .bind(i64::try_from(record.revision).unwrap_or(1))
        .bind(record.envelope.available_at)
        .bind(i32::try_from(record.attempt_count).unwrap_or(0))
        .bind(&attempts)
        .bind(record.envelope.dedupe_key.as_deref())
        .execute(&mut **transaction)
        .await;
        match result {
            Ok(_) => {
                sqlx::query("RELEASE SAVEPOINT minco_job_enqueue")
                    .execute(&mut **transaction)
                    .await
                    .map_err(|_| JobError::Infrastructure("job savepoint release failed".into()))?;
            }
            Err(error) => {
                let constraint = error
                    .as_database_error()
                    .and_then(|database| database.constraint());
                sqlx::query("ROLLBACK TO SAVEPOINT minco_job_enqueue")
                    .execute(&mut **transaction)
                    .await
                    .map_err(|_| {
                        JobError::Infrastructure("job savepoint rollback failed".into())
                    })?;
                if constraint == Some("minco_jobs_pkey") {
                    return Err(JobError::MissingJob(record.envelope.job_id));
                }
                if constraint == Some("minco_jobs_dedupe_key") {
                    return dedupe_outcome(&record, transaction).await;
                }
                return Err(infrastructure(&error));
            }
        }
        sqlx::query(
            "INSERT INTO minco_job_publications (job_id, worker_profile, status, attempt_count, \
             available_at, claimed_by, claim_expires_at, last_error) \
             VALUES ($1, $2, 'pending', 0, $3, NULL, NULL, NULL)",
        )
        .bind(record.envelope.job_id)
        .bind(&record.envelope.worker_profile)
        .bind(record.envelope.available_at)
        .execute(&mut **transaction)
        .await
        .map_err(|_| JobError::Infrastructure("job publication insert failed".into()))?;
        Ok(minco_plugin_jobs::EnqueueOutcome::Inserted(
            record.envelope.job_id,
        ))
    }
}

async fn dedupe_outcome(
    record: &JobRecord,
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<minco_plugin_jobs::EnqueueOutcome, JobError> {
    let existing: Option<(serde_json::Value,)> =
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
    let identical = existing_envelope.get("job_name")
        == Some(&serde_json::json!(record.envelope.job_name))
        && existing_envelope.get("job_version")
            == Some(&serde_json::json!(record.envelope.job_version))
        && existing_envelope.get("payload") == Some(&record.envelope.payload);
    let existing_id = existing_envelope
        .get("job_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse::<Uuid>().ok());
    match (identical, existing_id) {
        (true, Some(existing_job_id)) => Ok(minco_plugin_jobs::EnqueueOutcome::Duplicate(
            existing_job_id,
        )),
        (false, Some(existing_job_id)) => {
            Err(JobError::DuplicateSubmissionConflict { existing_job_id })
        }
        _ => Err(JobError::Infrastructure(
            "job dedupe conflict could not be resolved".into(),
        )),
    }
}

fn infrastructure(error: &sqlx::Error) -> JobError {
    JobError::Infrastructure(format!("postgres job store failed: {error}"))
}

fn decode_job_row(row: &PgRow) -> Result<JobRecord, JobError> {
    let envelope_json: serde_json::Value = row
        .try_get("envelope")
        .map_err(|_| JobError::Infrastructure("job envelope column was not valid JSON".into()))?;
    let mut envelope: minco_plugin_jobs::JobEnvelope = serde_json::from_value(envelope_json)
        .map_err(|error| {
            JobError::Infrastructure(format!("job envelope decode failed: {error}"))
        })?;
    let status: String = row
        .try_get("status")
        .map_err(|_| JobError::Infrastructure("job status column unreadable".into()))?;
    let status = parse_status(&status)?;
    let attempt_count: i32 = row
        .try_get("attempt_count")
        .map_err(|_| JobError::Infrastructure("job attempt column unreadable".into()))?;
    envelope.available_at = row
        .try_get("available_at")
        .map_err(|_| JobError::Infrastructure("job availability column unreadable".into()))?;
    envelope.attempt = u32::try_from(attempt_count)
        .unwrap_or(envelope.attempt)
        .max(1);
    let attempts_json: serde_json::Value = row
        .try_get("attempts")
        .map_err(|_| JobError::Infrastructure("job attempts column unreadable".into()))?;
    let attempts: Vec<JobAttempt> = serde_json::from_value(attempts_json)
        .map_err(|error| JobError::Infrastructure(format!("attempt decode failed: {error}")))?;
    Ok(JobRecord {
        envelope,
        status,
        revision: u64::try_from(
            row.try_get::<i64, _>("revision")
                .map_err(|_| JobError::Infrastructure("job revision unreadable".into()))?,
        )
        .unwrap_or(1),
        lease_owner: row
            .try_get("lease_owner")
            .map_err(|_| JobError::Infrastructure("job lease owner unreadable".into()))?,
        lease_expires_at: row
            .try_get("lease_expires_at")
            .map_err(|_| JobError::Infrastructure("job lease expiry unreadable".into()))?,
        attempt_count: u32::try_from(attempt_count).unwrap_or(0),
        attempts,
        failure_code: row
            .try_get("failure_code")
            .map_err(|_| JobError::Infrastructure("job failure code unreadable".into()))?,
        completed_at: row
            .try_get("completed_at")
            .map_err(|_| JobError::Infrastructure("job completion unreadable".into()))?,
    })
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

fn attempt_entry(
    attempt: u32,
    worker_id: &str,
    now: DateTime<Utc>,
    outcome: minco_plugin_jobs::JobAttemptOutcome,
) -> Result<serde_json::Value, JobError> {
    serde_json::to_value(JobAttempt {
        attempt,
        at: now,
        worker_id: worker_id.to_owned(),
        outcome,
    })
    .map_err(|error| JobError::Infrastructure(format!("attempt encode failed: {error}")))
}

/// Verify a conditional transition affected exactly one row, distinguishing
/// ownership loss from a missing job.
async fn require_job_update(
    pool: &PgPool,
    job_id: Uuid,
    worker_id: &str,
    result: &sqlx::postgres::PgQueryResult,
) -> Result<(), JobError> {
    if result.rows_affected() == 1 {
        return Ok(());
    }
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM minco_jobs WHERE job_id = $1)")
            .bind(job_id)
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

#[async_trait::async_trait]
impl minco_plugin_jobs::JobStore for PostgresJobStore {
    async fn enqueue_with_intent(
        &self,
        record: JobRecord,
    ) -> Result<minco_plugin_jobs::EnqueueOutcome, JobError> {
        let mut transaction = self
            .pool
            .begin()
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
        let row = sqlx::query(
            "UPDATE minco_jobs SET status = 'running', lease_owner = $2, lease_expires_at = $3, \
             attempt_count = attempt_count + 1, revision = revision + 1 \
             WHERE job_id = $1 AND ((status = 'pending' AND available_at <= $4) \
             OR (status = 'running' AND lease_expires_at IS NOT NULL AND lease_expires_at <= $4)) \
             RETURNING job_id, worker_profile, envelope, status, revision, available_at, \
             attempt_count, lease_owner, lease_expires_at, attempts, dedupe_key, failure_code, \
             completed_at",
        )
        .bind(job_id)
        .bind(worker_id)
        .bind(lease_expires_at)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        row.map(|row| decode_job_row(&row)).transpose()
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
             attempts = (CASE WHEN jsonb_array_length(attempts) >= 25 THEN attempts - 0 ELSE attempts END) || $4::jsonb \
             WHERE job_id = $1 AND status = 'running' AND lease_owner = $2",
        )
        .bind(job_id)
        .bind(worker_id)
        .bind(now)
        .bind(&entry)
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        require_job_update(&self.pool, job_id, worker_id, &result).await
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
             attempts = (CASE WHEN jsonb_array_length(attempts) >= 25 THEN attempts - 0 ELSE attempts END) || $5::jsonb \
             WHERE job_id = $1 AND status = 'running' AND lease_owner = $2",
        )
        .bind(job_id)
        .bind(worker_id)
        .bind(failure_code)
        .bind(next_available_at)
        .bind(&entry)
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        require_job_update(&self.pool, job_id, worker_id, &result).await
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
             attempts = (CASE WHEN jsonb_array_length(attempts) >= 25 THEN attempts - 0 ELSE attempts END) || $5::jsonb \
             WHERE job_id = $1 AND status = 'running' AND lease_owner = $2",
        )
        .bind(job_id)
        .bind(worker_id)
        .bind(failure_code)
        .bind(now)
        .bind(&entry)
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        require_job_update(&self.pool, job_id, worker_id, &result).await
    }

    async fn cancel(
        &self,
        job_id: Uuid,
        expected_revision: u64,
        now: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let expected = i64::try_from(expected_revision).unwrap_or(-1);
        let result = sqlx::query(
            "UPDATE minco_jobs SET status = 'cancelled', completed_at = $3, revision = revision + 1 \
             WHERE job_id = $1 AND revision = $2 AND status = 'pending'",
        )
        .bind(job_id)
        .bind(expected)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        verify_revision_guard(self, job_id, expected_revision, &result).await
    }

    async fn retry_failed(
        &self,
        job_id: Uuid,
        expected_revision: u64,
        now: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, JobError> {
        let expected = i64::try_from(expected_revision).unwrap_or(-1);
        let result = sqlx::query(
            "UPDATE minco_jobs SET status = 'pending', failure_code = NULL, completed_at = NULL, \
             available_at = $3, attempt_count = 0, lease_owner = NULL, lease_expires_at = NULL, \
             revision = revision + 1 WHERE job_id = $1 AND revision = $2 AND status = \
             'failed_permanently'",
        )
        .bind(job_id)
        .bind(expected)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        verify_revision_guard(self, job_id, expected_revision, &result).await?;
        Ok(now)
    }

    async fn recover_expired_leases(&self, now: DateTime<Utc>) -> Result<usize, JobError> {
        let result = sqlx::query(
            "UPDATE minco_jobs SET status = 'pending', lease_owner = NULL, lease_expires_at = \
             NULL, available_at = $1, revision = revision + 1 \
             WHERE status = 'running' AND lease_expires_at IS NOT NULL AND lease_expires_at <= $1",
        )
        .bind(now)
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
        .bind(job_id)
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
             ORDER BY completed_at NULLS LAST, job_id LIMIT $1",
        )
        .bind(i64::try_from(limit.min(100)).unwrap_or(100))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        rows.iter().map(decode_job_row).collect()
    }
}

impl PostgresJobStore {
    async fn current_attempt(&self, job_id: Uuid) -> Result<u32, JobError> {
        let attempt: Option<i32> =
            sqlx::query_scalar("SELECT attempt_count FROM minco_jobs WHERE job_id = $1")
                .bind(job_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| infrastructure(&error))?;
        Ok(attempt.map_or(1, |value| u32::try_from(value).unwrap_or(1).max(1)))
    }
}

async fn verify_revision_guard(
    store: &PostgresJobStore,
    job_id: Uuid,
    expected_revision: u64,
    result: &sqlx::postgres::PgQueryResult,
) -> Result<(), JobError> {
    if result.rows_affected() == 1 {
        return Ok(());
    }
    let current: Option<i64> =
        sqlx::query_scalar("SELECT revision FROM minco_jobs WHERE job_id = $1")
            .bind(job_id)
            .fetch_optional(&store.pool)
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

fn decode_publication_row(row: &PgRow) -> Result<JobPublication, JobError> {
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
    Ok(JobPublication {
        job_id: row
            .try_get("job_id")
            .map_err(|_| JobError::Infrastructure("publication id unreadable".into()))?,
        worker_profile: row
            .try_get("worker_profile")
            .map_err(|_| JobError::Infrastructure("publication profile unreadable".into()))?,
        status,
        attempt_count: u32::try_from(
            row.try_get::<i32, _>("attempt_count")
                .map_err(|_| JobError::Infrastructure("publication attempts unreadable".into()))?,
        )
        .unwrap_or(0),
        available_at: row
            .try_get("available_at")
            .map_err(|_| JobError::Infrastructure("publication availability unreadable".into()))?,
        claimed_by: row
            .try_get("claimed_by")
            .map_err(|_| JobError::Infrastructure("publication claimant unreadable".into()))?,
        claim_expires_at: row
            .try_get("claim_expires_at")
            .map_err(|_| JobError::Infrastructure("publication claim expiry unreadable".into()))?,
        last_error: row
            .try_get("last_error")
            .map_err(|_| JobError::Infrastructure("publication error unreadable".into()))?,
    })
}

#[async_trait::async_trait]
impl minco_plugin_jobs::JobPublicationStore for PostgresJobStore {
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
        .bind(job_id)
        .bind(worker_profile)
        .bind(available_at)
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
        let rows = sqlx::query(
            "WITH candidates AS (SELECT job_id FROM minco_job_publications \
             WHERE (status IN ('pending', 'failed') AND available_at <= $1) \
             OR (status = 'claimed' AND claim_expires_at IS NOT NULL AND claim_expires_at <= $1) \
             ORDER BY available_at, job_id FOR UPDATE SKIP LOCKED LIMIT $2) \
             UPDATE minco_job_publications AS publication \
             SET status = 'claimed', claimed_by = $3, claim_expires_at = $4, \
             attempt_count = publication.attempt_count + 1 \
             FROM candidates WHERE publication.job_id = candidates.job_id \
             RETURNING *",
        )
        .bind(now)
        .bind(i64::try_from(limit.min(100)).unwrap_or(100))
        .bind(worker_id)
        .bind(claim_expires_at)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        rows.iter().map(decode_publication_row).collect()
    }

    async fn mark_published(&self, job_id: Uuid, worker_id: &str) -> Result<(), JobError> {
        let result = sqlx::query(
            "UPDATE minco_job_publications SET status = 'published', claimed_by = NULL, \
             claim_expires_at = NULL, last_error = NULL \
             WHERE job_id = $1 AND status = 'claimed' AND claimed_by = $2",
        )
        .bind(job_id)
        .bind(worker_id)
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        require_publication_update(job_id, worker_id, &result, &self.pool).await
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
        .bind(job_id)
        .bind(worker_id)
        .bind(retry_at)
        .bind(error)
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        require_publication_update(job_id, worker_id, &result, &self.pool).await
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
        .bind(job_id)
        .bind(worker_id)
        .bind(available_at)
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        if result.rows_affected() == 1 {
            return Ok(());
        }
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM minco_job_publications WHERE job_id = $1)",
        )
        .bind(job_id)
        .fetch_one(&self.pool)
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

    async fn recover_expired_claims(&self, now: DateTime<Utc>) -> Result<usize, JobError> {
        let result = sqlx::query(
            "UPDATE minco_job_publications SET status = 'pending', claimed_by = NULL, \
             claim_expires_at = NULL \
             WHERE status = 'claimed' AND claim_expires_at IS NOT NULL AND claim_expires_at <= $1",
        )
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        Ok(usize::try_from(result.rows_affected()).unwrap_or(usize::MAX))
    }
}

async fn require_publication_update(
    job_id: Uuid,
    worker_id: &str,
    result: &sqlx::postgres::PgQueryResult,
    pool: &PgPool,
) -> Result<(), JobError> {
    if result.rows_affected() == 1 {
        return Ok(());
    }
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM minco_job_publications WHERE job_id = $1)")
            .bind(job_id)
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
impl minco_plugin_jobs::OverlapLockStore for PostgresJobStore {
    async fn acquire(
        &self,
        overlap_key: &str,
        owner: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool, JobError> {
        let inserted = sqlx::query(
            "INSERT INTO minco_job_locks (overlap_key, owner, expires_at) VALUES ($1, $2, $3) \
             ON CONFLICT (overlap_key) DO UPDATE SET owner = $2, expires_at = $3 \
             WHERE minco_job_locks.expires_at <= $4",
        )
        .bind(overlap_key)
        .bind(owner)
        .bind(expires_at)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        Ok(inserted.rows_affected() == 1)
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
        .bind(expires_at)
        .bind(now)
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
            .bind(now)
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
        EnqueueOutcome, JobEnvelope, JobOptions, JobPublicationStore as _, JobStore as _,
        RetryPolicy,
    };
    use std::sync::Arc;

    fn test_lock() -> &'static tokio::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(tokio::sync::Mutex::default)
    }

    async fn pool() -> Option<PgPool> {
        let Ok(url) = std::env::var("MINCO_TEST_POSTGRES_URL") else {
            eprintln!("MINCO_TEST_POSTGRES_URL not set; PostgreSQL jobs proof skipped");
            return None;
        };
        let pool = PgPool::connect(&url).await.ok()?;
        crate::plugin_adapters::migrate_plugin_storage(&pool)
            .await
            .ok()?;
        sqlx::raw_sql("TRUNCATE minco_job_publications, minco_jobs, minco_job_locks")
            .execute(&pool)
            .await
            .ok()?;
        Some(pool)
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
        let Some(pool) = pool().await else { return };
        let _guard = test_lock().lock().await;
        let store = PostgresJobStore::new(pool.clone());
        sqlx::raw_sql(
            "CREATE TABLE IF NOT EXISTS minco_jobs_proof_orders (id INTEGER PRIMARY KEY)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut transaction = pool.begin().await.unwrap();
        sqlx::query("INSERT INTO minco_jobs_proof_orders (id) VALUES (1)")
            .execute(&mut *transaction)
            .await
            .unwrap();
        let record = record(None);
        store
            .enqueue_in(&mut transaction, record.clone())
            .await
            .unwrap();
        transaction.rollback().await.unwrap();
        let job = store.get(record.envelope.job_id).await.unwrap();
        assert!(job.is_none(), "rollback must leave no durable job");
        let publications = store
            .claim_due(
                "probe",
                10,
                Utc::now() + chrono::TimeDelta::minutes(1),
                Utc::now(),
            )
            .await
            .unwrap();
        assert!(
            publications.is_empty(),
            "rollback must leave no publication intent"
        );
        let orders: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM minco_jobs_proof_orders")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(orders, 0, "the business mutation rolled back too");
    }

    #[tokio::test]
    async fn enqueue_in_commits_exactly_one_recoverable_intent_with_the_mutation() {
        let Some(pool) = pool().await else { return };
        let _guard = test_lock().lock().await;
        let store = PostgresJobStore::new(pool.clone());
        let mut transaction = pool.begin().await.unwrap();
        let record = record(None);
        match store
            .enqueue_in(&mut transaction, record.clone())
            .await
            .unwrap()
        {
            EnqueueOutcome::Inserted(job_id) => assert_eq!(job_id, record.envelope.job_id),
            EnqueueOutcome::Duplicate(existing) => {
                panic!("expected insertion, got duplicate {existing}")
            }
        }
        transaction.commit().await.unwrap();
        let job = store
            .get(record.envelope.job_id)
            .await
            .unwrap()
            .expect("committed job");
        assert_eq!(job.status, JobStatus::Pending);
        let publications = store
            .claim_due(
                "probe",
                10,
                Utc::now() + chrono::TimeDelta::minutes(1),
                Utc::now(),
            )
            .await
            .unwrap();
        assert_eq!(publications.len(), 1);
        assert_eq!(publications[0].job_id, record.envelope.job_id);
    }

    #[tokio::test]
    async fn concurrent_execution_claims_admit_exactly_one_owner() {
        let Some(pool) = pool().await else { return };
        let _guard = test_lock().lock().await;
        let store = Arc::new(PostgresJobStore::new(pool));
        let record = record(None);
        store.enqueue_with_intent(record.clone()).await.unwrap();
        let store_a = store.clone();
        let store_b = store.clone();
        let now = Utc::now();
        let lease = now + chrono::TimeDelta::minutes(10);
        let job_id = record.envelope.job_id;
        let a = tokio::spawn(async move {
            store_a
                .claim_execution(job_id, "worker-a", lease, now)
                .await
        });
        let b = tokio::spawn(async move {
            store_b
                .claim_execution(job_id, "worker-b", lease, now)
                .await
        });
        let owners: usize = [a.await.unwrap().unwrap(), b.await.unwrap().unwrap()]
            .into_iter()
            .flatten()
            .count();
        assert_eq!(owners, 1, "only one live owner may exist");
    }

    #[tokio::test]
    async fn wrong_owner_cannot_complete_and_expired_leases_recover() {
        let Some(pool) = pool().await else { return };
        let _guard = test_lock().lock().await;
        let store = PostgresJobStore::new(pool);
        let record = record(None);
        store.enqueue_with_intent(record.clone()).await.unwrap();
        let now = Utc::now();
        let job_id = record.envelope.job_id;
        store
            .claim_execution(job_id, "worker-a", now + chrono::TimeDelta::minutes(1), now)
            .await
            .unwrap()
            .expect("claim");
        let error = store
            .complete(job_id, "worker-b", now + chrono::TimeDelta::seconds(1))
            .await
            .unwrap_err();
        assert!(matches!(error, JobError::LeaseOwnership { .. }));
        let recovered = store
            .recover_expired_leases(now + chrono::TimeDelta::minutes(2))
            .await
            .unwrap();
        assert_eq!(recovered, 1);
        let reclaimed = store
            .claim_execution(
                job_id,
                "worker-b",
                now + chrono::TimeDelta::minutes(30),
                now + chrono::TimeDelta::minutes(2),
            )
            .await
            .unwrap()
            .expect("reclaim after expiry");
        assert_eq!(reclaimed.attempt_count, 2);
        let error = store
            .complete(job_id, "worker-a", now + chrono::TimeDelta::minutes(3))
            .await
            .unwrap_err();
        assert!(
            matches!(error, JobError::LeaseOwnership { .. }),
            "stale owners cannot overwrite newer completion"
        );
    }

    #[tokio::test]
    async fn duplicate_dedupe_submissions_are_deterministic() {
        let Some(pool) = pool().await else { return };
        let _guard = test_lock().lock().await;
        let store = PostgresJobStore::new(pool);
        store
            .enqueue_with_intent(record(Some("orders.confirm:o-1")))
            .await
            .unwrap();
        match store
            .enqueue_with_intent(record(Some("orders.confirm:o-1")))
            .await
            .unwrap()
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
        let error = store.enqueue_with_intent(conflicting).await.unwrap_err();
        assert!(matches!(
            error,
            JobError::DuplicateSubmissionConflict { .. }
        ));
    }

    #[tokio::test]
    async fn operator_transitions_are_revision_guarded() {
        let Some(pool) = pool().await else { return };
        let _guard = test_lock().lock().await;
        let store = PostgresJobStore::new(pool);
        let record = record(None);
        store.enqueue_with_intent(record.clone()).await.unwrap();
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
        let error = store
            .retry_failed(job_id, current.revision + 1, Utc::now())
            .await
            .unwrap_err();
        assert!(matches!(error, JobError::InvalidTransition { .. }));
    }

    #[tokio::test]
    async fn concurrent_operator_retries_create_one_authoritative_transition() {
        let Some(pool) = pool().await else { return };
        let _guard = test_lock().lock().await;
        let store = Arc::new(PostgresJobStore::new(pool));
        let record = record(None);
        store.enqueue_with_intent(record.clone()).await.unwrap();
        let job_id = record.envelope.job_id;
        let now = Utc::now();
        store
            .claim_execution(job_id, "w", now + chrono::TimeDelta::minutes(5), now)
            .await
            .unwrap()
            .expect("claim");
        store
            .fail_permanently(job_id, "w", "JOBS-RETRIES-EXHAUSTED", now)
            .await
            .unwrap();
        let revision = store.get(job_id).await.unwrap().unwrap().revision;
        let a = {
            let store = store.clone();
            tokio::spawn(async move {
                store
                    .retry_failed(job_id, revision, now + chrono::TimeDelta::seconds(1))
                    .await
            })
        };
        let b = {
            let store = store.clone();
            tokio::spawn(async move {
                store
                    .retry_failed(job_id, revision, now + chrono::TimeDelta::seconds(1))
                    .await
            })
        };
        let outcomes = [a.await.unwrap(), b.await.unwrap()];
        let succeeded = outcomes.iter().filter(|result| result.is_ok()).count();
        let conflicted = outcomes
            .iter()
            .filter(|result| matches!(result, Err(JobError::RevisionConflict { .. })))
            .count();
        assert_eq!(succeeded, 1, "exactly one authoritative transition");
        assert_eq!(conflicted, 1);
    }

    #[tokio::test]
    async fn concurrent_publication_claims_are_disjoint() {
        let Some(pool) = pool().await else { return };
        let _guard = test_lock().lock().await;
        let store = Arc::new(PostgresJobStore::new(pool));
        for _ in 0..4 {
            store.enqueue_with_intent(record(None)).await.unwrap();
        }
        let now = Utc::now();
        let a = {
            let store = store.clone();
            tokio::spawn(async move {
                store
                    .claim_due("dispatcher-a", 10, now + chrono::TimeDelta::minutes(1), now)
                    .await
            })
        };
        let b = {
            let store = store.clone();
            tokio::spawn(async move {
                store
                    .claim_due("dispatcher-b", 10, now + chrono::TimeDelta::minutes(1), now)
                    .await
            })
        };
        let claimed_a = a.await.unwrap().unwrap();
        let claimed_b = b.await.unwrap().unwrap();
        assert_eq!(claimed_a.len() + claimed_b.len(), 4);
        let overlap = claimed_a
            .iter()
            .filter(|publication| {
                claimed_b
                    .iter()
                    .any(|other| other.job_id == publication.job_id)
            })
            .count();
        assert_eq!(overlap, 0, "claims must be disjoint");
    }

    #[tokio::test]
    async fn retry_state_round_trips_through_publication_republish() {
        let Some(pool) = pool().await else { return };
        let _guard = test_lock().lock().await;
        let store = PostgresJobStore::new(pool);
        let record = record(None);
        store.enqueue_with_intent(record.clone()).await.unwrap();
        let job_id = record.envelope.job_id;
        let now = Utc::now();
        store
            .claim_execution(job_id, "w", now + chrono::TimeDelta::minutes(5), now)
            .await
            .unwrap()
            .expect("claim");
        let retry_at = now + chrono::TimeDelta::seconds(60);
        store
            .schedule_retry(job_id, "w", "notification-unavailable", retry_at, now)
            .await
            .unwrap();
        store.republish(job_id, "w", retry_at).await.unwrap();
        let early = store
            .claim_due("d", 10, now + chrono::TimeDelta::minutes(1), now)
            .await
            .unwrap();
        assert!(early.is_empty(), "retry publication is not due yet");
        let due = store
            .claim_due("d", 10, retry_at + chrono::TimeDelta::minutes(1), retry_at)
            .await
            .unwrap();
        assert_eq!(due.len(), 1, "retry publication becomes due at retry_at");
    }
}
