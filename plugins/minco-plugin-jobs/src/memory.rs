//! Deterministic in-memory stores and a recording fake dispatcher.
//!
//! These are the reference semantics for the SQL adapters and the test
//! doubles for application use cases; they are not a production backend.

use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::envelope::JobEnvelope;
use crate::ports::{
    EnqueueOutcome, JobAttempt, JobAttemptOutcome, JobDispatcher, JobError, JobPublication,
    JobPublicationStore, JobRecord, JobStatus, JobStore, OverlapLockStore, PublicationStatus,
    validate_worker_claim,
};

/// Maximum attempt-history entries retained on a record.
pub const MAX_ATTEMPT_HISTORY: usize = 25;

/// In-memory job store implementing every job port with reference semantics.
#[derive(Debug, Default)]
pub struct MemoryJobStore {
    jobs: RwLock<BTreeMap<Uuid, JobRecord>>,
    publications: RwLock<BTreeMap<Uuid, JobPublication>>,
    locks: RwLock<BTreeMap<String, (String, DateTime<Utc>)>>,
}

impl MemoryJobStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// All records in job-id order (test inspection).
    pub fn records(&self) -> Vec<JobRecord> {
        let jobs = self.jobs.read().expect("job store lock");
        let records = jobs.values().cloned().collect();
        drop(jobs);
        records
    }

    pub fn publication_records(&self) -> Vec<JobPublication> {
        let publications = self.publications.read().expect("publication lock");
        let records = publications.values().cloned().collect();
        drop(publications);
        records
    }
}

fn record_attempt(
    record: &mut JobRecord,
    worker_id: &str,
    now: DateTime<Utc>,
    outcome: JobAttemptOutcome,
) {
    let entry = JobAttempt {
        attempt: record.attempt_count,
        at: now,
        worker_id: worker_id.to_owned(),
        outcome,
    };
    record.attempts.push(entry);
    if record.attempts.len() > MAX_ATTEMPT_HISTORY {
        let excess = record.attempts.len() - MAX_ATTEMPT_HISTORY;
        record.attempts.drain(0..excess);
    }
}

#[async_trait::async_trait]
impl JobStore for MemoryJobStore {
    async fn enqueue_with_intent(&self, record: JobRecord) -> Result<EnqueueOutcome, JobError> {
        if record.status != JobStatus::Pending {
            return Err(JobError::InvalidJob("jobs must be enqueued pending".into()));
        }
        record.envelope.validate()?;
        let job_id = record.envelope.job_id;
        let dedupe_key = record.envelope.dedupe_key.clone();
        let identical_payload = record.envelope.payload.clone();
        let identical_name = record.envelope.job_name.clone();
        let identical_version = record.envelope.job_version;
        {
            let jobs = self.jobs.write().expect("job store lock");
            if let Some(dedupe_key) = dedupe_key.as_deref() {
                let existing = jobs
                    .values()
                    .find(|candidate| candidate.envelope.dedupe_key.as_deref() == Some(dedupe_key))
                    .cloned();
                if let Some(existing) = existing {
                    let identical = existing.envelope.payload == identical_payload
                        && existing.envelope.job_name == identical_name
                        && existing.envelope.job_version == identical_version;
                    let existing_job_id = existing.envelope.job_id;
                    drop(jobs);
                    return if identical {
                        Ok(EnqueueOutcome::Duplicate(existing_job_id))
                    } else {
                        Err(JobError::DuplicateSubmissionConflict { existing_job_id })
                    };
                }
            }
            if jobs.contains_key(&job_id) {
                drop(jobs);
                return Err(JobError::MissingJob(job_id));
            }
        }
        let publication = JobPublication {
            job_id,
            worker_profile: record.envelope.worker_profile.clone(),
            status: PublicationStatus::Pending,
            attempt_count: 0,
            available_at: record.envelope.available_at,
            claimed_by: None,
            claim_expires_at: None,
            last_error: None,
        };
        let mut jobs = self.jobs.write().expect("job store lock");
        let mut publications = self.publications.write().expect("publication lock");
        jobs.insert(job_id, record);
        publications.insert(job_id, publication);
        drop(jobs);
        drop(publications);
        Ok(EnqueueOutcome::Inserted(job_id))
    }

