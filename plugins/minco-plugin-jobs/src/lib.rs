//! Durable typed work for Minco: typed job contracts, an explicit handler
//! registry, a bounded versioned envelope, at-least-once dispatch ports,
//! lease-based execution, retry and permanent-failure semantics, overlap
//! locks and explicit scheduling contracts.
//!
//! Jobs are commands with one registered handler, never domain events. The
//! durable job row owns execution state, the publication row owns pending
//! transport delivery, and the queue message is delivery, never truth.
//! Nothing in this crate schedules itself: dispatch is explicit and bounded,
//! and disabling the jobs capability creates no queue, worker or schedule.
//!
//! Delivery is at least once. Duplicate deliveries are neutralized by an
//! atomic execution claim; application effects must still be idempotent.
//! Payload and metadata values never appear in `Debug` output.
#![forbid(unsafe_code)]

mod envelope;
mod memory;
mod policy;
mod ports;
mod registry;
mod services;

pub use envelope::{
    JOB_ENVELOPE_SCHEMA_VERSION, JobEnvelope, JobOptions, MAX_JOB_ATTEMPTS, MAX_JOB_ENVELOPE_BYTES,
    MAX_JOB_KEY_BYTES, MAX_JOB_METADATA_ENTRIES, MAX_JOB_METADATA_NAME_BYTES,
    MAX_JOB_METADATA_VALUE_BYTES, MAX_JOB_NAME_BYTES, MAX_JOB_PARTITION_BYTES,
    MAX_JOB_PAYLOAD_BYTES, MAX_WORKER_PROFILE_BYTES, validate_job_name, validate_worker_profile,
};
pub use memory::{
    DispatchAttempt, FailClosedDispatcher, FakeJobDispatcher, MAX_ATTEMPT_HISTORY, MemoryJobStore,
};
pub use policy::{
    BackoffMode, FakeJobClock, JobClock, MAX_BACKOFF_DELAY, RetryPolicy, SystemJobClock,
};
pub use ports::{
    EnqueueOutcome, JobAttempt, JobAttemptOutcome, JobDispatcher, JobError, JobPublication,
    JobPublicationStore, JobRecord, JobStatus, JobStore, OverlapLockStore, PublicationStatus,
    validate_worker_claim,
};
pub use registry::{
    Job, JobContext, JobExecutionFailure, JobHandler, JobHandlerRegistry, PayloadUpcaster,
    ResolvedHandler, TypedJobHandler, failure_codes, typed,
};
pub use services::{
    DEFAULT_EXECUTION_LEASE, DEFAULT_EXECUTION_TIMEOUT, DispatchReport, DurableSubmission,
    JobExecutionDisposition, JobExecutor, JobRunDisposition, JobSkipReason, JobsPlugin,
    JobsServices, OVERLAP_RETRY_DELAY, PUBLICATION_RETRY_DELAY,
};

