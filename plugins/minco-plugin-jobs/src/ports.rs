//! Use-case-shaped ports for durable jobs.
//!
//! These are application contracts, not generic CRUD repositories. The
//! durable job row is the authority: a delivery only locates the job, and
//! execution always uses the claimed record's envelope after a semantic
//! fingerprint check. Every mutation is fenced by the opaque claim identity
//! issued with the lease, so a stale invocation — even one reusing the same
//! worker name — cannot alter a newer claim's state.

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
    pub worker_execution_id: String,
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
    /// Opaque execution-lease identity fencing all mutations.
    pub lease_id: Option<Uuid>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub attempt_count: u32,
    /// Ordered, bounded attempt history (newest last).
    pub attempts: Vec<JobAttempt>,
    pub failure_code: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// A claimed execution: the authoritative record plus its opaque lease
/// identity.
///
/// Every subsequent mutation must present that identity. The fence value is
/// the record revision at claim time; presenting a stale lease fails closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobClaim {
    pub record: JobRecord,
    pub lease_id: Uuid,
    pub fence: u64,
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

/// One durable transport publication generation of one job.
///
/// `publication_id` is the stable identity of a single logical send: an
/// ambiguous resend of the same send reuses it (and therefore the same FIFO
/// deduplication identity), while a new retry generation mints a new one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobPublication {
    pub publication_id: Uuid,
    pub job_id: Uuid,
    /// 1-based generation; each durable retry opens the next generation.
    pub generation: u32,
    pub worker_profile: String,
    pub status: PublicationStatus,
    pub attempt_count: u32,
    pub available_at: DateTime<Utc>,
    pub claimed_by: Option<String>,
    pub claim_expires_at: Option<DateTime<Utc>>,
    /// Opaque publication-lease identity fencing publication mutations.
    pub lease_id: Option<Uuid>,
    pub last_error: Option<String>,
}

/// Outcome of a duplicate-key submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueOutcome {
    /// A new durable job was inserted.
    Inserted(Uuid),
    /// A semantically identical job already existed under the dedupe key.
    Duplicate(Uuid),
}

/// Outcome of ingesting a delivery that already exists on the transport
/// (Scheduler occurrences): the job is created or located with its
/// publication marked delivered, never pending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestOutcome {
    /// The occurrence was newly ingested with a delivered publication.
    Ingested(Uuid),
    /// The occurrence had already been ingested; the existing job identity
    /// is returned and no second publication was created.
    Duplicate(Uuid),
}

/// Errors carry stable, public-safe codes; infrastructure details stay in
/// the redacted message.
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
    #[error("job {existing_job_id} already claimed dedupe key; identical jobs are idempotent")]
    DuplicateSubmission { existing_job_id: Uuid },
    #[error(
        "dedupe key is already held by job {existing_job_id} with a different semantic fingerprint"
    )]
    DuplicateSubmissionConflict { existing_job_id: Uuid },
    #[error("job {0} does not exist")]
    MissingJob(Uuid),
    #[error("job identity {0} already exists with different semantics")]
    DuplicateJobIdentity(Uuid),
    #[error("the presented lease no longer owns job {job_id}")]
    LeaseFencedOut { job_id: Uuid },
    #[error("the presented publication lease no longer owns publication {publication_id}")]
    PublicationFencedOut { publication_id: Uuid },
    #[error("job {job_id} changed under revision {expected_revision}")]
    RevisionConflict {
        job_id: Uuid,
        expected_revision: u64,
    },
    #[error("job {job_id} is not in a state that accepts this transition")]
    InvalidTransition { job_id: Uuid },
    #[error("claims require a worker execution ID, positive limit and future lease")]
    InvalidClaim,
    #[error("worker profile {0} has no dispatch route")]
    UnknownWorkerProfile(String),
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
            Self::DuplicateJobIdentity(_) => "JOBS-DUPLICATE-JOB-IDENTITY",
            Self::LeaseFencedOut { .. } => "JOBS-LEASE-FENCED-OUT",
            Self::PublicationFencedOut { .. } => "JOBS-PUBLICATION-FENCED-OUT",
            Self::RevisionConflict { .. } => "JOBS-REVISION-CONFLICT",
            Self::InvalidTransition { .. } => "JOBS-INVALID-TRANSITION",
            Self::InvalidClaim => "JOBS-INVALID-CLAIM",
            Self::UnknownWorkerProfile(_) => "JOBS-UNKNOWN-WORKER-PROFILE",
            Self::Infrastructure(_) => "JOBS-INFRASTRUCTURE",
        }
    }
}

/// Durable job state store.
///
/// The execution claim is a single atomic compare-and-set that issues an
/// opaque lease identity; every mutation must present that identity and is
/// rejected once a newer claim, recovery or operator transition intervened.
#[async_trait::async_trait]
pub trait JobStore: Send + Sync + std::fmt::Debug {
    /// Atomically insert the job and its first pending publication
    /// generation. SQL adapters additionally expose `enqueue_in` to share
    /// the caller's transaction.
    async fn enqueue_with_intent(&self, record: JobRecord) -> Result<EnqueueOutcome, JobError>;

