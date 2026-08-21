//! Job services: submission modes, the due-publication dispatcher and the
//! lease-based executor that connects a delivery to one typed handler.
//!
//! Nothing here schedules itself. Dispatch is explicit and bounded; retries
//! are durable; permanent failure is persisted before acknowledgement.

use std::sync::Arc;

use chrono::{DateTime, TimeDelta, Utc};
use uuid::Uuid;

use crate::envelope::JobEnvelope;
use crate::memory::{FakeJobDispatcher, MemoryJobStore};
use crate::policy::JobClock;
use crate::ports::{
    EnqueueOutcome, JobDispatcher, JobError, JobPublicationStore, JobStatus, JobStore,
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
    /// An identical dedupe-keyed job already exists; nothing was written.
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
    /// The durable record no longer exists.
    Missing,
    /// The job is terminal or leased by another live worker.
    NotExecutable(JobStatus),
    /// The job is pending but its availability time has not arrived.
    NotYetAvailable,
}

/// Executes one job delivery: claim, dedupe, deadline, resolve, overlap,
/// timeout, and the durable transition before acknowledgement.
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

    /// Run one delivery to a durable disposition. Store and publication
    /// transitions happen before the disposition is returned, so callers can
    /// safely acknowledge the transport.
    pub async fn run(
        &self,
        envelope: &JobEnvelope,
        worker_id: &str,
        now: DateTime<Utc>,
        store: &dyn JobStore,
        publications: &dyn JobPublicationStore,
        locks: &dyn OverlapLockStore,
    ) -> Result<JobRunDisposition, JobError> {
        let job_id = envelope.job_id;
        let Some(record) = store
            .claim_execution(job_id, worker_id, now + self.execution_lease, now)
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

        if envelope.deadline.is_some_and(|deadline| deadline <= now) {
            store
                .fail_permanently(job_id, worker_id, failure_codes::DEADLINE_EXPIRED, now)
                .await?;
            return Ok(disp_failed(failure_codes::DEADLINE_EXPIRED));
        }

        let resolved = match self
            .registry
            .resolve(&envelope.job_name, envelope.job_version)
        {
            Ok(resolved) => resolved,
            Err(JobError::UnknownJob(_)) => {
                store
                    .fail_permanently(job_id, worker_id, failure_codes::UNKNOWN_JOB, now)
                    .await?;
                return Ok(disp_failed(failure_codes::UNKNOWN_JOB));
            }
            Err(JobError::UnsupportedJobVersion { .. }) => {
                store
                    .fail_permanently(job_id, worker_id, failure_codes::UNSUPPORTED_VERSION, now)
                    .await?;
                return Ok(disp_failed(failure_codes::UNSUPPORTED_VERSION));
            }
            Err(error) => return Err(error),
        };

        if let Some(overlap_key) = envelope.overlap_key.as_deref() {
            let acquired = locks
                .acquire(overlap_key, worker_id, now + self.execution_lease, now)
                .await?;
            if !acquired {
                let retry_at = now + self.overlap_retry_delay;
                store
                    .schedule_retry(
                        job_id,
                        worker_id,
                        failure_codes::OVERLAP_BUSY,
                        retry_at,
                        now,
                    )
                    .await?;
                publications.republish(job_id, worker_id, retry_at).await?;
                return Ok(JobRunDisposition::Executed(
                    JobExecutionDisposition::RetryScheduled {
                        code: failure_codes::OVERLAP_BUSY.to_owned(),
                        retry_at,
                    },
                ));
            }
        }

        let context = JobContext {
            job_id,
            correlation_id: envelope.correlation_id,
            causation_id: envelope.causation_id,
            attempt: record.attempt_count.max(1),
            maximum_attempts: envelope.maximum_attempts,
            deadline: envelope.deadline,
            partition: envelope.partition.clone(),
            metadata: envelope.metadata.clone(),
        };
        let outcome = tokio::time::timeout(
            self.execution_timeout.to_std().map_err(|error| {
                JobError::InvalidJob(format!("execution timeout is out of range: {error}"))
            })?,
            resolved.execute(envelope.payload.clone(), context),
        )
        .await;

        let failure = match outcome {
            Ok(Ok(())) => {
                store.complete(job_id, worker_id, now).await?;
                if let Some(overlap_key) = envelope.overlap_key.as_deref() {
                    locks.release(overlap_key, worker_id).await?;
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
                .fail_permanently(job_id, worker_id, failure.code(), now)
                .await?;
            if let Some(overlap_key) = envelope.overlap_key.as_deref() {
                locks.release(overlap_key, worker_id).await?;
            }
            return Ok(disp_failed(failure.code()));
        }

        let attempt = record.attempt_count.max(1);
        if attempt >= envelope.maximum_attempts {
            store
                .fail_permanently(job_id, worker_id, failure_codes::RETRIES_EXHAUSTED, now)
                .await?;
            if let Some(overlap_key) = envelope.overlap_key.as_deref() {
                locks.release(overlap_key, worker_id).await?;
            }
            return Ok(disp_failed(failure_codes::RETRIES_EXHAUSTED));
        }

        let policy = envelope.effective_retry();
        let retry_at = now + policy.delay_for_attempt(attempt);
        if envelope
            .deadline
            .is_some_and(|deadline| retry_at >= deadline)
        {
            store
                .fail_permanently(job_id, worker_id, failure_codes::DEADLINE_EXPIRED, now)
                .await?;
            if let Some(overlap_key) = envelope.overlap_key.as_deref() {
                locks.release(overlap_key, worker_id).await?;
            }
            return Ok(disp_failed(failure_codes::DEADLINE_EXPIRED));
        }
        store
            .schedule_retry(job_id, worker_id, failure.code(), retry_at, now)
            .await?;
        publications.republish(job_id, worker_id, retry_at).await?;
        Ok(JobRunDisposition::Executed(
            JobExecutionDisposition::RetryScheduled {
                code: failure.code().to_owned(),
                retry_at,
            },
        ))
    }

    /// Inline execution in the current process for explicit use and tests.
    /// No durable state is created or required.
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
        tokio::time::timeout(
            self.execution_timeout
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

    /// Submit durably: one atomic job-plus-intent write outside any caller
    /// transaction. Use the SQL adapters' `enqueue_in` to share the caller's
    /// transaction.
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
    /// Delivery is at least once and effects must be idempotent.
    pub async fn submit_queued(&self, envelope: JobEnvelope) -> Result<(), JobError> {
        envelope.validate()?;
        self.dispatcher.dispatch(&envelope, self.clock.now()).await
    }

    /// Submit inline: execute in the current process for explicit use and
    /// tests. Never a hidden fallback for failed durable infrastructure.
    pub async fn submit_inline(&self, envelope: JobEnvelope) -> Result<(), JobExecutionFailure> {
        self.executor.run_inline(&envelope, self.clock.now()).await
    }

    /// One explicit, bounded dispatch pass over due publications. Publication
    /// failures retry after [`PUBLICATION_RETRY_DELAY`]; terminal jobs are
    /// acknowledged without delivery. Never scheduled by this crate.
    pub async fn dispatch_due_once(
        &self,
        worker_id: &str,
        limit: usize,
        lease: TimeDelta,
    ) -> Result<DispatchReport, JobError> {
        let now = self.clock.now();
        crate::ports::validate_worker_claim(worker_id, limit, now + lease, now)?;
        self.publications.recover_expired_claims(now).await?;
        let claimed = self
            .publications
            .claim_due(worker_id, limit, now + lease, now)
            .await?;
        let mut report = DispatchReport {
            claimed: claimed.len(),
            ..DispatchReport::default()
        };
        for publication in claimed {
            let job_id = publication.job_id;
            let Some(record) = self.store.get(job_id).await? else {
                self.publications
                    .mark_failed(
                        job_id,
                        worker_id,
                        "job record missing",
                        now + PUBLICATION_RETRY_DELAY,
                    )
                    .await?;
                report.failed += 1;
                continue;
            };
            if record.status.is_terminal() {
                self.publications.mark_published(job_id, worker_id).await?;
                report.skipped_terminal += 1;
                continue;
            }
            let mut envelope = record.envelope.clone();
            envelope.attempt = record.attempt_count.max(1);
            match self.dispatcher.dispatch(&envelope, now).await {
                Ok(()) => {
                    self.publications.mark_published(job_id, worker_id).await?;
                    report.dispatched += 1;
                }
                Err(error) => {
                    self.publications
                        .mark_failed(
                            job_id,
                            worker_id,
                            &format!("{}: {error}", error.stable_code()),
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