    async fn claim_execution(
        &self,
        job_id: Uuid,
        worker_id: &str,
        lease_expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Option<JobRecord>, JobError> {
        validate_worker_claim(worker_id, 1, lease_expires_at, now)?;
        let mut jobs = self.jobs.write().expect("job store lock");
        let Some(record) = jobs.get_mut(&job_id) else {
            drop(jobs);
            return Ok(None);
        };
        let claimable = record.status == JobStatus::Pending && record.envelope.available_at <= now
            || record.status == JobStatus::Running
                && record.lease_expires_at.is_some_and(|expiry| expiry <= now);
        if !claimable {
            drop(jobs);
            return Ok(None);
        }
        record.status = JobStatus::Running;
        record.lease_owner = Some(worker_id.to_owned());
        record.lease_expires_at = Some(lease_expires_at);
        record.attempt_count = record.attempt_count.saturating_add(1);
        record.revision = record.revision.saturating_add(1);
        let claimed = record.clone();
        drop(jobs);
        Ok(Some(claimed))
    }

    async fn complete(
        &self,
        job_id: Uuid,
        worker_id: &str,
        now: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let mut jobs = self.jobs.write().expect("job store lock");
        let record = jobs.get_mut(&job_id).ok_or(JobError::MissingJob(job_id))?;
        if record.lease_owner.as_deref() != Some(worker_id) || record.status != JobStatus::Running {
            return Err(JobError::LeaseOwnership {
                job_id,
                worker_id: worker_id.to_owned(),
            });
        }
        record.status = JobStatus::Succeeded;
        record.lease_owner = None;
        record.lease_expires_at = None;
        record.failure_code = None;
        record.completed_at = Some(now);
        record.revision = record.revision.saturating_add(1);
        record_attempt(record, worker_id, now, JobAttemptOutcome::Succeeded);
        drop(jobs);
        Ok(())
    }

    async fn schedule_retry(
        &self,
        job_id: Uuid,
        worker_id: &str,
        failure_code: &str,
        next_available_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let mut jobs = self.jobs.write().expect("job store lock");
        let record = jobs.get_mut(&job_id).ok_or(JobError::MissingJob(job_id))?;
        if record.lease_owner.as_deref() != Some(worker_id) || record.status != JobStatus::Running {
            return Err(JobError::LeaseOwnership {
                job_id,
                worker_id: worker_id.to_owned(),
            });
        }
        record.status = JobStatus::Pending;
        record.lease_owner = None;
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
            worker_id,
            now,
            JobAttemptOutcome::Retried {
                code: failure_code.to_owned(),
            },
        );
        drop(jobs);
        Ok(())
    }

    async fn fail_permanently(
        &self,
        job_id: Uuid,
        worker_id: &str,
        failure_code: &str,
        now: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let mut jobs = self.jobs.write().expect("job store lock");
        let record = jobs.get_mut(&job_id).ok_or(JobError::MissingJob(job_id))?;
        if record.lease_owner.as_deref() != Some(worker_id) || record.status != JobStatus::Running {
            return Err(JobError::LeaseOwnership {
                job_id,
                worker_id: worker_id.to_owned(),
            });
        }
        record.status = JobStatus::FailedPermanently;
        record.lease_owner = None;
        record.lease_expires_at = None;
        record.failure_code = Some(failure_code.to_owned());
        record.completed_at = Some(now);
        record.revision = record.revision.saturating_add(1);
        record_attempt(
            record,
            worker_id,
            now,
            JobAttemptOutcome::FailedPermanently {
                code: failure_code.to_owned(),
            },
        );
        drop(jobs);
        Ok(())
    }

    async fn cancel(
        &self,
        job_id: Uuid,
        expected_revision: u64,
        now: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let mut jobs = self.jobs.write().expect("job store lock");
        let record = jobs.get_mut(&job_id).ok_or(JobError::MissingJob(job_id))?;
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
        drop(jobs);
        Ok(())
    }

