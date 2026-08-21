//! Use-case-shaped ports for durable jobs.
//!
//! These are application contracts, not generic CRUD repositories. The job
//! row owns execution state; the publication row owns pending transport
//! publication; the SQS message is delivery, never authoritative state.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::envelope::JobEnvelope;

/// Durable job execution state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// Durable and awaiting delivery or a scheduled retry time.
    Pending,
    /// A worker holds the execution lease.
    Running,
    /// Terminal success.
    Succeeded,
    /// Terminal failure; inspectable and operator-retryable.
    FailedPermanently,
    /// Terminal cancellation; never executes.
    Cancelled,
}

impl JobStatus {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::FailedPermanently | Self::Cancelled
        )
    }
}

/// One bounded attempt-history entry. The newest entry is last.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobAttempt {
    pub attempt: u32,
    pub at: DateTime<Utc>,
    pub worker_id: String,
    pub outcome: JobAttemptOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobAttemptOutcome {
    Succeeded,
    Retried { code: String },
    FailedPermanently { code: String },
    ExpiredDeadline,
}

/// The authoritative durable job record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRecord {
    pub envelope: JobEnvelope,
    pub status: JobStatus,
    /// Monotonic transition revision used by guarded operator mutations.
    pub revision: u64,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub attempt_count: u32,
    /// Ordered, bounded attempt history (newest last).
    pub attempts: Vec<JobAttempt>,
    pub failure_code: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Publication state for the transport outbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationStatus {
    Pending,
    Claimed,
    Published,
    Failed,
}

/// Pending transport publication intent for one job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobPublication {
    pub job_id: Uuid,
    pub worker_profile: String,
    pub status: PublicationStatus,
    pub attempt_count: u32,
    pub available_at: DateTime<Utc>,
    pub claimed_by: Option<String>,
    pub claim_expires_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

/// Outcome of a duplicate-key submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueOutcome {
    /// A new durable job was inserted.
    Inserted(Uuid),
    /// An identical dedupe key with an identical payload already exists.
    Duplicate(Uuid),
}

/// Errors carry stable, public-safe codes; infrastructure details stay in the
/// redacted message.
#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("job envelope or policy is invalid: {0}")]
    InvalidJob(String),
    #[error("job envelope schema {0} is not supported by this build")]
    UnsupportedEnvelopeSchema(u16),
    #[error("job payload is {bytes} bytes; the maximum is {maximum}")]
    PayloadTooLarge { bytes: usize, maximum: usize },
    #[error("job envelope is {0} bytes; the envelope maximum was exceeded")]
    EnvelopeTooLarge(usize),
    #[error("transport message is not a valid job envelope: {0}")]
    InvalidTransportMessage(String),
    #[error("unknown job: {0}")]
    UnknownJob(String),
    #[error("job {job_name} version {job_version} is not supported")]
    UnsupportedJobVersion { job_name: String, job_version: u16 },
    #[error("handler for {job_name} version {job_version} is already registered")]
    DuplicateRegistration { job_name: String, job_version: u16 },
    #[error("job {existing_job_id} already claimed dedupe key; identical payloads are idempotent")]
    DuplicateSubmission { existing_job_id: Uuid },
    #[error("dedupe key is already held by job {existing_job_id} with a different payload")]
    DuplicateSubmissionConflict { existing_job_id: Uuid },
    #[error("job {0} does not exist")]
    MissingJob(Uuid),
    #[error("worker {worker_id} does not own the lease for job {job_id}")]
    LeaseOwnership { job_id: Uuid, worker_id: String },
    #[error("job {job_id} changed under revision {expected_revision}")]
    RevisionConflict {
        job_id: Uuid,
        expected_revision: u64,
    },
    #[error("job {job_id} is not in a state that accepts this transition")]
    InvalidTransition { job_id: Uuid },
    #[error("claims require a worker ID, positive limit and future lease")]
    InvalidClaim,
    #[error("job infrastructure failed: {0}")]
    Infrastructure(String),
}