    /// Atomically claim the execution lease for one delivery. The
    /// `worker_execution_id` must be unique per invocation (not a static
    /// worker name); the returned claim's opaque `lease_id` is the sole
    /// mutation authority. Returns `None` when the job is missing, already
    /// leased, not yet available or terminal.
    async fn claim_execution(
        &self,
        job_id: Uuid,
        worker_execution_id: &str,
        lease_expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Option<JobClaim>, JobError>;

    /// Record terminal success and release the lease, fenced by the claim.
    async fn complete(&self, claim: &JobClaim, now: DateTime<Utc>) -> Result<(), JobError>;

    /// Atomically record a retryable failure: verify the claim fence,
    /// append the attempt, return the job to `pending` at the next
    /// availability time, clear the execution lease and insert the next
    /// pending publication generation — all in one transaction, so no
    /// intermediate committed state can strand the job.
    async fn schedule_retry_and_publish(
        &self,
        claim: &JobClaim,
        failure_code: &str,
        next_available_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Uuid, JobError>;

    /// Record terminal failure and release the lease, fenced by the claim.
    async fn fail_permanently(
        &self,
        claim: &JobClaim,
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

    /// Operator-guarded retry of a permanently failed job: back to
    /// `pending` at a fresh revision with a new pending publication
    /// generation, atomically.
    async fn retry_failed(
        &self,
        job_id: Uuid,
        expected_revision: u64,
        now: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, JobError>;

    /// Reset expired `running` leases back to `pending` and open a pending
    /// publication generation for each recovered job. Returns the number
    /// recovered.
    async fn recover_expired_leases(&self, now: DateTime<Utc>) -> Result<usize, JobError>;

    async fn get(&self, job_id: Uuid) -> Result<Option<JobRecord>, JobError>;

    /// Bounded listing of permanently failed jobs for operators.
    async fn list_failed(&self, limit: usize) -> Result<Vec<JobRecord>, JobError>;

    /// Atomically ingest a delivery that already exists on the transport
    /// (Scheduler occurrences): create the job with its publication marked
    /// delivered, or locate the existing occurrence, without ever inserting
    /// a pending publication.
    async fn ingest_existing_delivery(&self, record: JobRecord) -> Result<IngestOutcome, JobError>;
}

/// Transport publication outbox. Claims are atomic and fenced by an opaque
/// publication lease identity.
#[async_trait::async_trait]
pub trait JobPublicationStore: Send + Sync + std::fmt::Debug {
    /// Atomically claim up to `limit` due publications. The returned
    /// publications carry the claim's lease identity.
    async fn claim_due(
        &self,
        worker_execution_id: &str,
        limit: usize,
        claim_expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Vec<JobPublication>, JobError>;

    /// Mark one publication delivered, fenced by its lease.
    async fn mark_published(&self, publication_id: Uuid, lease_id: Uuid) -> Result<(), JobError>;

    /// Record a failed transport send with a retry time, fenced by the
    /// publication lease.
    async fn mark_failed(
        &self,
        publication_id: Uuid,
        lease_id: Uuid,
        error: &str,
        retry_at: DateTime<Utc>,
    ) -> Result<(), JobError>;

    /// Reset expired publication claims to pending. Returns the count.
    async fn recover_expired_claims(&self, now: DateTime<Utc>) -> Result<usize, JobError>;
}

/// One outbound transport delivery: the envelope plus the durable identity
/// of the publication being sent.
///
/// The publication identity — never the job identity — is the FIFO
/// deduplication identity, so an ambiguous resend of one send is suppressed
/// by the provider while a new retry generation is not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobDelivery {
    pub envelope: JobEnvelope,
    pub publication_id: Uuid,
}

/// Publishes one delivery to the selected transport. The queue message is
/// delivery, never state.
#[async_trait::async_trait]
pub trait JobDispatcher: Send + Sync + std::fmt::Debug {
    async fn dispatch(&self, delivery: &JobDelivery, now: DateTime<Utc>) -> Result<(), JobError>;
}

/// Narrow overlap-lock port backing `without_overlapping` semantics.
///
/// Locks are owned by opaque execution-lease identities, never reusable
/// worker names, so a stale claimant cannot release a newer owner's lock.
#[async_trait::async_trait]
pub trait OverlapLockStore: Send + Sync + std::fmt::Debug {
    /// Acquire the lock for `lease_id` until `expires_at`; false when held.
    async fn acquire(
        &self,
        overlap_key: &str,
        lease_id: Uuid,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool, JobError>;

    /// Extend a held lock; false when the caller no longer owns it.
    async fn refresh(
        &self,
        overlap_key: &str,
        lease_id: Uuid,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool, JobError>;

    /// Release a held lock. Releasing an unheld lock is a no-op; releasing
    /// another owner's lock is impossible by construction.
    async fn release(&self, overlap_key: &str, lease_id: Uuid) -> Result<(), JobError>;

    /// Delete expired locks. Returns the number removed.
    async fn recover_expired(&self, now: DateTime<Utc>) -> Result<usize, JobError>;
}

/// Validate explicit dispatch/claim parameters shared by every adapter.
pub fn validate_worker_claim(
    worker_execution_id: &str,
    limit: usize,
    expires_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<(), JobError> {
    if worker_execution_id.trim().is_empty()
        || worker_execution_id.len() > 128
        || worker_execution_id.contains(|c: char| c.is_control())
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