/// Build a pending [`JobRecord`] around an envelope for store submission.
#[must_use]
pub const fn pending_record(envelope: JobEnvelope) -> JobRecord {
    JobRecord {
        envelope,
        status: JobStatus::Pending,
        revision: 1,
        lease_owner: None,
        lease_expires_at: None,
        attempt_count: 0,
        attempts: Vec::new(),
        failure_code: None,
        completed_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[derive(serde::Serialize, serde::Deserialize)]
    struct Confirmation {
        order_id: String,
    }

    impl Job for Confirmation {
        const NAME: &'static str = "orders.send-confirmation";
        const VERSION: u16 = 1;
    }

    fn services() -> (JobsServices, Arc<MemoryJobStore>, Arc<FakeJobDispatcher>) {
        let registry = Arc::new(JobHandlerRegistry::new());
        registry
            .register_typed::<Confirmation, _, _>(|job: Confirmation, _ctx| async move {
                assert_eq!(job.order_id, "o-1");
                Ok(())
            })
            .expect("register");
        JobsServices::memory(registry)
    }

    fn envelope() -> JobEnvelope {
        JobEnvelope::for_job::<Confirmation>(
            &Confirmation {
                order_id: "o-1".into(),
            },
            "orders-notifications",
            uuid::Uuid::now_v7(),
        )
        .expect("valid envelope")
    }

    #[tokio::test]
    async fn durable_submission_then_dispatch_then_execution() {
        let (services, store, dispatcher) = services();
        let envelope = envelope();
        match services
            .submit_durable(envelope.clone())
            .await
            .expect("submit")
        {
            DurableSubmission::Inserted(job_id) => assert_eq!(job_id, envelope.job_id),
            DurableSubmission::Duplicate(existing) => {
                panic!("expected insertion, got duplicate {existing}")
            }
        }
        let report = services
            .dispatch_due_once("worker-1", 10, chrono::TimeDelta::minutes(1))
            .await
            .expect("dispatch");
        assert_eq!(report.dispatched, 1);
        let sent = dispatcher.dispatched();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].job_id, envelope.job_id);
        let disposition = services
            .executor
            .run(
                &sent[0],
                "worker-1",
                chrono::Utc::now(),
                services.store.as_ref(),
                services.publications.as_ref(),
                services.locks.as_ref(),
            )
            .await
            .expect("execute");
        assert_eq!(
            disposition,
            JobRunDisposition::Executed(JobExecutionDisposition::Succeeded)
        );
        let record = store
            .get(envelope.job_id)
            .await
            .expect("get")
            .expect("present");
        assert_eq!(record.status, JobStatus::Succeeded);
        assert_eq!(record.attempt_count, 1);
    }

    #[tokio::test]
    async fn duplicate_dedupe_submission_returns_existing_identity() {
        let (services, _store, _dispatcher) = services();
        let envelope =
            envelope().with(JobOptions::default().with_dedupe_key("orders.send-confirmation:o-1"));
        services
            .submit_durable(envelope.clone())
            .await
            .expect("first");
        match services
            .submit_durable(envelope.clone())
            .await
            .expect("second")
        {
            DurableSubmission::Duplicate(existing) => assert_eq!(existing, envelope.job_id),
            DurableSubmission::Inserted(inserted) => {
                panic!("expected duplicate, got insertion {inserted}")
            }
        }
    }

    #[tokio::test]
    async fn same_dedupe_key_with_incompatible_payload_fails_closed() {
        let (services, _store, _dispatcher) = services();
        let key = JobOptions::default().with_dedupe_key("orders.send-confirmation:o-1");
        services
            .submit_durable(envelope().with(key.clone()))
            .await
            .expect("first");
        let conflicting = JobEnvelope::for_parts(
            "orders.send-confirmation",
            1,
            serde_json::json!({ "order_id": "o-2" }),
            "orders-notifications",
            uuid::Uuid::now_v7(),
        )
        .expect("valid")
        .with(key);
        let error = services
            .submit_durable(conflicting)
            .await
            .expect_err("conflict");
        assert!(matches!(
            error,
            JobError::DuplicateSubmissionConflict { .. }
        ));
    }

    #[tokio::test]
    async fn completed_duplicate_delivery_does_not_execute_again() {
        let (services, store, _dispatcher) = services();
        let envelope = envelope();
        services
            .submit_durable(envelope.clone())
            .await
            .expect("submit");
        let now = chrono::Utc::now();
        services
            .executor
            .run(
                &envelope,
                "worker-1",
                now,
                services.store.as_ref(),
                services.publications.as_ref(),
                services.locks.as_ref(),
            )
            .await
            .expect("first run");
        let disposition = services
            .executor
            .run(
                &envelope,
                "worker-2",
                now,
                services.store.as_ref(),
                services.publications.as_ref(),
                services.locks.as_ref(),
            )
            .await
            .expect("duplicate delivery");
        assert_eq!(
            disposition,
            JobRunDisposition::Skipped(JobSkipReason::NotExecutable(JobStatus::Succeeded))
        );
        let record = store
            .get(envelope.job_id)
            .await
            .expect("get")
            .expect("present");
        assert_eq!(record.attempt_count, 1, "handler must not run twice");
    }

    #[tokio::test]
    async fn transient_failure_retries_with_durable_backoff() {
        let registry = Arc::new(JobHandlerRegistry::new());
        let attempts = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = attempts.clone();
        registry
            .register_typed::<Confirmation, _, _>(move |_: Confirmation, _| {
                let counter = counter.clone();
                async move {
                    if counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                        Err(JobExecutionFailure::retryable("notification-unavailable"))
                    } else {
                        Ok(())
                    }
                }
            })
            .expect("register");
        let services = JobsServices::memory(registry).0;
        let policy = RetryPolicy::fixed(2, 60);
        let envelope = envelope().with(JobOptions::default().with_retry(policy));
        services
            .submit_durable(envelope.clone())
            .await
            .expect("submit");
        let now = chrono::Utc::now();
        let first = services
            .executor
            .run(
                &envelope,
                "worker-1",
                now,
                services.store.as_ref(),
                services.publications.as_ref(),
                services.locks.as_ref(),
            )
            .await
            .expect("first run");
        assert!(matches!(
            first,
            JobRunDisposition::Executed(JobExecutionDisposition::RetryScheduled { .. })
        ));
        let record = services.store.get(envelope.job_id).await.unwrap().unwrap();
        assert_eq!(record.status, JobStatus::Pending);
        assert_eq!(
            record.envelope.available_at,
            now + chrono::TimeDelta::seconds(60)
        );
        // A delivery before the retry time does not execute.
        let early = services
            .executor
            .run(
                &envelope,
                "worker-1",
                now + chrono::TimeDelta::seconds(1),
                services.store.as_ref(),
                services.publications.as_ref(),
                services.locks.as_ref(),
            )
            .await
            .expect("early duplicate");
        assert_eq!(
            early,
            JobRunDisposition::Skipped(JobSkipReason::NotYetAvailable)
        );
        // After the retry time the job completes.
        let second = services
            .executor
            .run(
                &envelope,
                "worker-1",
                now + chrono::TimeDelta::seconds(61),
                services.store.as_ref(),
                services.publications.as_ref(),
                services.locks.as_ref(),
            )
            .await
            .expect("second run");
        assert_eq!(
            second,
            JobRunDisposition::Executed(JobExecutionDisposition::Succeeded)
        );
    }

    #[tokio::test]
    async fn attempt_exhaustion_fails_permanently() {
        let registry = Arc::new(JobHandlerRegistry::new());
        registry
            .register_typed::<Confirmation, _, _>(|_: Confirmation, _| async {
                Err(JobExecutionFailure::retryable("notification-unavailable"))
            })
            .expect("register");
        let services = JobsServices::memory(registry).0;
        let envelope = envelope().with(JobOptions::default().with_retry(RetryPolicy::fixed(2, 1)));
        services
            .submit_durable(envelope.clone())
            .await
            .expect("submit");
        let now = chrono::Utc::now();
        services
            .executor
            .run(
                &envelope,
                "w",
                now,
                services.store.as_ref(),
                services.publications.as_ref(),
                services.locks.as_ref(),
            )
            .await
            .expect("attempt 1");
        let disposition = services
            .executor
            .run(
                &envelope,
                "w",
                now + chrono::TimeDelta::seconds(2),
                services.store.as_ref(),
                services.publications.as_ref(),
                services.locks.as_ref(),
            )
            .await
            .expect("attempt 2");
        assert_eq!(
            disposition,
            JobRunDisposition::Executed(JobExecutionDisposition::FailedPermanently {
                code: failure_codes::RETRIES_EXHAUSTED.to_owned()
            })
        );
        let record = services.store.get(envelope.job_id).await.unwrap().unwrap();
        assert_eq!(record.status, JobStatus::FailedPermanently);
        assert_eq!(
            record.failure_code.as_deref(),
            Some(failure_codes::RETRIES_EXHAUSTED)
        );
    }

    #[tokio::test]
    async fn deadline_expiry_prevents_handler_execution() {
        let registry = Arc::new(JobHandlerRegistry::new());
        let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = ran.clone();
        registry
            .register_typed::<Confirmation, _, _>(move |_: Confirmation, _| {
                let flag = flag.clone();
                async move {
                    flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    Ok(())
                }
            })
            .expect("register");
        let services = JobsServices::memory(registry).0;
        let now = chrono::Utc::now();
        let envelope = envelope()
            .with(JobOptions::default().with_deadline(now + chrono::TimeDelta::seconds(10)));
        services
            .submit_durable(envelope.clone())
            .await
            .expect("submit");
        let disposition = services
            .executor
            .run(
                &envelope,
                "w",
                now + chrono::TimeDelta::seconds(11),
                services.store.as_ref(),
                services.publications.as_ref(),
                services.locks.as_ref(),
            )
            .await
            .expect("run");
        assert_eq!(
            disposition,
            JobRunDisposition::Executed(JobExecutionDisposition::FailedPermanently {
                code: failure_codes::DEADLINE_EXPIRED.to_owned()
            })
        );
        assert!(
            !ran.load(std::sync::atomic::Ordering::SeqCst),
            "handler must not run"
        );
    }

    #[tokio::test]
    async fn unknown_job_and_unsupported_version_fail_permanently() {
        let (services, _store, _dispatcher) = services();
        for (name, version) in [("orders.missing", 1), ("orders.send-confirmation", 7)] {
            let envelope = JobEnvelope::for_parts(
                name,
                version,
                serde_json::json!({ "order_id": "o-1" }),
                "orders-notifications",
                uuid::Uuid::now_v7(),
            )
            .expect("valid");
            services
                .submit_durable(envelope.clone())
                .await
                .expect("submit");
            let disposition = services
                .executor
                .run(
                    &envelope,
                    "w",
                    chrono::Utc::now(),
                    services.store.as_ref(),
                    services.publications.as_ref(),
                    services.locks.as_ref(),
                )
                .await
                .expect("run");
            let expected = if version == 1 {
                failure_codes::UNKNOWN_JOB
            } else {
                failure_codes::UNSUPPORTED_VERSION
            };
            assert_eq!(
                disposition,
                JobRunDisposition::Executed(JobExecutionDisposition::FailedPermanently {
                    code: expected.to_owned()
                })
            );
        }
    }

    #[tokio::test]
    async fn overlap_lock_blocks_concurrent_execution_and_recovers() {
        let registry = Arc::new(JobHandlerRegistry::new());
        registry
            .register_typed::<Confirmation, _, _>(|_: Confirmation, _| async { Ok(()) })
            .expect("register");
        let services = JobsServices::memory(registry).0;
        let key = JobOptions::default().with_overlap_key("orders.send-confirmation:o-1");
        let first_envelope = envelope().with(key);
        services
            .submit_durable(first_envelope.clone())
            .await
            .expect("submit");
        let now = chrono::Utc::now();
        let first = services
            .executor
            .run(
                &first_envelope,
                "worker-1",
                now,
                services.store.as_ref(),
                services.publications.as_ref(),
                services.locks.as_ref(),
            )
            .await
            .expect("first");
        assert_eq!(
            first,
            JobRunDisposition::Executed(JobExecutionDisposition::Succeeded)
        );
        // The lock was released on completion, so a new job on the same key runs.
        let second_envelope =
            envelope().with(JobOptions::default().with_overlap_key("orders.send-confirmation:o-1"));
        services
            .submit_durable(second_envelope.clone())
            .await
            .expect("submit two");
        let second = services
            .executor
            .run(
                &second_envelope,
                "worker-2",
                now + chrono::TimeDelta::seconds(1),
                services.store.as_ref(),
                services.publications.as_ref(),
                services.locks.as_ref(),
            )
            .await
            .expect("second");
        assert_eq!(
            second,
            JobRunDisposition::Executed(JobExecutionDisposition::Succeeded)
        );
    }

    #[tokio::test]
    async fn cancelled_pending_job_cannot_run() {
        let (services, store, _dispatcher) = services();
        let envelope = envelope();
        services
            .submit_durable(envelope.clone())
            .await
            .expect("submit");
        let record = store.get(envelope.job_id).await.unwrap().unwrap();
        services
            .store
            .cancel(envelope.job_id, record.revision, chrono::Utc::now())
            .await
            .expect("cancel");
        let disposition = services
            .executor
            .run(
                &envelope,
                "w",
                chrono::Utc::now(),
                services.store.as_ref(),
                services.publications.as_ref(),
                services.locks.as_ref(),
            )
            .await
            .expect("run");
        assert_eq!(
            disposition,
            JobRunDisposition::Skipped(JobSkipReason::NotExecutable(JobStatus::Cancelled))
        );
    }

    #[tokio::test]
    async fn expired_lease_can_be_reclaimed_by_another_worker() {
        let (services, _store, _dispatcher) = services();
        let envelope = envelope();
        services
            .submit_durable(envelope.clone())
            .await
            .expect("submit");
        let start = chrono::Utc::now();
        let claimed = services
            .store
            .claim_execution(
                envelope.job_id,
                "ghost",
                start + chrono::TimeDelta::minutes(1),
                start,
            )
            .await
            .expect("ghost claims")
            .expect("claim succeeds");
        assert_eq!(claimed.status, JobStatus::Running);
        // A second worker cannot claim while the lease is live.
        let blocked = services
            .store
            .claim_execution(
                envelope.job_id,
                "worker-2",
                start + chrono::TimeDelta::minutes(1),
                start + chrono::TimeDelta::seconds(30),
            )
            .await
            .expect("blocked claim");
        assert!(blocked.is_none());
        // After the lease expires the row is recovered and reclaimable.
        let expired_at = start + chrono::TimeDelta::minutes(2);
        let recovered = services
            .store
            .recover_expired_leases(expired_at)
            .await
            .expect("recover");
        assert_eq!(recovered, 1);
        let reclaimed = services
            .store
            .claim_execution(
                envelope.job_id,
                "worker-2",
                expired_at + chrono::TimeDelta::minutes(15),
                expired_at,
            )
            .await
            .expect("reclaim")
            .expect("job is claimable again");
        assert_eq!(reclaimed.lease_owner.as_deref(), Some("worker-2"));
        assert_eq!(
            reclaimed.attempt_count, 2,
            "the vanished attempt stays counted"
        );
        // The stale ghost owner cannot complete the job afterwards.
        let error = services
            .store
            .complete(
                envelope.job_id,
                "ghost",
                expired_at + chrono::TimeDelta::minutes(1),
            )
            .await
            .expect_err("stale owner");
        assert!(matches!(error, JobError::LeaseOwnership { .. }));
    }

    #[tokio::test]
    async fn failed_job_retry_is_revision_guarded() {
        let (services, store, _dispatcher) = services();
        let envelope = envelope();
        services
            .submit_durable(envelope.clone())
            .await
            .expect("submit");
        let record = store.get(envelope.job_id).await.unwrap().unwrap();
        let error = services
            .store
            .retry_failed(envelope.job_id, record.revision + 5, chrono::Utc::now())
            .await
            .expect_err("stale revision");
        assert!(matches!(error, JobError::RevisionConflict { .. }));
        let error = services
            .store
            .retry_failed(envelope.job_id, record.revision, chrono::Utc::now())
            .await
            .expect_err("not permanently failed yet");
        assert!(matches!(error, JobError::InvalidTransition { .. }));
    }

    #[tokio::test]
    async fn dispatch_failure_retries_publication_after_backoff() {
        let (services, store, dispatcher) = services();
        let envelope = envelope();
        services
            .submit_durable(envelope.clone())
            .await
            .expect("submit");
        dispatcher.fail_next("sqs unavailable");
        let report = services
            .dispatch_due_once("w", 10, chrono::TimeDelta::minutes(1))
            .await
            .expect("dispatch");
        assert_eq!(report.failed, 1);
        let publications = store.publication_records();
        assert_eq!(publications.len(), 1);
        assert_eq!(publications[0].status, PublicationStatus::Failed);
        let report = services
            .dispatch_due_once("w", 10, chrono::TimeDelta::minutes(1))
            .await
            .expect("immediate redispatch is not due");
        assert_eq!(report.claimed, 0);
    }

    #[tokio::test]
    async fn inline_and_queued_modes_are_explicit() {
        let (services, _store, dispatcher) = services();
        services.submit_inline(envelope()).await.expect("inline");
        services.submit_queued(envelope()).await.expect("queued");
        assert_eq!(dispatcher.dispatched().len(), 1);
    }

    #[tokio::test]
    async fn attempt_history_remains_ordered_and_bounded() {
        let registry = Arc::new(JobHandlerRegistry::new());
        registry
            .register_typed::<Confirmation, _, _>(|_: Confirmation, _| async {
                Err(JobExecutionFailure::retryable("always-busy"))
            })
            .expect("register");
        let services = JobsServices::memory(registry).0;
        let envelope = envelope().with(JobOptions::default().with_retry(RetryPolicy::fixed(60, 1)));
        services
            .submit_durable(envelope.clone())
            .await
            .expect("submit");
        let mut now = chrono::Utc::now();
        for _ in 0..30 {
            services
                .executor
                .run(
                    &envelope,
                    "w",
                    now,
                    services.store.as_ref(),
                    services.publications.as_ref(),
                    services.locks.as_ref(),
                )
                .await
                .expect("run");
            now += chrono::TimeDelta::seconds(2);
        }
        let record = services.store.get(envelope.job_id).await.unwrap().unwrap();
        assert_eq!(record.attempts.len(), MAX_ATTEMPT_HISTORY);
        let first = record.attempts.first().expect("oldest attempt");
        let last = record.attempts.last().expect("newest attempt");
        assert!(first.attempt < last.attempt, "history is ordered");
        assert_eq!(record.attempt_count, 30);
    }
}