    async fn retry_failed(
        &self,
        job_id: Uuid,
        expected_revision: u64,
        now: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, JobError> {
        let mut jobs = self.jobs.write().expect("job store lock");
        let record = jobs.get_mut(&job_id).ok_or(JobError::MissingJob(job_id))?;
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
        drop(jobs);
        Ok(now)
    }

    async fn recover_expired_leases(&self, now: DateTime<Utc>) -> Result<usize, JobError> {
        let mut jobs = self.jobs.write().expect("job store lock");
        let mut recovered = 0;
        for record in jobs.values_mut() {
            if record.status == JobStatus::Running
                && record.lease_expires_at.is_some_and(|expiry| expiry <= now)
            {
                record.status = JobStatus::Pending;
                record.lease_owner = None;
                record.lease_expires_at = None;
                record.envelope.available_at = now;
                record.revision = record.revision.saturating_add(1);
                recovered += 1;
            }
        }
        drop(jobs);
        Ok(recovered)
    }

    async fn get(&self, job_id: Uuid) -> Result<Option<JobRecord>, JobError> {
        let jobs = self.jobs.read().expect("job store lock");
        let record = jobs.get(&job_id).cloned();
        drop(jobs);
        Ok(record)
    }

    async fn list_failed(&self, limit: usize) -> Result<Vec<JobRecord>, JobError> {
        let jobs = self.jobs.read().expect("job store lock");
        Ok(jobs
            .values()
            .filter(|record| record.status == JobStatus::FailedPermanently)
            .take(limit)
            .cloned()
            .collect())
    }
}

#[async_trait::async_trait]
impl JobPublicationStore for MemoryJobStore {
    async fn enqueue_intent(
        &self,
        job_id: Uuid,
        worker_profile: &str,
        available_at: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let mut publications = self.publications.write().expect("publication lock");
        if publications.contains_key(&job_id) {
            drop(publications);
            return Err(JobError::InvalidTransition { job_id });
        }
        publications.insert(
            job_id,
            JobPublication {
                job_id,
                worker_profile: worker_profile.to_owned(),
                status: PublicationStatus::Pending,
                attempt_count: 0,
                available_at,
                claimed_by: None,
                claim_expires_at: None,
                last_error: None,
            },
        );
        drop(publications);
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
        let mut publications = self.publications.write().expect("publication lock");
        let mut claimed = Vec::new();
        for publication in publications.values_mut() {
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
                publication.status = PublicationStatus::Claimed;
                publication.claimed_by = Some(worker_id.to_owned());
                publication.claim_expires_at = Some(claim_expires_at);
                publication.attempt_count = publication.attempt_count.saturating_add(1);
                claimed.push(publication.clone());
            }
        }
        drop(publications);
        Ok(claimed)
    }

    async fn mark_published(&self, job_id: Uuid, worker_id: &str) -> Result<(), JobError> {
        let mut publications = self.publications.write().expect("publication lock");
        let publication = publications
            .get_mut(&job_id)
            .ok_or(JobError::MissingJob(job_id))?;
        if publication.claimed_by.as_deref() != Some(worker_id)
            || publication.status != PublicationStatus::Claimed
        {
            return Err(JobError::LeaseOwnership {
                job_id,
                worker_id: worker_id.to_owned(),
            });
        }
        publication.status = PublicationStatus::Published;
        publication.claimed_by = None;
        publication.claim_expires_at = None;
        publication.last_error = None;
        drop(publications);
        Ok(())
    }

    async fn mark_failed(
        &self,
        job_id: Uuid,
        worker_id: &str,
        error: &str,
        retry_at: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let mut publications = self.publications.write().expect("publication lock");
        let publication = publications
            .get_mut(&job_id)
            .ok_or(JobError::MissingJob(job_id))?;
        if publication.claimed_by.as_deref() != Some(worker_id) {
            return Err(JobError::LeaseOwnership {
                job_id,
                worker_id: worker_id.to_owned(),
            });
        }
        publication.status = PublicationStatus::Failed;
        publication.claimed_by = None;
        publication.claim_expires_at = None;
        publication.available_at = retry_at;
        publication.last_error = Some(error.to_owned());
        drop(publications);
        Ok(())
    }

