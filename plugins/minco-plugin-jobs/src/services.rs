//! Job services: submission modes, the due-publication dispatcher and the
//! lease-based executor that connects a delivery to one typed handler.
//!
//! The durable record is authoritative: after an atomic claim the executor
//! runs the claimed record's envelope, and a delivery whose semantic
//! fingerprint differs from the record's fails closed as an inspectable
//! transport-integrity failure. Every mutation is fenced by the claim's
//! opaque lease identity, and retries commit their state and their next
//! publication generation in one store transaction. Nothing here schedules
//! itself: dispatch is explicit and bounded, and permanent failure is
//! persisted before acknowledgement.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{DateTime, TimeDelta, Utc};
use uuid::Uuid;

use crate::envelope::JobEnvelope;
use crate::envelope::semantic_fingerprint;
use crate::memory::{FakeJobDispatcher, MemoryJobStore};
use crate::policy::JobClock;
use crate::ports::{
    EnqueueOutcome, JobDelivery, JobDispatcher, JobError, JobPublicationStore, JobStatus, JobStore,
    OverlapLockStore,
};
use crate::registry::{JobContext, JobExecutionFailure, JobHandlerRegistry, failure_codes};

/// Publication retry backoff after a failed transport send.
pub const PUBLICATION_RETRY_DELAY: TimeDelta = TimeDelta::seconds(30);
/// Default execution lease granted to one worker delivery.
pub const DEFAULT_EXECUTION_LEASE: TimeDelta = TimeDelta::minutes(15);
/// Delay before re-running a job whose overlap lock is busy.
pub const OVERLAP_RETRY_DELAY: TimeDelta = TimeDelta::seconds(5);
/// Default per-execution handler timeout when none is configured.
pub const DEFAULT_EXECUTION_TIMEOUT: TimeDelta = TimeDelta::seconds(300);

/// Result of a durable submission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DurableSubmission {
    Inserted(Uuid),
    /// A semantically identical dedupe-keyed job already exists; nothing
    /// was written.
    Duplicate(Uuid),
}

/// Bounded report for one explicit dispatch run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DispatchReport {
    pub claimed: usize,
    pub dispatched: usize,
    pub skipped_terminal: usize,
    pub failed: usize,
}

