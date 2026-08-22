//! Deterministic in-memory stores and a recording fake dispatcher.
//!
//! These are the reference semantics for the SQL adapters and the test
//! doubles for application use cases. A single internal state lock models
//! the adapters' transaction boundary: dedupe check, job insertion,
//! publication insertion, lock insertion and every state transition are
//! atomic with respect to each other.

use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::envelope::semantic_fingerprint;
use crate::ports::{
    EnqueueOutcome, IngestOutcome, JobAttempt, JobAttemptOutcome, JobClaim, JobDelivery,
    JobDispatcher, JobError, JobPublication, JobPublicationStore, JobRecord, JobStatus, JobStore,
    OverlapLockStore, PublicationStatus, validate_worker_claim,
};

/// Maximum attempt-history entries retained on a record.
pub const MAX_ATTEMPT_HISTORY: usize = 25;

#[derive(Default)]
struct MemoryState {
    jobs: BTreeMap<Uuid, JobRecord>,
    fingerprints: BTreeMap<String, Uuid>,
    publications: BTreeMap<Uuid, JobPublication>,
    locks: BTreeMap<String, (Uuid, DateTime<Utc>)>,
}

/// In-memory job store implementing every job port with reference
/// semantics. All operations run under one lock, mirroring the SQL
/// adapters' single-transaction atomicity.
#[derive(Default)]
pub struct MemoryJobStore {
    state: Mutex<MemoryState>,
}

impl std::fmt::Debug for MemoryJobStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryJobStore").finish_non_exhaustive()
    }
}

impl MemoryJobStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// All records in job-id order (test inspection).
    pub fn records(&self) -> Vec<JobRecord> {
        let state = self.state.lock().expect("job store lock");
        state.jobs.values().cloned().collect()
    }

    /// All publication generations (test inspection).
    pub fn publication_records(&self) -> Vec<JobPublication> {
        let state = self.state.lock().expect("job store lock");
        state.publications.values().cloned().collect()
    }
}

fn insert_publication(
    state: &mut MemoryState,
    job_id: Uuid,
    worker_profile: &str,
    available_at: DateTime<Utc>,
    status: PublicationStatus,
) -> Uuid {
    let generation = state
        .publications
        .values()
        .filter(|publication| publication.job_id == job_id)
        .map(|publication| publication.generation)
        .max()
        .unwrap_or(0)
        + 1;
    let publication_id = Uuid::now_v7();
    state.publications.insert(
        publication_id,
        JobPublication {
            publication_id,
            job_id,
            generation,
            worker_profile: worker_profile.to_owned(),
            status,
            attempt_count: 0,
            available_at,
            claimed_by: None,
            claim_expires_at: None,
            lease_id: None,
            last_error: None,
        },
    );
    publication_id
}

fn record_attempt(
    record: &mut JobRecord,
    worker_execution_id: &str,
    now: DateTime<Utc>,
    outcome: JobAttemptOutcome,
) {
    let entry = JobAttempt {
        attempt: record.attempt_count,
        at: now,
        worker_execution_id: worker_execution_id.to_owned(),
        outcome,
    };
    record.attempts.push(entry);
    if record.attempts.len() > MAX_ATTEMPT_HISTORY {
        let excess = record.attempts.len() - MAX_ATTEMPT_HISTORY;
        record.attempts.drain(0..excess);
    }
}

/// The dedupe rule: one dedupe key maps to one semantic job. The same key
/// with a different semantic fingerprint fails closed.
fn dedupe_outcome(
    state: &MemoryState,
    record: &JobRecord,
) -> Option<Result<EnqueueOutcome, JobError>> {
    let key = record.envelope.dedupe_key.as_deref()?;
    let existing_id = state.fingerprints.get(key).copied()?;
    let existing = &state.jobs[&existing_id];
    if existing.envelope.dedupe_key.as_deref() == Some(key)
        && semantic_fingerprint(&existing.envelope) == semantic_fingerprint(&record.envelope)
    {
        Some(Ok(EnqueueOutcome::Duplicate(existing_id)))
    } else {
        Some(Err(JobError::DuplicateSubmissionConflict {
            existing_job_id: existing_id,
        }))
    }
}

fn validate_for_insert(record: &JobRecord) -> Result<(), JobError> {
    if record.status != JobStatus::Pending {
        return Err(JobError::InvalidJob("jobs must be enqueued pending".into()));
    }
    record.envelope.validate()?;
    Ok(())
}

fn claim_execution_suffix(claim: &JobClaim) -> String {
    format!("lease-{}", claim.lease_id)
}