    async fn republish(
        &self,
        job_id: Uuid,
        worker_id: &str,
        available_at: DateTime<Utc>,
    ) -> Result<(), JobError> {
        let owned = {
            let jobs = self.jobs.read().expect("job store lock");
            let owned = jobs
                .get(&job_id)
                .is_some_and(|record| record.envelope.available_at == available_at);
            drop(jobs);
            owned
        };
        if !owned {
            // The retry transition must have happened first.
            return Err(JobError::InvalidTransition { job_id });
        }
        let mut publications = self.publications.write().expect("publication lock");
        let publication = publications
            .get_mut(&job_id)
            .ok_or(JobError::MissingJob(job_id))?;
        if publication.status == PublicationStatus::Claimed
            && publication.claimed_by.as_deref() != Some(worker_id)
        {
            return Err(JobError::LeaseOwnership {
                job_id,
                worker_id: worker_id.to_owned(),
            });
        }
        publication.status = PublicationStatus::Pending;
        publication.claimed_by = None;
        publication.claim_expires_at = None;
        publication.available_at = available_at;
        drop(publications);
        Ok(())
    }

    async fn recover_expired_claims(&self, now: DateTime<Utc>) -> Result<usize, JobError> {
        let mut publications = self.publications.write().expect("publication lock");
        let mut recovered = 0;
        for publication in publications.values_mut() {
            if publication.status == PublicationStatus::Claimed
                && publication
                    .claim_expires_at
                    .is_some_and(|expiry| expiry <= now)
            {
                publication.status = PublicationStatus::Pending;
                publication.claimed_by = None;
                publication.claim_expires_at = None;
                recovered += 1;
            }
        }
        drop(publications);
        Ok(recovered)
    }
}

#[async_trait::async_trait]
impl OverlapLockStore for MemoryJobStore {
    async fn acquire(
        &self,
        overlap_key: &str,
        owner: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool, JobError> {
        let mut locks = self.locks.write().expect("lock table");
        let free = match locks.get(overlap_key) {
            Some((_, expiry)) => *expiry <= now,
            None => true,
        };
        if !free {
            drop(locks);
            return Ok(false);
        }
        locks.insert(overlap_key.to_owned(), (owner.to_owned(), expires_at));
        drop(locks);
        Ok(true)
    }

    async fn refresh(
        &self,
        overlap_key: &str,
        owner: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool, JobError> {
        let mut locks = self.locks.write().expect("lock table");
        let refreshed = match locks.get(overlap_key) {
            Some((held_owner, expiry)) if held_owner == owner && *expiry > now => {
                locks.insert(overlap_key.to_owned(), (owner.to_owned(), expires_at));
                true
            }
            _ => false,
        };
        drop(locks);
        Ok(refreshed)
    }

    async fn release(&self, overlap_key: &str, owner: &str) -> Result<(), JobError> {
        let mut locks = self.locks.write().expect("lock table");
        if locks
            .get(overlap_key)
            .is_some_and(|(held_owner, _)| held_owner == owner)
        {
            locks.remove(overlap_key);
        }
        drop(locks);
        Ok(())
    }

    async fn recover_expired(&self, now: DateTime<Utc>) -> Result<usize, JobError> {
        let mut locks = self.locks.write().expect("lock table");
        let expired: Vec<String> = locks
            .iter()
            .filter(|(_, (_, expiry))| *expiry <= now)
            .map(|(key, _)| key.clone())
            .collect();
        let count = expired.len();
        for key in expired {
            locks.remove(&key);
        }
        drop(locks);
        Ok(count)
    }
}

/// One recorded dispatch attempt. Never carries the payload into `Debug`.
#[derive(Clone)]
pub struct DispatchAttempt {
    pub envelope: JobEnvelope,
}

impl std::fmt::Debug for DispatchAttempt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DispatchAttempt")
            .field("job_id", &self.envelope.job_id)
            .field("job_name", &self.envelope.job_name)
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

    /// Dispatched envelopes in order.
    pub fn dispatched(&self) -> Vec<JobEnvelope> {
        self.attempts
            .read()
            .expect("fake dispatcher attempts")
            .iter()
            .map(|attempt| attempt.envelope.clone())
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
    async fn dispatch(&self, envelope: &JobEnvelope, _now: DateTime<Utc>) -> Result<(), JobError> {
        envelope.validate()?;
        self.attempts
            .write()
            .expect("fake dispatcher attempts")
            .push(DispatchAttempt {
                envelope: envelope.clone(),
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
    async fn dispatch(&self, envelope: &JobEnvelope, _now: DateTime<Utc>) -> Result<(), JobError> {
        Err(JobError::Infrastructure(format!(
            "dispatch is not configured; job {} was never published",
            envelope.job_id
        )))
    }
}