impl JobError {
    /// Stable, public-safe error code for diagnostics.
    #[must_use]
    pub const fn stable_code(&self) -> &'static str {
        match self {
            Self::InvalidJob(_) => "JOBS-INVALID-JOB",
            Self::UnsupportedEnvelopeSchema(_) => "JOBS-UNSUPPORTED-ENVELOPE-SCHEMA",
            Self::PayloadTooLarge { .. } => "JOBS-PAYLOAD-TOO-LARGE",
            Self::EnvelopeTooLarge(_) => "JOBS-ENVELOPE-TOO-LARGE",
            Self::InvalidTransportMessage(_) => "JOBS-INVALID-TRANSPORT-MESSAGE",
            Self::UnknownJob(_) => "JOBS-UNKNOWN-JOB",
            Self::UnsupportedJobVersion { .. } => "JOBS-UNSUPPORTED-VERSION",
            Self::DuplicateRegistration { .. } => "JOBS-DUPLICATE-REGISTRATION",
            Self::DuplicateSubmission { .. } => "JOBS-DUPLICATE-SUBMISSION",
            Self::DuplicateSubmissionConflict { .. } => "JOBS-DUPLICATE-SUBMISSION-CONFLICT",
            Self::MissingJob(_) => "JOBS-MISSING-JOB",
            Self::LeaseOwnership { .. } => "JOBS-LEASE-OWNERSHIP",
            Self::RevisionConflict { .. } => "JOBS-REVISION-CONFLICT",
            Self::InvalidTransition { .. } => "JOBS-INVALID-TRANSITION",
            Self::InvalidClaim => "JOBS-INVALID-CLAIM",
            Self::Infrastructure(_) => "JOBS-INFRASTRUCTURE",
        }
    }
}

/// Durable job state store. Implementations must make the execution claim a
/// single atomic compare-and-set: read-then-write in two statements is
/// non-conforming because duplicate deliveries would both win.
#[async_trait::async_trait]
pub trait JobStore: Send + Sync + std::fmt::Debug {
    /// Atomically insert the job and its publication intent in one
    /// transaction. SQL adapters additionally expose `enqueue_in` to share
    /// the caller's transaction.
    async fn enqueue_with_intent(&self, record: JobRecord) -> Result<EnqueueOutcome, JobError>;