#[async_trait::async_trait]
impl JobStore for MemoryJobStore {
    async fn enqueue_with_intent(&self, record: JobRecord) -> Result<EnqueueOutcome, JobError> {
        validate_for_insert(&record)?;
        let mut state = self.state.lock().expect("job store lock");
        if let Some(outcome) = dedupe_outcome(&state, &record) {
            drop(state);
            return outcome;
        }
        let job_id = record.envelope.job_id;
        if state.jobs.contains_key(&job_id) {
            drop(state);
            // The job exists under this identity but did not match the
            // dedupe rule above: an identity collision with different
            // semantics, not a missing job.
            return Err(JobError::DuplicateJobIdentity(job_id));
        }
        if let Some(key) = record.envelope.dedupe_key.clone() {
            state.fingerprints.insert(key, job_id);
        }
        let profile = record.envelope.worker_profile.clone();
        let available_at = record.envelope.available_at;
        state.jobs.insert(job_id, record);
        insert_publication(
            &mut state,
            job_id,
            &profile,
            available_at,
            PublicationStatus::Pending,
        );
        drop(state);
        Ok(EnqueueOutcome::Inserted(job_id))
    }

    async fn claim_execution(
        &self,
        job_id: Uuid,
        worker_execution_id: &str,
        lease_expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Option<JobClaim>, JobError> {
        validate_worker_claim(worker_execution_id, 1, lease_expires_at, now)?;
        let mut state = self.state.lock().expect("job store lock");
        let Some(record) = state.jobs.get_mut(&job_id) else {
            return Ok(None);
        };
        let claimable = record.status == JobStatus::Pending && record.envelope.available_at <= now
            || record.status == JobStatus::Running
                && record.lease_expires_at.is_some_and(|expiry| expiry <= now);
        if !claimable {
            return Ok(None);
        }
        let lease_id = Uuid::now_v7();
        record.status = JobStatus::Running;
        record.lease_id = Some(lease_id);
        record.lease_expires_at = Some(lease_expires_at);
        record.attempt_count = record.attempt_count.saturating_add(1);
        record.revision = record.revision.saturating_add(1);
        let fence = record.revision;
        let claimed = record.clone();
        drop(state);
        Ok(Some(JobClaim {
            record: claimed,
            lease_id,
            fence,
        }))
    }

    async fn complete(&self, claim: &JobClaim, now: DateTime<Utc>) -> Result<(), JobError> {
        let job_id = claim.record.envelope.job_id;
        let mut state = self.state.lock().expect("job store lock");
        let record = state
            .jobs
            .get_mut(&job_id)
            .ok_or(JobError::MissingJob(job_id))?;
        if record.lease_id != Some(claim.lease_id) || record.status != JobStatus::Running {
            return Err(JobError::LeaseFencedOut { job_id });
        }
        let worker_execution_id = claim_execution_suffix(claim);
        record.status = JobStatus::Succeeded;
        record.lease_id = None;
        record.lease_expires_at = None;
        record.failure_code = None;
        record.completed_at = Some(now);
        record.revision = record.revision.saturating_add(1);
        record_attempt(
            record,
            &worker_execution_id,
            now,
            JobAttemptOutcome::Succeeded,
        );
        drop(state);
        Ok(())
    }

    async fn schedule_retry_and_publish(
        &self,
        claim: &JobClaim,
        failure_code: &str,
        next_available_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Uuid, JobError> {
        let job_id = claim.record.envelope.job_id;
        let mut state = self.state.lock().expect("job store lock");
        let record = state
            .jobs
            .get_mut(&job_id)
            .ok_or(JobError::MissingJob(job_id))?;
        if record.lease_id != Some(claim.lease_id) || record.status != JobStatus::Running {
            return Err(JobError::LeaseFencedOut { job_id });
        }
        let worker_execution_id = claim_execution_suffix(claim);
        record.status = JobStatus::Pending;
        record.lease_id = None;
        record.lease_expires_at = None;
        record.envelope.available_at = next_available_at;
        record.envelope.attempt = record
            .envelope
            .attempt
            .saturating_add(1)
            .min(record.envelope.maximum_attempts);
        record.failure_code = Some(failure_code.to_owned());
        record.revision = record.revision.saturating_add(1);
        record_attempt(
            record,
            &worker_execution_id,
            now,
            JobAttemptOutcome::Retried {
                code: failure_code.to_owned(),
            },
        );
        let profile = record.envelope.worker_profile.clone();
        let publication_id = insert_publication(
            &mut state,
            job_id,
            &profile,
            next_available_at,
            PublicationStatus::Pending,
        );
        drop(state);
        Ok(publication_id)
    }

    async fn fail_permanently(
        &self,
        claim: &JobClaim,
        failure_code: &str,
        now: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let job_id = claim.record.envelope.job_id;
        let mut state = self.state.lock().expect("job store lock");
        let record = state
            .jobs
            .get_mut(&job_id)
            .ok_or(JobError::MissingJob(job_id))?;
        if record.lease_id != Some(claim.lease_id) || record.status != JobStatus::Running {
            return Err(JobError::LeaseFencedOut { job_id });
        }
        let worker_execution_id = claim_execution_suffix(claim);
        record.status = JobStatus::FailedPermanently;
        record.lease_id = None;
        record.lease_expires_at = None;
        record.failure_code = Some(failure_code.to_owned());
        record.completed_at = Some(now);
        record.revision = record.revision.saturating_add(1);
        record_attempt(
            record,
            &worker_execution_id,
            now,
            JobAttemptOutcome::FailedPermanently {
                code: failure_code.to_owned(),
            },
        );
        drop(state);
        Ok(())
    }

    async fn cancel(
        &self,
        job_id: Uuid,
        expected_revision: u64,
        now: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let mut state = self.state.lock().expect("job store lock");
        let record = state
            .jobs
            .get_mut(&job_id)
            .ok_or(JobError::MissingJob(job_id))?;
        if record.revision != expected_revision {
            return Err(JobError::RevisionConflict {
                job_id,
                expected_revision,
            });
        }
        if record.status != JobStatus::Pending {
            return Err(JobError::InvalidTransition { job_id });
        }
        record.status = JobStatus::Cancelled;
        record.completed_at = Some(now);
        record.revision = record.revision.saturating_add(1);
        drop(state);
        Ok(())
    }

    async fn retry_failed(
        &self,
        job_id: Uuid,
        expected_revision: u64,
        now: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, JobError> {
        let mut state = self.state.lock().expect("job store lock");
        let record = state
            .jobs
            .get_mut(&job_id)
            .ok_or(JobError::MissingJob(job_id))?;
        if record.revision != expected_revision {
            return Err(JobError::RevisionConflict {
                job_id,
                expected_revision,
            });
        }
        if record.status != JobStatus::FailedPermanently {
            return Err(JobError::InvalidTransition { job_id });
        }
        record.status = JobStatus::Pending;
        record.failure_code = None;
        record.completed_at = None;
        record.envelope.available_at = now;
        record.attempt_count = 0;
        record.envelope.attempt = 1;
        record.revision = record.revision.saturating_add(1);
        let profile = record.envelope.worker_profile.clone();
        insert_publication(
            &mut state,
            job_id,
            &profile,
            now,
            PublicationStatus::Pending,
        );
        drop(state);
        Ok(now)
    }

    async fn recover_expired_leases(&self, now: DateTime<Utc>) -> Result<usize, JobError> {
        let mut state = self.state.lock().expect("job store lock");
        let mut recovered = 0;
        let mut reopened: Vec<(Uuid, String)> = Vec::new();
        for (job_id, record) in &mut state.jobs {
            if record.status == JobStatus::Running
                && record.lease_expires_at.is_some_and(|expiry| expiry <= now)
            {
                record.status = JobStatus::Pending;
                record.lease_id = None;
                record.lease_expires_at = None;
                record.envelope.available_at = now;
                record.revision = record.revision.saturating_add(1);
                reopened.push((*job_id, record.envelope.worker_profile.clone()));
                recovered += 1;
            }
        }
        for (job_id, profile) in reopened {
            insert_publication(
                &mut state,
                job_id,
                &profile,
                now,
                PublicationStatus::Pending,
            );
        }
        drop(state);
        Ok(recovered)
    }

    async fn get(&self, job_id: Uuid) -> Result<Option<JobRecord>, JobError> {
        let state = self.state.lock().expect("job store lock");
        Ok(state.jobs.get(&job_id).cloned())
    }

    async fn list_failed(&self, limit: usize) -> Result<Vec<JobRecord>, JobError> {
        let state = self.state.lock().expect("job store lock");
        let failed = state
            .jobs
            .values()
            .filter(|record| record.status == JobStatus::FailedPermanently)
            .take(limit)
            .cloned()
            .collect();
        drop(state);
        Ok(failed)
    }

    async fn ingest_existing_delivery(&self, record: JobRecord) -> Result<IngestOutcome, JobError> {
        validate_for_insert(&record)?;
        let mut state = self.state.lock().expect("job store lock");
        if let Some(outcome) = dedupe_outcome(&state, &record) {
            return match outcome {
                Ok(EnqueueOutcome::Duplicate(existing)) => Ok(IngestOutcome::Duplicate(existing)),
                Ok(EnqueueOutcome::Inserted(_)) => unreachable!("dedupe map hit implies insert"),
                Err(error) => Err(error),
            };
        }
        let job_id = record.envelope.job_id;
        let profile = record.envelope.worker_profile.clone();
        let available_at = record.envelope.available_at;
        if let Some(key) = record.envelope.dedupe_key.clone() {
            state.fingerprints.insert(key, job_id);
        }
        state.jobs.insert(job_id, record);
        // The delivery already exists on the transport: record the
        // publication as delivered so no pending generation is created.
        insert_publication(
            &mut state,
            job_id,
            &profile,
            available_at,
            PublicationStatus::Published,
        );
        drop(state);
        Ok(IngestOutcome::Ingested(job_id))
    }
}

#[async_trait::async_trait]
impl JobPublicationStore for MemoryJobStore {
    async fn claim_due(
        &self,
        worker_execution_id: &str,
        limit: usize,
        claim_expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Vec<JobPublication>, JobError> {
        validate_worker_claim(worker_execution_id, limit, claim_expires_at, now)?;
        let mut state = self.state.lock().expect("job store lock");
        let mut claimed = Vec::new();
        for publication in state.publications.values_mut() {
            if claimed.len() >= limit {
                break;
            }
            let due = publication.status == PublicationStatus::Pending
                || publication.status == PublicationStatus::Failed;
            let expired = publication.status == PublicationStatus::Claimed
                && publication
                    .claim_expires_at
                    .is_some_and(|expiry| expiry <= now);
            if (due && publication.available_at <= now) || expired {
                let lease_id = Uuid::now_v7();
                publication.status = PublicationStatus::Claimed;
                publication.claimed_by = Some(worker_execution_id.to_owned());
                publication.claim_expires_at = Some(claim_expires_at);
                publication.lease_id = Some(lease_id);
                publication.attempt_count = publication.attempt_count.saturating_add(1);
                claimed.push(publication.clone());
            }
        }
        drop(state);
        Ok(claimed)
    }

    async fn mark_published(&self, publication_id: Uuid, lease_id: Uuid) -> Result<(), JobError> {
        let mut state = self.state.lock().expect("job store lock");
        let publication = state
            .publications
            .get_mut(&publication_id)
            .ok_or(JobError::MissingJob(publication_id))?;
        if publication.lease_id != Some(lease_id)
            || publication.status != PublicationStatus::Claimed
        {
            return Err(JobError::PublicationFencedOut { publication_id });
        }
        publication.status = PublicationStatus::Published;
        publication.claimed_by = None;
        publication.claim_expires_at = None;
        publication.lease_id = None;
        publication.last_error = None;
        drop(state);
        Ok(())
    }

    async fn mark_failed(
        &self,
        publication_id: Uuid,
        lease_id: Uuid,
        error: &str,
        retry_at: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let mut state = self.state.lock().expect("job store lock");
        let publication = state
            .publications
            .get_mut(&publication_id)
            .ok_or(JobError::MissingJob(publication_id))?;
        if publication.lease_id != Some(lease_id) {
            return Err(JobError::PublicationFencedOut { publication_id });
        }
        publication.status = PublicationStatus::Failed;
        publication.claimed_by = None;
        publication.claim_expires_at = None;
        publication.lease_id = None;
        publication.available_at = retry_at;
        publication.last_error = Some(error.to_owned());
        drop(state);
        Ok(())
    }

    async fn recover_expired_claims(&self, now: DateTime<Utc>) -> Result<usize, JobError> {
        let mut state = self.state.lock().expect("job store lock");
        let mut recovered = 0;
        for publication in state.publications.values_mut() {
            if publication.status == PublicationStatus::Claimed
                && publication
                    .claim_expires_at
                    .is_some_and(|expiry| expiry <= now)
            {
                publication.status = PublicationStatus::Pending;
                publication.claimed_by = None;
                publication.claim_expires_at = None;
                publication.lease_id = None;
                recovered += 1;
            }
        }
        drop(state);
        Ok(recovered)
    }
}

#[async_trait::async_trait]
impl OverlapLockStore for MemoryJobStore {
    async fn acquire(
        &self,
        overlap_key: &str,
        lease_id: Uuid,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool, JobError> {
        let mut state = self.state.lock().expect("job store lock");
        let free = match state.locks.get(overlap_key) {
            Some((_, expiry)) => *expiry <= now,
            None => true,
        };
        if !free {
            drop(state);
            return Ok(false);
        }
        state
            .locks
            .insert(overlap_key.to_owned(), (lease_id, expires_at));
        drop(state);
        Ok(true)
    }

    async fn refresh(
        &self,
        overlap_key: &str,
        lease_id: Uuid,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool, JobError> {
        let mut state = self.state.lock().expect("job store lock");
        let refreshed = match state.locks.get(overlap_key) {
            Some((held_lease, expiry)) if *held_lease == lease_id && *expiry > now => {
                state
                    .locks
                    .insert(overlap_key.to_owned(), (lease_id, expires_at));
                true
            }
            _ => false,
        };
        drop(state);
        Ok(refreshed)
    }

    async fn release(&self, overlap_key: &str, lease_id: Uuid) -> Result<(), JobError> {
        let mut state = self.state.lock().expect("job store lock");
        if state
            .locks
            .get(overlap_key)
            .is_some_and(|(held_lease, _)| *held_lease == lease_id)
        {
            state.locks.remove(overlap_key);
        }
        drop(state);
        Ok(())
    }

    async fn recover_expired(&self, now: DateTime<Utc>) -> Result<usize, JobError> {
        let mut state = self.state.lock().expect("job store lock");
        let expired: Vec<String> = state
            .locks
            .iter()
            .filter(|(_, (_, expiry))| *expiry <= now)
            .map(|(key, _)| key.clone())
            .collect();
        let count = expired.len();
        for key in expired {
            state.locks.remove(&key);
        }
        drop(state);
        Ok(count)
    }
}

/// One recorded dispatch attempt. Never carries the payload into `Debug`.
#[derive(Clone)]
pub struct DispatchAttempt {
    pub delivery: JobDelivery,
}

impl std::fmt::Debug for DispatchAttempt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DispatchAttempt")
            .field("job_id", &self.delivery.envelope.job_id)
            .field("job_name", &self.delivery.envelope.job_name)
            .field("publication_id", &self.delivery.publication_id)
            .finish_non_exhaustive()
    }
}

/// Recording fake dispatcher with deterministic one-shot failures.
#[derive(Default)]
pub struct FakeJobDispatcher {
    attempts: RwLock<Vec<DispatchAttempt>>,
    failures: Mutex<VecDeque<String>>,
}

impl FakeJobDispatcher {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Fail exactly the next dispatch with this redacted message.
    pub fn fail_next(&self, message: impl Into<String>) {
        self.failures
            .lock()
            .expect("fake dispatcher failures")
            .push_back(message.into());
    }

    /// Dispatched deliveries in order.
    pub fn dispatched(&self) -> Vec<JobDelivery> {
        self.attempts
            .read()
            .expect("fake dispatcher attempts")
            .iter()
            .map(|attempt| attempt.delivery.clone())
            .collect()
    }

    pub fn clear(&self) {
        self.attempts
            .write()
            .expect("fake dispatcher attempts")
            .clear();
    }
}

impl std::fmt::Debug for FakeJobDispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeJobDispatcher").finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl JobDispatcher for FakeJobDispatcher {
    async fn dispatch(&self, delivery: &JobDelivery, _now: DateTime<Utc>) -> Result<(), JobError> {
        delivery.envelope.validate()?;
        self.attempts
            .write()
            .expect("fake dispatcher attempts")
            .push(DispatchAttempt {
                delivery: delivery.clone(),
            });
        let failure = {
            let mut failures = self.failures.lock().expect("fake dispatcher failures");
            failures.pop_front()
        };
        if let Some(message) = failure {
            return Err(JobError::Infrastructure(message));
        }
        Ok(())
    }
}

/// A dispatcher that fails every dispatch closed.
///
/// Compositions that only execute deliveries (never publish) bind this so
/// an accidental dispatch pass surfaces as an explicit error instead of
/// silently dropping work or falling back to inline execution.
#[derive(Debug, Default)]
pub struct FailClosedDispatcher;

#[async_trait::async_trait]
impl JobDispatcher for FailClosedDispatcher {
    async fn dispatch(&self, delivery: &JobDelivery, _now: DateTime<Utc>) -> Result<(), JobError> {
        Err(JobError::Infrastructure(format!(
            "dispatch is not configured; job {} was never published",
            delivery.envelope.job_id
        )))
    }
}
