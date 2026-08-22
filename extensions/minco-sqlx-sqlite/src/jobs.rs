//! Durable `SQLite` job storage: job rows, publication generations,
//! fenced execution leases and overlap locks.
//!
//! The job row owns execution state and the publication row owns pending
//! transport delivery; the queue message is never authoritative. `SQLite`
//! has one writer at a time, so claims run inside `BEGIN IMMEDIATE`
//! transactions; every mutation re-checks the claim's opaque lease
//! identity, and retry state plus the next publication generation commit
//! in one transaction.

use chrono::{DateTime, Utc};
use minco_plugin_jobs::{
    EnqueueOutcome, IngestOutcome, JobAttempt, JobClaim, JobError, JobPublication, JobRecord,
    JobStatus, PublicationStatus, semantic_fingerprint, validate_worker_claim,
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

    /// Insert the job and its first publication generation inside the
    /// caller's transaction so the business mutation and the durable
    /// dispatch commit atomically.
    pub async fn enqueue_in(
        &self,
        transaction: &mut Transaction<'_, sqlx::Sqlite>,
        record: JobRecord,
    ) -> Result<EnqueueOutcome, JobError> {
        Self::enqueue_record_in(transaction, record, "pending").await
    }

    /// Insert the job with its publication recorded as already delivered
    /// (Scheduler ingestion) inside the caller's transaction.
    pub async fn ingest_in(
        &self,
        transaction: &mut Transaction<'_, sqlx::Sqlite>,
        record: JobRecord,
    ) -> Result<IngestOutcome, JobError> {
        match Self::enqueue_record_in(transaction, record, "published").await? {
            EnqueueOutcome::Inserted(job_id) => Ok(IngestOutcome::Ingested(job_id)),
            EnqueueOutcome::Duplicate(existing) => Ok(IngestOutcome::Duplicate(existing)),
        }
    }

    async fn enqueue_record_in(
        transaction: &mut Transaction<'_, sqlx::Sqlite>,
        record: JobRecord,
        publication_status: &str,
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
        let fingerprint = semantic_fingerprint(&record.envelope);
        let result = sqlx::query(
            "INSERT INTO minco_jobs (job_id, worker_profile, envelope, fingerprint, status, \
             revision, available_at, attempt_count, lease_id, lease_expires_at, attempts, \
             dedupe_key, failure_code, completed_at) \
             VALUES ($1, $2, $3, $4, 'pending', $5, $6, $7, NULL, NULL, $8, $9, NULL, NULL)",
        )
        .bind(record.envelope.job_id.to_string())
        .bind(&record.envelope.worker_profile)
        .bind(&envelope_json)
        .bind(&fingerprint)
        .bind(i64::try_from(record.revision).unwrap_or(1))
        .bind(record.envelope.available_at.to_rfc3339())
        .bind(i64::from(record.attempt_count))
        .bind(&attempts)
        .bind(record.envelope.dedupe_key.as_deref())
        .execute(&mut **transaction)
        .await;
        if let Err(error) = result {
            let message = format!("{error}");
            if message.contains("minco_jobs.job_id") {
                // An identical re-ingestion (same job identity and
                // fingerprint) is an idempotent duplicate; anything else is
                // an error.
                let existing: Option<(String,)> =
                    sqlx::query_as("SELECT fingerprint FROM minco_jobs WHERE job_id = $1")
                        .bind(record.envelope.job_id.to_string())
                        .fetch_optional(&mut **transaction)
                        .await
                        .map_err(|_| {
                            JobError::Infrastructure("job identity probe failed".into())
                        })?;
                return match existing {
                    Some((existing_fingerprint,)) if existing_fingerprint == fingerprint => {
                        Ok(EnqueueOutcome::Duplicate(record.envelope.job_id))
                    }
                    _ => Err(JobError::DuplicateJobIdentity(record.envelope.job_id)),
                };
            }
            if message.contains("minco_jobs.dedupe_key") {
                return dedupe_outcome(&record, &fingerprint, transaction).await;
            }
            return Err(infrastructure(&error));
        }
        sqlx::query(
            "INSERT INTO minco_job_publications (publication_id, job_id, generation, \
             worker_profile, status, attempt_count, available_at, claimed_by, \
             claim_expires_at, lease_id, last_error) \
             VALUES ($1, $2, 1, $3, $4, 0, $5, NULL, NULL, NULL, NULL)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(record.envelope.job_id.to_string())
        .bind(&record.envelope.worker_profile)
        .bind(publication_status)
        .bind(record.envelope.available_at.to_rfc3339())
        .execute(&mut **transaction)
        .await
        .map_err(|_| JobError::Infrastructure("job publication insert failed".into()))?;
        Ok(EnqueueOutcome::Inserted(record.envelope.job_id))
    }
}

async fn dedupe_outcome(
    record: &JobRecord,
    fingerprint: &str,
    transaction: &mut Transaction<'_, sqlx::Sqlite>,
) -> Result<EnqueueOutcome, JobError> {
    let existing: Option<(String, String)> =
        sqlx::query_as("SELECT envelope, fingerprint FROM minco_jobs WHERE dedupe_key = $1")
            .bind(record.envelope.dedupe_key.as_deref())
            .fetch_optional(&mut **transaction)
            .await
            .map_err(|_| JobError::Infrastructure("job dedupe lookup failed".into()))?;
    let Some((existing_envelope, existing_fingerprint)) = existing else {
        return Err(JobError::Infrastructure(
            "job dedupe conflict vanished during insert".into(),
        ));
    };
    let existing: serde_json::Value = serde_json::from_str(&existing_envelope)
        .map_err(|_| JobError::Infrastructure("existing job envelope decode failed".into()))?;
    let existing_id = existing
        .get("job_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse::<Uuid>().ok());
    match (existing_fingerprint == fingerprint, existing_id) {
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

fn optional_timestamp(value: Option<&str>) -> Result<Option<DateTime<Utc>>, JobError> {
    value.map(parse_timestamp).transpose()
}

fn optional_uuid(value: Option<&str>) -> Option<Uuid> {
    value.and_then(|text| text.parse::<Uuid>().ok())
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
        lease_id: optional_uuid(
            row.try_get::<Option<String>, _>("lease_id")
                .map_err(|_| JobError::Infrastructure("job lease identity unreadable".into()))?
                .as_deref(),
        ),
        lease_expires_at: optional_timestamp(
            row.try_get::<Option<String>, _>("lease_expires_at")
                .map_err(|_| JobError::Infrastructure("job lease expiry unreadable".into()))?
                .as_deref(),
        )?,
        attempt_count: u32::try_from(attempt_count).unwrap_or(0),
        attempts,
        failure_code: row
            .try_get("failure_code")
            .map_err(|_| JobError::Infrastructure("job failure code unreadable".into()))?,
        completed_at: optional_timestamp(
            row.try_get::<Option<String>, _>("completed_at")
                .map_err(|_| JobError::Infrastructure("job completion unreadable".into()))?
                .as_deref(),
        )?,
    })
}

fn attempt_entry(
    attempt: u32,
    worker_execution_id: &str,
    now: DateTime<Utc>,
    outcome: minco_plugin_jobs::JobAttemptOutcome,
) -> Result<String, JobError> {
    serde_json::to_string(&JobAttempt {
        attempt,
        at: now,
        worker_execution_id: worker_execution_id.to_owned(),
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
    let publication_id: String = row
        .try_get("publication_id")
        .map_err(|_| JobError::Infrastructure("publication identity unreadable".into()))?;
    let job_id: String = row
        .try_get("job_id")
        .map_err(|_| JobError::Infrastructure("publication job unreadable".into()))?;
    Ok(JobPublication {
        publication_id: Uuid::parse_str(&publication_id)
            .map_err(|_| JobError::Infrastructure("publication identity decode failed".into()))?,
        job_id: Uuid::parse_str(&job_id)
            .map_err(|_| JobError::Infrastructure("publication job decode failed".into()))?,
        generation: u32::try_from(
            row.try_get::<i64, _>("generation").map_err(|_| {
                JobError::Infrastructure("publication generation unreadable".into())
            })?,
        )
        .unwrap_or(1),
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
        claim_expires_at: optional_timestamp(
            row.try_get::<Option<String>, _>("claim_expires_at")
                .map_err(|_| {
                    JobError::Infrastructure("publication claim expiry unreadable".into())
                })?
                .as_deref(),
        )?,
        lease_id: optional_uuid(
            row.try_get::<Option<String>, _>("lease_id")
                .map_err(|_| JobError::Infrastructure("publication lease unreadable".into()))?
                .as_deref(),
        ),
        last_error: row
            .try_get("last_error")
            .map_err(|_| JobError::Infrastructure("publication error unreadable".into()))?,
    })
}

async fn verify_job_update(
    pool: &SqlitePool,
    job_id: Uuid,
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
        Err(JobError::LeaseFencedOut { job_id })
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
    publication_id: Uuid,
    result: &sqlx::sqlite::SqliteQueryResult,
) -> Result<(), JobError> {
    if result.rows_affected() == 1 {
        return Ok(());
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM minco_job_publications WHERE publication_id = $1)",
    )
    .bind(publication_id.to_string())
    .fetch_one(pool)
    .await
    .map_err(|_| JobError::Infrastructure("publication probe failed".into()))?;
    if exists {
        Err(JobError::PublicationFencedOut { publication_id })
    } else {
        Err(JobError::MissingJob(publication_id))
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

    async fn ingest_existing_delivery(&self, record: JobRecord) -> Result<IngestOutcome, JobError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| infrastructure(&error))?;
        let outcome = self.ingest_in(&mut transaction, record).await?;
        transaction
            .commit()
            .await
            .map_err(|error| infrastructure(&error))?;
        Ok(outcome)
    }

    async fn claim_execution(
        &self,
        job_id: Uuid,
        worker_execution_id: &str,
        lease_expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Option<JobClaim>, JobError> {
        validate_worker_claim(worker_execution_id, 1, lease_expires_at, now)?;
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
        let lease_id = Uuid::now_v7();
        sqlx::query(
            "UPDATE minco_jobs SET status = 'running', lease_id = $2, lease_expires_at = $3, \
             attempt_count = attempt_count + 1, revision = revision + 1 WHERE job_id = $1",
        )
        .bind(job_id.to_string())
        .bind(lease_id.to_string())
        .bind(lease_expires_at.to_rfc3339())
        .execute(&mut *transaction)
        .await
        .map_err(|error| infrastructure(&error))?;
        let row = sqlx::query(
            "SELECT job_id, worker_profile, envelope, status, revision, available_at, \
             attempt_count, lease_id, lease_expires_at, attempts, dedupe_key, failure_code, \
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
        let record = decode_job_row(&row)?;
        Ok(Some(JobClaim {
            fence: record.revision,
            record,
            lease_id,
        }))
    }

    async fn complete(&self, claim: &JobClaim, now: DateTime<Utc>) -> Result<(), JobError> {
        let job_id = claim.record.envelope.job_id;
        let entry = attempt_entry(
            claim.record.attempt_count,
            &format!("lease-{}", claim.lease_id),
            now,
            minco_plugin_jobs::JobAttemptOutcome::Succeeded,
        )?;
        let result = sqlx::query(
            "UPDATE minco_jobs SET status = 'succeeded', lease_id = NULL, lease_expires_at = \
             NULL, failure_code = NULL, completed_at = $3, revision = revision + 1, \
             attempts = json_insert(CASE WHEN json_array_length(attempts) >= 25 \
             THEN json_remove(attempts, '$[0]') ELSE attempts END, '$[#]', json($4)) \
             WHERE job_id = $1 AND status = 'running' AND lease_id = $2",
        )
        .bind(job_id.to_string())
        .bind(claim.lease_id.to_string())
        .bind(now.to_rfc3339())
        .bind(entry)
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        verify_job_update(&self.pool, job_id, &result).await
    }

    async fn schedule_retry_and_publish(
        &self,
        claim: &JobClaim,
        failure_code: &str,
        next_available_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Uuid, JobError> {
        let job_id = claim.record.envelope.job_id;
        let entry = attempt_entry(
            claim.record.attempt_count,
            &format!("lease-{}", claim.lease_id),
            now,
            minco_plugin_jobs::JobAttemptOutcome::Retried {
                code: failure_code.to_owned(),
            },
        )?;
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| infrastructure(&error))?;
        let result = sqlx::query(
            "UPDATE minco_jobs SET status = 'pending', lease_id = NULL, lease_expires_at = \
             NULL, available_at = $4, failure_code = $3, revision = revision + 1, \
             attempts = json_insert(CASE WHEN json_array_length(attempts) >= 25 \
             THEN json_remove(attempts, '$[0]') ELSE attempts END, '$[#]', json($5)) \
             WHERE job_id = $1 AND status = 'running' AND lease_id = $2",
        )
        .bind(job_id.to_string())
        .bind(claim.lease_id.to_string())
        .bind(failure_code)
        .bind(next_available_at.to_rfc3339())
        .bind(entry)
        .execute(&mut *transaction)
        .await
        .map_err(|error| infrastructure(&error))?;
        if result.rows_affected() != 1 {
            transaction
                .rollback()
                .await
                .map_err(|error| infrastructure(&error))?;
            verify_job_update(&self.pool, job_id, &result).await?;
            return Err(JobError::LeaseFencedOut { job_id });
        }
        let publication_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO minco_job_publications (publication_id, job_id, generation, \
             worker_profile, status, attempt_count, available_at, claimed_by, \
             claim_expires_at, lease_id, last_error) \
             SELECT $1, $2, COALESCE(MAX(generation), 0) + 1, $3, 'pending', 0, $4, NULL, \
             NULL, NULL, NULL FROM minco_job_publications WHERE job_id = $2",
        )
        .bind(publication_id.to_string())
        .bind(job_id.to_string())
        .bind(&claim.record.envelope.worker_profile)
        .bind(next_available_at.to_rfc3339())
        .execute(&mut *transaction)
        .await
        .map_err(|error| infrastructure(&error))?;
        transaction
            .commit()
            .await
            .map_err(|error| infrastructure(&error))?;
        Ok(publication_id)
    }

    async fn fail_permanently(
        &self,
        claim: &JobClaim,
        failure_code: &str,
        now: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let job_id = claim.record.envelope.job_id;
        let entry = attempt_entry(
            claim.record.attempt_count,
            &format!("lease-{}", claim.lease_id),
            now,
            minco_plugin_jobs::JobAttemptOutcome::FailedPermanently {
                code: failure_code.to_owned(),
            },
        )?;
        let result = sqlx::query(
            "UPDATE minco_jobs SET status = 'failed_permanently', lease_id = NULL, \
             lease_expires_at = NULL, failure_code = $3, completed_at = $4, revision = revision + 1, \
             attempts = json_insert(CASE WHEN json_array_length(attempts) >= 25 \
             THEN json_remove(attempts, '$[0]') ELSE attempts END, '$[#]', json($5)) \
             WHERE job_id = $1 AND status = 'running' AND lease_id = $2",
        )
        .bind(job_id.to_string())
        .bind(claim.lease_id.to_string())
        .bind(failure_code)
        .bind(now.to_rfc3339())
        .bind(entry)
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        verify_job_update(&self.pool, job_id, &result).await
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
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| infrastructure(&error))?;
        let result = sqlx::query(
            "UPDATE minco_jobs SET status = 'pending', failure_code = NULL, completed_at = NULL, \
             available_at = $3, attempt_count = 0, lease_id = NULL, lease_expires_at = NULL, \
             revision = revision + 1 WHERE job_id = $1 AND revision = $2 AND status = \
             'failed_permanently'",
        )
        .bind(job_id.to_string())
        .bind(i64::try_from(expected_revision).unwrap_or(-1))
        .bind(now.to_rfc3339())
        .execute(&mut *transaction)
        .await
        .map_err(|error| infrastructure(&error))?;
        if result.rows_affected() != 1 {
            transaction
                .rollback()
                .await
                .map_err(|error| infrastructure(&error))?;
            verify_revision_guard(&self.pool, job_id, expected_revision, &result).await?;
            return Err(JobError::RevisionConflict {
                job_id,
                expected_revision,
            });
        }
        let profile: String =
            sqlx::query_scalar("SELECT worker_profile FROM minco_jobs WHERE job_id = $1")
                .bind(job_id.to_string())
                .fetch_one(&mut *transaction)
                .await
                .map_err(|error| infrastructure(&error))?;
        let publication_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO minco_job_publications (publication_id, job_id, generation, \
             worker_profile, status, attempt_count, available_at, claimed_by, \
             claim_expires_at, lease_id, last_error) \
             SELECT $1, $2, COALESCE(MAX(generation), 0) + 1, $3, 'pending', 0, $4, NULL, \
             NULL, NULL, NULL FROM minco_job_publications WHERE job_id = $2",
        )
        .bind(publication_id.to_string())
        .bind(job_id.to_string())
        .bind(&profile)
        .bind(now.to_rfc3339())
        .execute(&mut *transaction)
        .await
        .map_err(|error| infrastructure(&error))?;
        transaction
            .commit()
            .await
            .map_err(|error| infrastructure(&error))?;
        Ok(now)
    }

    async fn recover_expired_leases(&self, now: DateTime<Utc>) -> Result<usize, JobError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| infrastructure(&error))?;
        let recovered: Vec<(String, String)> = sqlx::query_as(
            "UPDATE minco_jobs SET status = 'pending', lease_id = NULL, lease_expires_at = \
             NULL, available_at = $1, revision = revision + 1 \
             WHERE status = 'running' AND lease_expires_at IS NOT NULL AND lease_expires_at <= $1 \
             RETURNING job_id, worker_profile",
        )
        .bind(now.to_rfc3339())
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| infrastructure(&error))?;
        let count = recovered.len();
        for (job_id, profile) in &recovered {
            let publication_id = Uuid::now_v7();
            sqlx::query(
                "INSERT INTO minco_job_publications (publication_id, job_id, generation, \
                 worker_profile, status, attempt_count, available_at, claimed_by, \
                 claim_expires_at, lease_id, last_error) \
                 SELECT $1, $2, COALESCE(MAX(generation), 0) + 1, $3, 'pending', 0, $4, NULL, \
                 NULL, NULL, NULL FROM minco_job_publications WHERE job_id = $2",
            )
            .bind(publication_id.to_string())
            .bind(job_id)
            .bind(profile)
            .bind(now.to_rfc3339())
            .execute(&mut *transaction)
            .await
            .map_err(|error| infrastructure(&error))?;
        }
        transaction
            .commit()
            .await
            .map_err(|error| infrastructure(&error))?;
        Ok(count)
    }

    async fn get(&self, job_id: Uuid) -> Result<Option<JobRecord>, JobError> {
        let row = sqlx::query(
            "SELECT job_id, worker_profile, envelope, status, revision, available_at, \
             attempt_count, lease_id, lease_expires_at, attempts, dedupe_key, failure_code, \
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
             attempt_count, lease_id, lease_expires_at, attempts, dedupe_key, failure_code, \
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

#[async_trait::async_trait]
impl minco_plugin_jobs::JobPublicationStore for SqliteJobStore {
    async fn claim_due(
        &self,
        worker_execution_id: &str,
        limit: usize,
        claim_expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Vec<JobPublication>, JobError> {
        validate_worker_claim(worker_execution_id, limit, claim_expires_at, now)?;
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| infrastructure(&error))?;
        let ids: Vec<(String,)> = sqlx::query_as(
            "SELECT publication_id FROM minco_job_publications \
             WHERE (status IN ('pending', 'failed') AND available_at <= $1) \
             OR (status = 'claimed' AND claim_expires_at IS NOT NULL AND claim_expires_at <= $1) \
             ORDER BY available_at, publication_id LIMIT $2",
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
            builder.push_bind(worker_execution_id.to_owned());
            builder.push(", claim_expires_at = ");
            builder.push_bind(claim_expires_at.to_rfc3339());
            builder.push(", lease_id = ");
            builder.push_bind(Uuid::now_v7().to_string());
            builder.push(", attempt_count = attempt_count + 1 WHERE publication_id IN (");
            let mut separated = builder.separated(", ");
            for (publication_id,) in &ids {
                separated.push_bind(publication_id.clone());
            }
            builder.push(
                ") RETURNING publication_id, job_id, generation, worker_profile, status, \
                 attempt_count, available_at, claimed_by, claim_expires_at, lease_id, \
                 last_error",
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

    async fn mark_published(&self, publication_id: Uuid, lease_id: Uuid) -> Result<(), JobError> {
        let result = sqlx::query(
            "UPDATE minco_job_publications SET status = 'published', claimed_by = NULL, \
             claim_expires_at = NULL, lease_id = NULL, last_error = NULL \
             WHERE publication_id = $1 AND status = 'claimed' AND lease_id = $2",
        )
        .bind(publication_id.to_string())
        .bind(lease_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        verify_publication_update(&self.pool, publication_id, &result).await
    }

    async fn mark_failed(
        &self,
        publication_id: Uuid,
        lease_id: Uuid,
        error: &str,
        retry_at: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let result = sqlx::query(
            "UPDATE minco_job_publications SET status = 'failed', claimed_by = NULL, \
             claim_expires_at = NULL, lease_id = NULL, available_at = $3, last_error = $4 \
             WHERE publication_id = $1 AND status = 'claimed' AND lease_id = $2",
        )
        .bind(publication_id.to_string())
        .bind(lease_id.to_string())
        .bind(retry_at.to_rfc3339())
        .bind(error)
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        verify_publication_update(&self.pool, publication_id, &result).await
    }

    async fn recover_expired_claims(&self, now: DateTime<Utc>) -> Result<usize, JobError> {
        let result = sqlx::query(
            "UPDATE minco_job_publications SET status = 'pending', claimed_by = NULL, \
             claim_expires_at = NULL, lease_id = NULL \
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
        lease_id: Uuid,
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
            .bind(lease_id.to_string())
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
                .bind(lease_id.to_string())
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
        lease_id: Uuid,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool, JobError> {
        let result = sqlx::query(
            "UPDATE minco_job_locks SET expires_at = $3 \
             WHERE overlap_key = $1 AND owner = $2 AND expires_at > $4",
        )
        .bind(overlap_key)
        .bind(lease_id.to_string())
        .bind(expires_at.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|error| infrastructure(&error))?;
        Ok(result.rows_affected() == 1)
    }

    async fn release(&self, overlap_key: &str, lease_id: Uuid) -> Result<(), JobError> {
        sqlx::query("DELETE FROM minco_job_locks WHERE overlap_key = $1 AND owner = $2")
            .bind(overlap_key)
            .bind(lease_id.to_string())
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
        EnqueueOutcome, JobEnvelope, JobOptions, JobPublicationStore as _, JobStore as _,
        OverlapLockStore as _, RetryPolicy,
    };

    async fn pool() -> SqlitePool {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("jobs.sqlite");
        std::mem::forget(directory);
        crate::plugin_adapters::migrate_plugin_storage(
            &SqlitePool::connect(&format!("sqlite://{}?mode=rwc", path.display()))
                .await
                .expect("sqlite pool"),
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
        assert!(
            store
                .get(record.envelope.job_id)
                .await
                .expect("get")
                .is_none()
        );
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
    async fn enqueue_in_commits_exactly_one_recoverable_generation() {
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
        assert_eq!(publications[0].generation, 1);
        assert!(publications[0].lease_id.is_some());
    }

    #[tokio::test]
    async fn stale_claims_cannot_mutate_newer_claims_even_with_one_worker_name() {
        let pool = pool().await;
        let store = SqliteJobStore::new(pool);
        let record = record(None);
        store
            .enqueue_with_intent(record.clone())
            .await
            .expect("enqueue");
        let start = Utc::now();
        let stale = store
            .claim_execution(
                record.envelope.job_id,
                "same-worker-name",
                start + chrono::TimeDelta::minutes(1),
                start,
            )
            .await
            .expect("first claim")
            .expect("claimed");
        let newer = store
            .claim_execution(
                record.envelope.job_id,
                "same-worker-name",
                start + chrono::TimeDelta::minutes(30),
                start + chrono::TimeDelta::minutes(2),
            )
            .await
            .expect("reclaim")
            .expect("reclaimed");
        assert_ne!(stale.lease_id, newer.lease_id);
        let error = store.complete(&stale, start).await.unwrap_err();
        assert!(matches!(error, JobError::LeaseFencedOut { .. }));
        let error = store
            .schedule_retry_and_publish(
                &stale,
                "stale",
                start + chrono::TimeDelta::minutes(3),
                start,
            )
            .await
            .unwrap_err();
        assert!(matches!(error, JobError::LeaseFencedOut { .. }));
        let error = store
            .fail_permanently(&stale, "stale", start)
            .await
            .unwrap_err();
        assert!(matches!(error, JobError::LeaseFencedOut { .. }));
        store.complete(&newer, start).await.expect("newer owns it");
    }

    #[tokio::test]
    async fn retry_state_and_next_generation_commit_together() {
        let pool = pool().await;
        let store = SqliteJobStore::new(pool);
        let record = record(None);
        store
            .enqueue_with_intent(record.clone())
            .await
            .expect("enqueue");
        let now = Utc::now();
        let delivered = store
            .claim_due("dispatcher-1", 10, now + chrono::TimeDelta::minutes(1), now)
            .await
            .expect("deliver generation 1");
        assert_eq!(delivered.len(), 1);
        store
            .mark_published(
                delivered[0].publication_id,
                delivered[0].lease_id.expect("lease"),
            )
            .await
            .expect("published");
        let claim = store
            .claim_execution(
                record.envelope.job_id,
                "worker-exec-1",
                now + chrono::TimeDelta::minutes(5),
                now,
            )
            .await
            .expect("claim")
            .expect("claimed");
        let retry_at = now + chrono::TimeDelta::seconds(60);
        let publication_id = store
            .schedule_retry_and_publish(&claim, "notification-unavailable", retry_at, now)
            .await
            .expect("atomic retry");
        assert_ne!(publication_id, Uuid::nil());
        let generations: Vec<(i64, String)> = sqlx::query_as(
            "SELECT generation, status FROM minco_job_publications WHERE job_id = $1 ORDER BY \
             generation",
        )
        .bind(record.envelope.job_id.to_string())
        .fetch_all(&store.pool)
        .await
        .expect("generations");
        assert_eq!(
            generations.len(),
            2,
            "generation 2 committed with the retry"
        );
        assert_eq!(generations[1].1, "pending");
        let early = store
            .claim_due("d", 10, now + chrono::TimeDelta::minutes(1), now)
            .await
            .expect("early");
        assert!(early.is_empty(), "generation 2 is not due yet");
        let due = store
            .claim_due("d", 10, retry_at + chrono::TimeDelta::minutes(1), retry_at)
            .await
            .expect("due");
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].generation, 2);
    }

    #[tokio::test]
    async fn execution_claims_admit_exactly_one_owner() {
        let pool = pool().await;
        let store = SqliteJobStore::new(pool);
        let record = record(None);
        store
            .enqueue_with_intent(record.clone())
            .await
            .expect("enqueue");
        let now = Utc::now();
        assert!(
            store
                .claim_execution(
                    record.envelope.job_id,
                    "worker-a",
                    now + chrono::TimeDelta::minutes(10),
                    now
                )
                .await
                .expect("first claim")
                .is_some()
        );
        assert!(
            store
                .claim_execution(
                    record.envelope.job_id,
                    "worker-b",
                    now + chrono::TimeDelta::minutes(10),
                    now
                )
                .await
                .expect("second claim")
                .is_none(),
            "a live lease blocks other owners"
        );
    }

    #[tokio::test]
    async fn duplicate_dedupe_uses_the_semantic_fingerprint() {
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
        let conflicting = record(None);
        let mut envelope = conflicting.envelope.clone();
        envelope.payload = serde_json::json!({ "order_id": "o-2" });
        envelope.dedupe_key = Some("orders.confirm:o-1".into());
        let error = store
            .enqueue_with_intent(minco_plugin_jobs::pending_record(envelope))
            .await
            .expect_err("conflict");
        assert!(matches!(
            error,
            JobError::DuplicateSubmissionConflict { .. }
        ));
    }

    #[tokio::test]
    async fn ingestion_creates_no_pending_publication() {
        let pool = pool().await;
        let store = SqliteJobStore::new(pool);
        let mut occurrence = record(None);
        occurrence.envelope.dedupe_key = Some("orders-nightly:2026-08-22T13:00:00Z".into());
        match store
            .ingest_existing_delivery(occurrence.clone())
            .await
            .expect("ingest")
        {
            minco_plugin_jobs::IngestOutcome::Ingested(job_id) => {
                assert_eq!(job_id, occurrence.envelope.job_id);
            }
            minco_plugin_jobs::IngestOutcome::Duplicate(existing) => {
                panic!("first occurrence ingests, got duplicate {existing}")
            }
        }
        let statuses: Vec<(String,)> =
            sqlx::query_as("SELECT status FROM minco_job_publications WHERE job_id = $1")
                .bind(occurrence.envelope.job_id.to_string())
                .fetch_all(&store.pool)
                .await
                .expect("statuses");
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].0, "published", "no pending generation appears");
        match store
            .ingest_existing_delivery(occurrence)
            .await
            .expect("re")
        {
            minco_plugin_jobs::IngestOutcome::Duplicate(_) => {}
            minco_plugin_jobs::IngestOutcome::Ingested(job_id) => {
                panic!("re-ingestion is idempotent, got {job_id}")
            }
        }
    }

    #[tokio::test]
    async fn stale_overlap_owner_cannot_release_a_newer_lock() {
        let pool = pool().await;
        let store = SqliteJobStore::new(pool);
        let now = Utc::now();
        let stale_lease = Uuid::now_v7();
        assert!(
            store
                .acquire(
                    "orders.confirm:o-1",
                    stale_lease,
                    now + chrono::TimeDelta::minutes(1),
                    now
                )
                .await
                .expect("acquire")
        );
        let newer_lease = Uuid::now_v7();
        assert!(
            store
                .acquire(
                    "orders.confirm:o-1",
                    newer_lease,
                    now + chrono::TimeDelta::minutes(30),
                    now + chrono::TimeDelta::minutes(2)
                )
                .await
                .expect("reclaim"),
            "the expired lock is reclaimable"
        );
        store
            .release("orders.confirm:o-1", stale_lease)
            .await
            .expect("stale release is a no-op");
        let held: Option<(String,)> =
            sqlx::query_as("SELECT owner FROM minco_job_locks WHERE overlap_key = $1")
                .bind("orders.confirm:o-1")
                .fetch_optional(&store.pool)
                .await
                .expect("held");
        assert_eq!(
            held.map(|(owner,)| owner),
            Some(newer_lease.to_string()),
            "the stale owner cannot release the newer lock"
        );
    }

    #[tokio::test]
    async fn stale_publication_claimant_cannot_mark_delivery() {
        let pool = pool().await;
        let store = SqliteJobStore::new(pool);
        store
            .enqueue_with_intent(record(None))
            .await
            .expect("enqueue");
        let now = Utc::now();
        let stale = store
            .claim_due("dispatcher-a", 10, now + chrono::TimeDelta::minutes(1), now)
            .await
            .expect("stale claim");
        assert_eq!(stale.len(), 1);
        let stale_lease = stale[0].lease_id.expect("lease");
        let newer = store
            .claim_due(
                "dispatcher-b",
                10,
                now + chrono::TimeDelta::minutes(30),
                now + chrono::TimeDelta::minutes(2),
            )
            .await
            .expect("reclaim");
        assert_eq!(newer.len(), 1);
        let error = store
            .mark_published(stale[0].publication_id, stale_lease)
            .await
            .expect_err("fenced");
        assert!(matches!(error, JobError::PublicationFencedOut { .. }));
        store
            .mark_published(
                newer[0].publication_id,
                newer[0].lease_id.expect("newer lease"),
            )
            .await
            .expect("newer marks delivery");
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
            let claim = store
                .claim_execution(
                    record.envelope.job_id,
                    "worker-exec",
                    now + chrono::TimeDelta::minutes(5),
                    now,
                )
                .await
                .expect("claim")
                .expect("claimed");
            store
                .schedule_retry_and_publish(
                    &claim,
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
    }
}