    /// Atomically claim the execution lease for one delivery. Returns the
    /// claimed record, or `None` when the job is missing, already leased, not
    /// yet available or terminal.
    async fn claim_execution(
        &self,
        job_id: Uuid,
        worker_id: &str,
        lease_expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Option<JobRecord>, JobError>;

    /// Record terminal success and release the lease.
    async fn complete(
        &self,
        job_id: Uuid,
        worker_id: &str,
        now: DateTime<Utc>,
    ) -> Result<(), JobError>;

    /// Record a retryable failure and return the job to `pending` with the
    /// next availability time.
    async fn schedule_retry(
        &self,
        job_id: Uuid,
        worker_id: &str,
        failure_code: &str,
        next_available_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<(), JobError>;

    /// Record terminal failure and release the lease.
    async fn fail_permanently(
        &self,
        job_id: Uuid,
        worker_id: &str,
        failure_code: &str,
        now: DateTime<Utc>,
    ) -> Result<(), JobError>;

    /// Operator-guarded cancellation of a non-running job.
    async fn cancel(
        &self,
        job_id: Uuid,
        expected_revision: u64,
        now: DateTime<Utc>,
    ) -> Result<(), JobError>;

    /// Operator-guarded retry of a permanently failed job: back to `pending`
    /// at a fresh revision with cleared failure state.
    async fn retry_failed(
        &self,
        job_id: Uuid,
        expected_revision: u64,
        now: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, JobError>;

    /// Reset expired `running` leases back to `pending`. Returns the number
    /// recovered.
    async fn recover_expired_leases(&self, now: DateTime<Utc>) -> Result<usize, JobError>;

    async fn get(&self, job_id: Uuid) -> Result<Option<JobRecord>, JobError>;

    /// Bounded listing of permanently failed jobs for operators.
    async fn list_failed(&self, limit: usize) -> Result<Vec<JobRecord>, JobError>;
}

/// Transport publication outbox. Claiming must be atomic across workers.
#[async_trait::async_trait]
pub trait JobPublicationStore: Send + Sync + std::fmt::Debug {
    /// Record publication intent (inside the submitter's transaction when the
    /// adapter provides `enqueue_intent_in`).
    async fn enqueue_intent(
        &self,
        job_id: Uuid,
        worker_profile: &str,
        available_at: DateTime<Utc>,
    ) -> Result<(), JobError>;

    /// Atomically claim up to `limit` due publications.
    async fn claim_due(
        &self,
        worker_id: &str,
        limit: usize,
        claim_expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Vec<JobPublication>, JobError>;

    async fn mark_published(&self, job_id: Uuid, worker_id: &str) -> Result<(), JobError>;

    async fn mark_failed(
        &self,
        job_id: Uuid,
        worker_id: &str,
        error: &str,
        retry_at: DateTime<Utc>,
    ) -> Result<(), JobError>;

    /// Re-open publication after a durable job retry was scheduled.
    async fn republish(
        &self,
        job_id: Uuid,
        worker_id: &str,
        available_at: DateTime<Utc>,
    ) -> Result<(), JobError>;

    async fn recover_expired_claims(&self, now: DateTime<Utc>) -> Result<usize, JobError>;
}

/// Publishes a serialized envelope to the selected transport.
///
/// The queue message is delivery, not state. `now` is passed so delayed
/// dispatch is deterministic under test clocks; adapters must honor the
/// envelope's availability time within their provider's real delay range or
/// fail before provider contact.
#[async_trait::async_trait]
pub trait JobDispatcher: Send + Sync + std::fmt::Debug {
    async fn dispatch(&self, envelope: &JobEnvelope, now: DateTime<Utc>) -> Result<(), JobError>;
}

/// Narrow overlap-lock port backing `without_overlapping` semantics. Never a
/// process mutex in production, never a general cache.
#[async_trait::async_trait]
pub trait OverlapLockStore: Send + Sync + std::fmt::Debug {
    /// Acquire the lock for `owner` until `expires_at`; false when held.
    async fn acquire(
        &self,
        overlap_key: &str,
        owner: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool, JobError>;

    /// Extend a held lock; false when the caller no longer owns it.
    async fn refresh(
        &self,
        overlap_key: &str,
        owner: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool, JobError>;

    /// Release a held lock. Releasing an unheld lock is a no-op.
    async fn release(&self, overlap_key: &str, owner: &str) -> Result<(), JobError>;

    /// Delete expired locks. Returns the number removed.
    async fn recover_expired(&self, now: DateTime<Utc>) -> Result<usize, JobError>;
}

/// Validate explicit dispatch/claim parameters shared by every adapter.
pub fn validate_worker_claim(
    worker_id: &str,
    limit: usize,
    expires_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<(), JobError> {
    if worker_id.trim().is_empty()
        || worker_id.len() > 128
        || worker_id.contains(|c: char| c.is_control())
    {
        return Err(JobError::InvalidClaim);
    }
    if limit == 0 || limit > 100 {
        return Err(JobError::InvalidClaim);
    }
    if expires_at <= now || expires_at > now + chrono::TimeDelta::hours(1) {
        return Err(JobError::InvalidClaim);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_codes_are_public_safe() {
        assert_eq!(
            JobError::UnknownJob("x".into()).stable_code(),
            "JOBS-UNKNOWN-JOB"
        );
        assert_eq!(
            JobError::Infrastructure("secret payload".into()).stable_code(),
            "JOBS-INFRASTRUCTURE"
        );
        let rendered = JobError::DuplicateSubmissionConflict {
            existing_job_id: Uuid::nil(),
        }
        .to_string();
        assert!(!rendered.contains("secret"));
    }

    #[test]
    fn terminal_states_are_classified() {
        assert!(JobStatus::Succeeded.is_terminal());
        assert!(JobStatus::Cancelled.is_terminal());
        assert!(JobStatus::FailedPermanently.is_terminal());
        assert!(!JobStatus::Pending.is_terminal());
        assert!(!JobStatus::Running.is_terminal());
    }
}