/// How one delivery was disposed of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobRunDisposition {
    /// The handler ran to a durable transition.
    Executed(JobExecutionDisposition),
    /// The delivery required no execution.
    Skipped(JobSkipReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobExecutionDisposition {
    Succeeded,
    RetryScheduled {
        code: String,
        retry_at: DateTime<Utc>,
    },
    FailedPermanently {
        code: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobSkipReason {
    /// The durable record does not exist. Durable workers never fall back
    /// to inline execution; the delivery follows the queue's poison path.
    Missing,
    /// The job is terminal or leased by another live claim.
    NotExecutable(JobStatus),
    /// The job is pending but its availability time has not arrived.
    NotYetAvailable,
}

/// Executes one job delivery to a durable disposition.
///
/// Claim, transport-integrity check, deadline, resolve, overlap, timeout,
/// and the durable transition before acknowledgement. Every mutation
/// presents the claim's lease identity, so a stale invocation cannot alter
/// a newer claim's state.
#[derive(Debug, Clone)]
pub struct JobExecutor {
    pub registry: Arc<JobHandlerRegistry>,
    pub execution_timeout: TimeDelta,
    pub execution_lease: TimeDelta,
    pub overlap_retry_delay: TimeDelta,
}

impl JobExecutor {
    #[must_use]
    pub const fn new(registry: Arc<JobHandlerRegistry>) -> Self {
        Self {
            registry,
            execution_timeout: DEFAULT_EXECUTION_TIMEOUT,
            execution_lease: DEFAULT_EXECUTION_LEASE,
            overlap_retry_delay: OVERLAP_RETRY_DELAY,
        }
    }

    /// Run one delivery to a durable disposition.
    ///
    /// `worker_execution_id` must be unique per invocation (derive it from
    /// the transport message identity, not a static worker name). Store
    /// transitions happen before the disposition is returned, so callers
    /// can safely acknowledge the transport. The clock is read again after
    /// handler completion for completion, retry and failure timestamps.
    pub async fn run(
        &self,
        delivery_envelope: &JobEnvelope,
        worker_execution_id: &str,
        clock: &dyn JobClock,
        store: &dyn JobStore,
        _publications: &dyn JobPublicationStore,
        locks: &dyn OverlapLockStore,
    ) -> Result<JobRunDisposition, JobError> {
        let now = clock.now();
        let job_id = delivery_envelope.job_id;
        let Some(claim) = store
            .claim_execution(job_id, worker_execution_id, now + self.execution_lease, now)
            .await?
        else {
            let reason = match store.get(job_id).await? {
                Some(record) if record.status == JobStatus::Pending => {
                    JobSkipReason::NotYetAvailable
                }
                Some(record) => JobSkipReason::NotExecutable(record.status),
                None => JobSkipReason::Missing,
            };
            return Ok(JobRunDisposition::Skipped(reason));
        };

        // The durable record is authoritative. The transport copy only
        // locates the job: a semantic mismatch is a forged or stale
        // delivery and fails closed as an inspectable permanent failure
        // without executing the handler.
        let authoritative = claim.record.envelope.clone();
        if semantic_fingerprint(delivery_envelope) != semantic_fingerprint(&authoritative) {
            store
                .fail_permanently(&claim, failure_codes::TRANSPORT_INTEGRITY, now)
                .await?;
            return Ok(disp_failed(failure_codes::TRANSPORT_INTEGRITY));
        }

        // Effective timeout respects a remaining deadline; a handler is
        // never started when no positive budget remains.
        let effective_timeout = match authoritative.deadline {
            Some(deadline) if deadline <= now => {
                store
                    .fail_permanently(&claim, failure_codes::DEADLINE_EXPIRED, now)
                    .await?;
                return Ok(disp_failed(failure_codes::DEADLINE_EXPIRED));
            }
            Some(deadline) => self.execution_timeout.min(deadline - now),
            None => self.execution_timeout,
        };

        let resolved = match self
            .registry
            .resolve(&authoritative.job_name, authoritative.job_version)
        {
            Ok(resolved) => resolved,
            Err(JobError::UnknownJob(_)) => {
                store
                    .fail_permanently(&claim, failure_codes::UNKNOWN_JOB, now)
                    .await?;
                return Ok(disp_failed(failure_codes::UNKNOWN_JOB));
            }
            Err(JobError::UnsupportedJobVersion { .. }) => {
                store
                    .fail_permanently(&claim, failure_codes::UNSUPPORTED_VERSION, now)
                    .await?;
                return Ok(disp_failed(failure_codes::UNSUPPORTED_VERSION));
            }
            Err(error) => return Err(error),
        };

        if let Some(overlap_key) = authoritative.overlap_key.as_deref() {
            let acquired = locks
                .acquire(overlap_key, claim.lease_id, now + self.execution_lease, now)
                .await?;
            if !acquired {
                let retry_at = now + self.overlap_retry_delay;
                locks.release(overlap_key, claim.lease_id).await.ok();
                store
                    .schedule_retry_and_publish(&claim, failure_codes::OVERLAP_BUSY, retry_at, now)
                    .await?;
                return Ok(JobRunDisposition::Executed(
                    JobExecutionDisposition::RetryScheduled {
                        code: failure_codes::OVERLAP_BUSY.to_owned(),
                        retry_at,
                    },
                ));
            }
        }
        let overlap_key = authoritative.overlap_key.clone();

        let context = JobContext {
            job_id,
            correlation_id: authoritative.correlation_id,
            causation_id: authoritative.causation_id,
            attempt: claim.record.attempt_count.max(1),
            maximum_attempts: authoritative.maximum_attempts,
            deadline: authoritative.deadline,
            partition: authoritative.partition.clone(),
            metadata: authoritative.metadata.clone(),
        };
        let outcome = tokio::time::timeout(
            effective_timeout.to_std().map_err(|error| {
                JobError::InvalidJob(format!("execution timeout is out of range: {error}"))
            })?,
            resolved.execute(authoritative.payload.clone(), context),
        )
        .await;

        // Fresh time after execution governs completion, retry and failure
        // timestamps and the next retry calculation.
        let finished_at = clock.now();
        let failure = match outcome {
            Ok(Ok(())) => {
                store.complete(&claim, finished_at).await?;
                if let Some(overlap_key) = overlap_key.as_deref() {
                    locks.release(overlap_key, claim.lease_id).await?;
                }
                return Ok(JobRunDisposition::Executed(
                    JobExecutionDisposition::Succeeded,
                ));
            }
            Ok(Err(failure)) => failure,
            Err(_) => JobExecutionFailure::retryable(failure_codes::HANDLER_TIMEOUT),
        };

        if failure.is_permanent() {
            store
                .fail_permanently(&claim, failure.code(), finished_at)
                .await?;
            if let Some(overlap_key) = overlap_key.as_deref() {
                locks.release(overlap_key, claim.lease_id).await?;
            }
            return Ok(disp_failed(failure.code()));
        }

        let attempt = claim.record.attempt_count.max(1);
        if attempt >= authoritative.maximum_attempts {
            store
                .fail_permanently(&claim, failure_codes::RETRIES_EXHAUSTED, finished_at)
                .await?;
            if let Some(overlap_key) = overlap_key.as_deref() {
                locks.release(overlap_key, claim.lease_id).await?;
            }
            return Ok(disp_failed(failure_codes::RETRIES_EXHAUSTED));
        }

        let policy = authoritative.effective_retry();
        let retry_at = finished_at + policy.delay_for_attempt(attempt);
        if authoritative
            .deadline
            .is_some_and(|deadline| retry_at >= deadline)
        {
            store
                .fail_permanently(&claim, failure_codes::DEADLINE_EXPIRED, finished_at)
                .await?;
            if let Some(overlap_key) = overlap_key.as_deref() {
                locks.release(overlap_key, claim.lease_id).await?;
            }
            return Ok(disp_failed(failure_codes::DEADLINE_EXPIRED));
        }
        // Release the overlap boundary before the retry so the next
        // attempt is not blocked until TTL; the release is idempotent and
        // a crash between the two operations is covered by lease recovery.
        if let Some(overlap_key) = overlap_key.as_deref() {
            locks.release(overlap_key, claim.lease_id).await?;
        }
        store
            .schedule_retry_and_publish(&claim, failure.code(), retry_at, finished_at)
            .await?;
        Ok(JobRunDisposition::Executed(
            JobExecutionDisposition::RetryScheduled {
                code: failure.code().to_owned(),
                retry_at,
            },
        ))
    }

    /// Inline execution in the current process for explicit use and tests.
    /// No durable state is created or required; it is never a hidden
    /// fallback for durable infrastructure failures.
    pub async fn run_inline(
        &self,
        envelope: &JobEnvelope,
        now: DateTime<Utc>,
    ) -> Result<(), JobExecutionFailure> {
        envelope.validate().map_err(|error| {
            JobExecutionFailure::permanent(format!(
                "{}?{}",
                error.stable_code().to_lowercase(),
                error
            ))
        })?;
        if envelope.deadline.is_some_and(|deadline| deadline <= now) {
            return Err(JobExecutionFailure::permanent(
                failure_codes::DEADLINE_EXPIRED,
            ));
        }
        let resolved = self
            .registry
            .resolve(&envelope.job_name, envelope.job_version)
            .map_err(|error| match error {
                JobError::UnknownJob(_) => {
                    JobExecutionFailure::permanent(failure_codes::UNKNOWN_JOB)
                }
                JobError::UnsupportedJobVersion { .. } => {
                    JobExecutionFailure::permanent(failure_codes::UNSUPPORTED_VERSION)
                }
                other => JobExecutionFailure::permanent(other.stable_code().to_lowercase()),
            })?;
        let context = JobContext {
            job_id: envelope.job_id,
            correlation_id: envelope.correlation_id,
            causation_id: envelope.causation_id,
            attempt: envelope.attempt,
            maximum_attempts: envelope.maximum_attempts,
            deadline: envelope.deadline,
            partition: envelope.partition.clone(),
            metadata: envelope.metadata.clone(),
        };
        let effective_timeout = match envelope.deadline {
            Some(deadline) => self.execution_timeout.min(deadline - now),
            None => self.execution_timeout,
        };
        tokio::time::timeout(
            effective_timeout
                .to_std()
                .map_err(|_| JobExecutionFailure::permanent("jobs-invalid-timeout"))?,
            resolved.execute(envelope.payload.clone(), context),
        )
        .await
        .map_err(|_| JobExecutionFailure::retryable(failure_codes::HANDLER_TIMEOUT))?
    }
}

fn disp_failed(code: &str) -> JobRunDisposition {
    JobRunDisposition::Executed(JobExecutionDisposition::FailedPermanently {
        code: code.to_owned(),
    })
}

/// The composed job services registered by the plugin.
#[derive(Clone)]
pub struct JobsServices {
    pub store: Arc<dyn JobStore>,
    pub publications: Arc<dyn JobPublicationStore>,
    pub dispatcher: Arc<dyn JobDispatcher>,
    pub locks: Arc<dyn OverlapLockStore>,
    pub clock: Arc<dyn JobClock>,
    pub executor: Arc<JobExecutor>,
}

impl std::fmt::Debug for JobsServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobsServices")
            .field("store", &self.store)
            .field("publications", &self.publications)
            .field("dispatcher", &self.dispatcher)
            .field("locks", &self.locks)
            .field("clock", &self.clock)
            .finish_non_exhaustive()
    }
}

impl JobsServices {
    #[must_use]
    pub fn new(
        store: Arc<dyn JobStore>,
        publications: Arc<dyn JobPublicationStore>,
        dispatcher: Arc<dyn JobDispatcher>,
        locks: Arc<dyn OverlapLockStore>,
        clock: Arc<dyn JobClock>,
        executor: Arc<JobExecutor>,
    ) -> Self {
        Self {
            store,
            publications,
            dispatcher,
            locks,
            clock,
            executor,
        }
    }

    /// An in-memory composition for tests and the facade default stack.
    #[must_use]
    pub fn memory(
        registry: Arc<JobHandlerRegistry>,
    ) -> (Self, Arc<MemoryJobStore>, Arc<FakeJobDispatcher>) {
        let store = Arc::new(MemoryJobStore::new());
        let dispatcher = Arc::new(FakeJobDispatcher::new());
        (
            Self {
                store: store.clone(),
                publications: store.clone(),
                dispatcher: dispatcher.clone(),
                locks: store.clone(),
                clock: Arc::new(crate::policy::SystemJobClock),
                executor: Arc::new(JobExecutor::new(registry)),
            },
            store,
            dispatcher,
        )
    }

    /// Submit durably: one atomic job-plus-first-generation write outside
    /// any caller transaction. Use the SQL adapters' `enqueue_in` to share
    /// the caller's transaction.
    pub async fn submit_durable(
        &self,
        envelope: JobEnvelope,
    ) -> Result<DurableSubmission, JobError> {
        match self
            .store
            .enqueue_with_intent(crate::pending_record(envelope))
            .await?
        {
            EnqueueOutcome::Inserted(job_id) => Ok(DurableSubmission::Inserted(job_id)),
            EnqueueOutcome::Duplicate(existing) => Ok(DurableSubmission::Duplicate(existing)),
        }
    }

    /// Submit queued: serialize and publish directly with no durable row.
    /// Delivery is at least once and effects must be idempotent. The
    /// publication identity is minted here and scopes the FIFO
    /// deduplication identity.
    pub async fn submit_queued(&self, envelope: JobEnvelope) -> Result<(), JobError> {
        envelope.validate()?;
        let delivery = JobDelivery {
            envelope,
            publication_id: Uuid::now_v7(),
        };
        self.dispatcher.dispatch(&delivery, self.clock.now()).await
    }

    /// Submit inline: execute in the current process for explicit use and
    /// tests. Never a hidden fallback for failed durable infrastructure.
    pub async fn submit_inline(&self, envelope: JobEnvelope) -> Result<(), JobExecutionFailure> {
        self.executor.run_inline(&envelope, self.clock.now()).await
    }

    /// One explicit, bounded dispatch pass over due publications. This is
    /// the real publication driver: call it request-assisted after a
    /// commit, from an explicit publisher worker, or from an operator
    /// recovery command — never from a hidden timer. A failed send leaves
    /// the publication safely pending for later recovery.
    pub async fn dispatch_due_once(
        &self,
        worker_execution_id: &str,
        limit: usize,
        lease: TimeDelta,
    ) -> Result<DispatchReport, JobError> {
        let now = self.clock.now();
        crate::ports::validate_worker_claim(worker_execution_id, limit, now + lease, now)?;
        self.publications.recover_expired_claims(now).await?;
        let claimed = self
            .publications
            .claim_due(worker_execution_id, limit, now + lease, now)
            .await?;
        let mut report = DispatchReport {
            claimed: claimed.len(),
            ..DispatchReport::default()
        };
        for publication in claimed {
            let publication_id = publication.publication_id;
            let lease_id = publication
                .lease_id
                .expect("claimed publications carry a lease identity");
            let Some(record) = self.store.get(publication.job_id).await? else {
                self.publications
                    .mark_failed(
                        publication_id,
                        lease_id,
                        "JOBS-MISSING-JOB: job record missing",
                        now + PUBLICATION_RETRY_DELAY,
                    )
                    .await?;
                report.failed += 1;
                continue;
            };
            if record.status.is_terminal() {
                self.publications
                    .mark_published(publication_id, lease_id)
                    .await?;
                report.skipped_terminal += 1;
                continue;
            }
            let mut envelope = record.envelope.clone();
            envelope.attempt = record.attempt_count.max(1);
            let delivery = JobDelivery {
                envelope,
                publication_id,
            };
            match self.dispatcher.dispatch(&delivery, now).await {
                Ok(()) => {
                    self.publications
                        .mark_published(publication_id, lease_id)
                        .await?;
                    report.dispatched += 1;
                }
                Err(error) => {
                    self.publications
                        .mark_failed(
                            publication_id,
                            lease_id,
                            error.stable_code(),
                            now + PUBLICATION_RETRY_DELAY,
                        )
                        .await?;
                    report.failed += 1;
                }
            }
        }
        Ok(report)
    }
}

/// Routes dispatch by worker profile: one explicit, validated route per
/// profile. An unknown profile fails before any provider contact, and one
/// profile can never dispatch through another profile's route.
#[derive(Debug, Default)]
pub struct ProfileRoutedDispatcher {
    routes: BTreeMap<String, Arc<dyn JobDispatcher>>,
}

impl ProfileRoutedDispatcher {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind one profile to one dispatcher. Registering a profile twice
    /// fails closed.
    pub fn route(
        &mut self,
        worker_profile: &str,
        dispatcher: Arc<dyn JobDispatcher>,
    ) -> Result<(), JobError> {
        crate::envelope::validate_worker_profile(worker_profile)?;
        // Check before inserting: a rejected duplicate registration must
        // leave the existing route untouched, never replace it.
        if self.routes.contains_key(worker_profile) {
            return Err(JobError::InvalidJob(format!(
                "worker profile '{worker_profile}' already has a dispatch route"
            )));
        }
        self.routes.insert(worker_profile.to_owned(), dispatcher);
        Ok(())
    }
}

#[async_trait::async_trait]
impl JobDispatcher for ProfileRoutedDispatcher {
    async fn dispatch(&self, delivery: &JobDelivery, now: DateTime<Utc>) -> Result<(), JobError> {
        let route = self
            .routes
            .get(&delivery.envelope.worker_profile)
            .ok_or_else(|| {
                JobError::UnknownWorkerProfile(delivery.envelope.worker_profile.clone())
            })?;
        route.dispatch(delivery, now).await
    }
}

/// Official jobs plugin: registers [`JobsServices`] behind explicit
/// composition. The plugin never creates a schedule, daemon or poller.
#[derive(Debug, Clone)]
pub struct JobsPlugin {
    services: JobsServices,
}

impl JobsPlugin {
    #[must_use]
    pub const fn new(services: JobsServices) -> Self {
        Self { services }
    }

    /// In-memory plugin for tests and the facade default stack.
    #[must_use]
    pub fn memory(
        registry: Arc<JobHandlerRegistry>,
    ) -> (Self, Arc<MemoryJobStore>, Arc<FakeJobDispatcher>) {
        let (services, store, dispatcher) = JobsServices::memory(registry);
        (Self { services }, store, dispatcher)
    }
}

impl minco_core::Plugin for JobsPlugin {
    fn descriptor(&self) -> minco_core::PluginDescriptor {
        let mut descriptor = minco_core::PluginDescriptor::new(
            minco_core::PluginId::new("jobs").expect("valid plugin id"),
            semver::Version::new(1, 0, 0),
            "Durable typed work with at-least-once delivery and explicit scheduling",
        );
        descriptor.documentation = Some("https://docs.rs/minco-plugin-jobs".into());
        descriptor.core_compatibility =
            semver::VersionReq::parse(concat!("^", env!("CARGO_PKG_VERSION")))
                .expect("valid core compatibility");
        descriptor.stability = minco_core::PluginStability::Beta;
        descriptor.default_enabled = false;
        descriptor.data_classes = vec![
            minco_core::DataClass::Internal,
            minco_core::DataClass::CustomerProvided,
        ];
        descriptor.provides = vec![
            minco_core::CapabilityProvision {
                name: "jobs.submit".into(),
                version: semver::Version::new(1, 0, 0),
            },
            minco_core::CapabilityProvision {
                name: "jobs.dispatch".into(),
                version: semver::Version::new(1, 0, 0),
            },
            minco_core::CapabilityProvision {
                name: "jobs.execution".into(),
                version: semver::Version::new(1, 0, 0),
            },
        ];
        descriptor
    }

    fn install(
        &self,
        context: &mut minco_core::PluginContext<'_>,
    ) -> Result<(), minco_core::PluginError> {
        context.services().insert(Arc::new(self.services.clone()))?;
        Ok(())
    }
}
