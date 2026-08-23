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
    MAX_JOB_PAYLOAD_BYTES, MAX_WORKER_PROFILE_BYTES, semantic_fingerprint, validate_job_name,
    validate_worker_profile,
};
pub use memory::{
    DispatchAttempt, FailClosedDispatcher, FakeJobDispatcher, MAX_ATTEMPT_HISTORY, MemoryJobStore,
};
pub use policy::{
    BackoffMode, FakeJobClock, JobClock, MAX_BACKOFF_DELAY, RetryPolicy, SystemJobClock,
};
pub use ports::{
    EnqueueOutcome, IngestOutcome, JobAttempt, JobAttemptOutcome, JobClaim, JobDelivery,
    JobDispatcher, JobError, JobPublication, JobPublicationStore, JobRecord, JobStatus, JobStore,
    OverlapLockStore, PublicationStatus, validate_worker_claim,
};
pub use registry::{
    Job, JobContext, JobExecutionFailure, JobHandler, JobHandlerRegistry, PayloadUpcaster,
    ResolvedHandler, TypedJobHandler, failure_codes, typed,
};
pub use services::{
    DEFAULT_EXECUTION_LEASE, DEFAULT_EXECUTION_TIMEOUT, DispatchReport, DurableSubmission,
    JobExecutionDisposition, JobExecutor, JobRunDisposition, JobSkipReason, JobsPlugin,
    JobsServices, OVERLAP_RETRY_DELAY, PUBLICATION_RETRY_DELAY, ProfileRoutedDispatcher,
};

/// Build a pending [`JobRecord`] around an envelope for store submission.
#[must_use]
pub const fn pending_record(envelope: JobEnvelope) -> JobRecord {
    JobRecord {
        envelope,
        status: JobStatus::Pending,
        revision: 1,
        lease_id: None,
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
    use crate::policy::FakeJobClock;
    use chrono::TimeDelta;
    use std::sync::Arc;

    #[derive(serde::Serialize, serde::Deserialize)]
    struct Confirmation {
        order_id: String,
    }

    impl Job for Confirmation {
        const NAME: &'static str = "orders.send-confirmation";
        const VERSION: u16 = 1;
    }

    struct Harness {
        services: JobsServices,
        store: Arc<MemoryJobStore>,
        dispatcher: Arc<FakeJobDispatcher>,
        clock: Arc<FakeJobClock>,
    }

    fn services() -> Harness {
        let registry = Arc::new(JobHandlerRegistry::new());
        registry
            .register_typed::<Confirmation, _, _>(|job: Confirmation, _ctx| async move {
                assert_eq!(job.order_id, "o-1");
                Ok(())
            })
            .expect("register");
        let (services, store, dispatcher) = JobsServices::memory(registry);
        let clock = Arc::new(FakeJobClock::starting(chrono::Utc::now()));
        Harness {
            services: JobsServices {
                clock: clock.clone(),
                ..services
            },
            store,
            dispatcher,
            clock,
        }
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

    async fn run(
        harness: &Harness,
        envelope: &JobEnvelope,
        worker_execution_id: &str,
    ) -> Result<JobRunDisposition, JobError> {
        // Absorb sub-second construction skew: envelopes are minted with
        // the real clock after the deterministic clock was frozen.
        harness.clock.advance(TimeDelta::milliseconds(50));
        harness
            .services
            .executor
            .run(
                envelope,
                worker_execution_id,
                harness.services.clock.as_ref(),
                harness.services.store.as_ref(),
                harness.services.publications.as_ref(),
                harness.services.locks.as_ref(),
            )
            .await
    }

    #[tokio::test]
    async fn durable_submission_dispatch_and_execution_lifecycle() {
        let harness = services();
        let envelope = envelope();
        match harness
            .services
            .submit_durable(envelope.clone())
            .await
            .expect("submit")
        {
            DurableSubmission::Inserted(job_id) => assert_eq!(job_id, envelope.job_id),
            DurableSubmission::Duplicate(existing) => {
                panic!("expected insertion, got duplicate {existing}")
            }
        }
        harness.clock.advance(TimeDelta::milliseconds(50));
        let report = harness
            .services
            .dispatch_due_once("dispatch-1", 10, TimeDelta::minutes(1))
            .await
            .expect("dispatch");
        assert_eq!(report.dispatched, 1);
        let sent = harness.dispatcher.dispatched();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].envelope.job_id, envelope.job_id);
        let disposition = run(&harness, &sent[0].envelope, "worker-exec-1")
            .await
            .expect("execute");
        assert_eq!(
            disposition,
            JobRunDisposition::Executed(JobExecutionDisposition::Succeeded)
        );
        let record = harness
            .store
            .get(envelope.job_id)
            .await
            .expect("get")
            .expect("present");
        assert_eq!(record.status, JobStatus::Succeeded);
        assert_eq!(record.attempt_count, 1);
    }

    #[tokio::test]
    async fn altered_transport_copies_cannot_override_the_durable_record() {
        for mutate in [
            "payload",
            "retry-policy",
            "deadline",
            "maximum-attempts",
            "worker-profile",
            "overlap-key",
        ] {
            let harness = services();
            let envelope = envelope().with(
                JobOptions::default()
                    .with_retry(RetryPolicy::fixed(3, 60))
                    .with_overlap_key("orders.confirm:o-1"),
            );
            harness
                .services
                .submit_durable(envelope.clone())
                .await
                .expect("submit");
            let mut forged = envelope.clone();
            match mutate {
                "payload" => forged.payload = serde_json::json!({ "order_id": "o-forged" }),
                "retry-policy" => forged.retry = Some(RetryPolicy::fixed(9, 9)),
                "deadline" => {
                    forged.deadline = Some(forged.created_at + TimeDelta::days(3));
                }
                "maximum-attempts" => forged.maximum_attempts = 99,
                "worker-profile" => forged.worker_profile = "attacker-profile".into(),
                "overlap-key" => forged.overlap_key = Some("other-boundary".into()),
                _ => unreachable!(),
            }
            let outcome = run(&harness, &forged, "worker-exec-1").await;
            let disposition = outcome.expect("a divergent copy fails closed deterministically");
            assert_eq!(
                disposition,
                JobRunDisposition::Executed(JobExecutionDisposition::FailedPermanently {
                    code: failure_codes::TRANSPORT_INTEGRITY.to_owned()
                }),
                "{mutate} mismatch must fail closed"
            );
            let record = harness
                .store
                .get(envelope.job_id)
                .await
                .unwrap()
                .expect("present");
            assert_ne!(
                record.status,
                JobStatus::Succeeded,
                "{mutate} never executes"
            );
            assert_eq!(
                record.envelope.payload, envelope.payload,
                "{mutate} cannot override the durable payload"
            );
        }
    }

    #[tokio::test]
    async fn missing_durable_row_never_falls_back_to_inline_execution() {
        let harness = services();
        let forged = envelope();
        let disposition = run(&harness, &forged, "worker-exec-1").await.expect("run");
        assert_eq!(
            disposition,
            JobRunDisposition::Skipped(JobSkipReason::Missing),
            "a durable delivery with no durable row must not execute inline"
        );
        assert!(harness.store.records().is_empty());
    }

    #[tokio::test]
    async fn stale_claims_cannot_mutate_newer_claims() {
        let harness = services();
        let envelope = envelope();
        harness
            .services
            .submit_durable(envelope.clone())
            .await
            .expect("submit");
        let store = harness.services.store.clone();
        let start = chrono::Utc::now();
        let stale = store
            .claim_execution(
                envelope.job_id,
                "same-worker-name",
                start + TimeDelta::minutes(1),
                start,
            )
            .await
            .expect("stale claim")
            .expect("claimed");
        // The lease expires and a newer claimant (same worker name) starts.
        harness.clock.set(start + TimeDelta::minutes(2));
        let newer = store
            .claim_execution(
                envelope.job_id,
                "same-worker-name",
                start + TimeDelta::minutes(30),
                start + TimeDelta::minutes(2),
            )
            .await
            .expect("newer claim")
            .expect("reclaimed");
        assert_ne!(stale.lease_id, newer.lease_id);
        let error = store
            .complete(&stale, start + TimeDelta::minutes(3))
            .await
            .unwrap_err();
        assert!(matches!(error, JobError::LeaseFencedOut { .. }));
        let error = store
            .schedule_retry_and_publish(
                &stale,
                "stale-code",
                start + TimeDelta::minutes(4),
                start + TimeDelta::minutes(3),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, JobError::LeaseFencedOut { .. }));
        let error = store
            .fail_permanently(&stale, "stale-code", start + TimeDelta::minutes(3))
            .await
            .unwrap_err();
        assert!(matches!(error, JobError::LeaseFencedOut { .. }));
        store
            .complete(&newer, start + TimeDelta::minutes(3))
            .await
            .expect("the newer claim still owns the job");
    }

    #[tokio::test]
    async fn retry_state_and_next_publication_generation_commit_together() {
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
        let (base, store, _dispatcher) = JobsServices::memory(registry);
        let clock = Arc::new(FakeJobClock::starting(chrono::Utc::now()));
        let services = JobsServices {
            clock: clock.clone(),
            ..base
        };
        let harness = Harness {
            services: services.clone(),
            store: store.clone(),
            dispatcher: Arc::new(FakeJobDispatcher::new()),
            clock: clock.clone(),
        };
        let envelope = envelope().with(JobOptions::default().with_retry(RetryPolicy::fixed(2, 60)));
        services
            .submit_durable(envelope.clone())
            .await
            .expect("submit");
        clock.advance(TimeDelta::milliseconds(50));
        let delivered = services
            .dispatch_due_once("dispatch-1", 10, TimeDelta::minutes(1))
            .await
            .expect("deliver generation 1");
        assert_eq!(delivered.dispatched, 1);
        let first = run(&harness, &envelope, "worker-exec-1")
            .await
            .expect("first run");
        assert!(matches!(
            first,
            JobRunDisposition::Executed(JobExecutionDisposition::RetryScheduled { .. })
        ));
        // The retry transition and the next publication generation are one
        // atomic unit: generation 2 exists, pending, at the retry time.
        let publications = store.publication_records();
        assert_eq!(publications.len(), 2, "generation 2 was inserted");
        assert_eq!(
            publications
                .iter()
                .map(|p| p.generation)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(publications[1].status, PublicationStatus::Pending);
        assert_ne!(
            publications[1].publication_id, publications[0].publication_id,
            "a new retry generation has a new publication identity"
        );
        // The next dispatch round claims only the new generation.
        clock.advance(TimeDelta::milliseconds(50));
        let report = services
            .dispatch_due_once("dispatch-2", 10, TimeDelta::minutes(1))
            .await
            .expect("dispatch before due time");
        assert_eq!(report.claimed, 0);
        clock.advance(TimeDelta::seconds(61));
        let report = services
            .dispatch_due_once("dispatch-2", 10, TimeDelta::minutes(1))
            .await
            .expect("dispatch at due time");
        assert_eq!(report.dispatched, 1);
    }

    #[tokio::test]
    async fn duplicate_submission_uses_the_semantic_fingerprint() {
        let harness = services();
        let options = JobOptions::default().with_dedupe_key("orders.send-confirmation:o-1");
        harness
            .services
            .submit_durable(envelope().with(options.clone()))
            .await
            .expect("first");
        match harness
            .services
            .submit_durable(envelope().with(options.clone()))
            .await
            .expect("semantically identical resubmission is idempotent")
        {
            DurableSubmission::Duplicate(_) => {}
            DurableSubmission::Inserted(inserted) => {
                panic!("expected duplicate, got insertion {inserted}")
            }
        }
        let mut conflicting = envelope();
        conflicting.payload = serde_json::json!({ "order_id": "o-2" });
        let error = harness
            .services
            .submit_durable(conflicting.with(options))
            .await
            .expect_err("same key, different semantics fails closed");
        assert!(matches!(
            error,
            JobError::DuplicateSubmissionConflict { .. }
        ));
    }

    #[tokio::test]
    async fn memory_store_reports_duplicate_identity_as_a_duplicate_not_as_missing() {
        let harness = services();
        let first = envelope();
        harness
            .services
            .submit_durable(first.clone())
            .await
            .expect("first");
        // Reusing an explicit job identity with different semantics is an
        // identity collision; the reference store must not report the job
        // as missing when it demonstrably exists.
        let mut conflicting = envelope();
        conflicting.payload = serde_json::json!({ "order_id": "o-other" });
        conflicting.job_id = first.job_id;
        let error = harness
            .store
            .enqueue_with_intent(pending_record(conflicting))
            .await
            .expect_err("identity collision fails closed");
        assert!(
            matches!(error, JobError::DuplicateJobIdentity(id) if id == first.job_id),
            "expected a duplicate-identity error, got {error:?}"
        );
        assert!(
            harness
                .store
                .get(first.job_id)
                .await
                .expect("get")
                .is_some(),
            "the original job still exists"
        );
    }

    #[tokio::test]
    async fn ingest_never_overwrites_an_existing_job_identity() {
        let harness = services();
        let first = envelope();
        harness
            .services
            .submit_durable(first.clone())
            .await
            .expect("first");
        // Ingesting a delivery that reuses an existing identity without a
        // matching dedupe key must fail closed, never replace the record.
        let mut conflicting = envelope();
        conflicting.payload = serde_json::json!({ "order_id": "o-other" });
        conflicting.job_id = first.job_id;
        conflicting.dedupe_key = None;
        let error = harness
            .store
            .ingest_existing_delivery(pending_record(conflicting))
            .await
            .expect_err("identity collision on ingest fails closed");
        assert!(
            matches!(error, JobError::DuplicateJobIdentity(id) if id == first.job_id),
            "expected a duplicate-identity error, got {error:?}"
        );
        let surviving = harness
            .store
            .get(first.job_id)
            .await
            .expect("get")
            .expect("the original job still exists");
        assert_eq!(
            surviving.envelope.payload, first.payload,
            "the original record was not overwritten"
        );
    }

    #[tokio::test]
    async fn overlap_locks_release_on_every_exit_path() {
        let registry = Arc::new(JobHandlerRegistry::new());
        registry
            .register_typed::<Confirmation, _, _>(|_: Confirmation, _| async {
                Err(JobExecutionFailure::retryable("always-busy"))
            })
            .expect("register");
        let (base, store, _dispatcher) = JobsServices::memory(registry);
        let clock = Arc::new(FakeJobClock::starting(chrono::Utc::now()));
        let services = JobsServices {
            clock: clock.clone(),
            ..base
        };
        let harness = Harness {
            services: services.clone(),
            store: store.clone(),
            dispatcher: Arc::new(FakeJobDispatcher::new()),
            clock: clock.clone(),
        };
        let options = JobOptions::default()
            .with_retry(RetryPolicy::fixed(5, 1))
            .with_overlap_key("orders.confirm:o-1");
        let envelope = envelope().with(options);
        services
            .submit_durable(envelope.clone())
            .await
            .expect("submit");
        let first = run(&harness, &envelope, "worker-exec-1")
            .await
            .expect("run");
        assert!(matches!(
            first,
            JobRunDisposition::Executed(JobExecutionDisposition::RetryScheduled { .. })
        ));
        // The overlap lock was released before the retry, so the next
        // attempt is not blocked until TTL.
        clock.advance(TimeDelta::seconds(2));
        let second = run(&harness, &envelope, "worker-exec-2")
            .await
            .expect("run");
        assert!(
            matches!(
                second,
                JobRunDisposition::Executed(JobExecutionDisposition::RetryScheduled { .. })
            ),
            "the freed overlap boundary admits the retry: {second:?}"
        );
    }

    #[tokio::test]
    async fn profile_routing_fails_closed_for_unknown_profiles() {
        let mut routed = ProfileRoutedDispatcher::new();
        let sink = Arc::new(FakeJobDispatcher::new());
        routed
            .route("orders-notifications", sink.clone())
            .expect("route");
        let envelope = envelope();
        let delivery = JobDelivery {
            envelope: envelope.clone(),
            publication_id: uuid::Uuid::now_v7(),
        };
        routed
            .dispatch(&delivery, chrono::Utc::now())
            .await
            .expect("known profile dispatches");
        let mut foreign = envelope;
        foreign.worker_profile = "other-profile".into();
        let foreign_delivery = JobDelivery {
            envelope: foreign,
            publication_id: uuid::Uuid::now_v7(),
        };
        let error = routed
            .dispatch(&foreign_delivery, chrono::Utc::now())
            .await
            .expect_err("unknown profile fails before provider contact");
        assert!(matches!(error, JobError::UnknownWorkerProfile(_)));
        assert_eq!(sink.dispatched().len(), 1);
    }

    #[tokio::test]
    async fn rejected_duplicate_route_registration_preserves_the_original_route() {
        let mut routed = ProfileRoutedDispatcher::new();
        let original = Arc::new(FakeJobDispatcher::new());
        routed
            .route("orders-notifications", original.clone())
            .expect("route");
        // A rejected duplicate registration must not replace the bound
        // dispatcher: the routing table stays exactly as it was.
        let replacement = Arc::new(FakeJobDispatcher::new());
        let error = routed
            .route("orders-notifications", replacement.clone())
            .expect_err("duplicate registration fails closed");
        assert!(matches!(error, JobError::InvalidJob(_)));
        let envelope = envelope();
        let delivery = JobDelivery {
            envelope,
            publication_id: uuid::Uuid::now_v7(),
        };
        routed
            .dispatch(&delivery, chrono::Utc::now())
            .await
            .expect("dispatch still succeeds");
        assert_eq!(
            original.dispatched().len(),
            1,
            "the delivery reaches the original dispatcher"
        );
        assert!(
            replacement.dispatched().is_empty(),
            "the rejected replacement never receives a delivery"
        );
    }

    #[tokio::test]
    async fn scheduler_ingestion_creates_no_pending_publication() {
        let harness = services();
        let mut occurrence = envelope();
        occurrence.dedupe_key = Some("orders-nightly:2026-08-22T13:00:00Z".into());
        match harness
            .store
            .ingest_existing_delivery(pending_record(occurrence.clone()))
            .await
            .expect("ingest")
        {
            IngestOutcome::Ingested(job_id) => assert_eq!(job_id, occurrence.job_id),
            IngestOutcome::Duplicate(existing) => panic!("first occurrence, got {existing}"),
        }
        let publications = harness.store.publication_records();
        assert_eq!(publications.len(), 1);
        assert_eq!(
            publications[0].status,
            PublicationStatus::Published,
            "ingestion records the existing delivery, never a pending one"
        );
        match harness
            .store
            .ingest_existing_delivery(pending_record(occurrence.clone()))
            .await
            .expect("re-ingest")
        {
            IngestOutcome::Duplicate(existing) => assert_eq!(existing, occurrence.job_id),
            IngestOutcome::Ingested(job_id) => panic!("duplicate occurrence, got {job_id}"),
        }
        assert_eq!(
            harness.store.publication_records().len(),
            1,
            "no orphan publication appears"
        );
    }

    #[tokio::test]
    async fn queued_and_inline_modes_are_explicit() {
        let harness = services();
        harness
            .services
            .submit_inline(envelope())
            .await
            .expect("inline");
        harness
            .services
            .submit_queued(envelope())
            .await
            .expect("queued");
        assert_eq!(harness.dispatcher.dispatched().len(), 1);
    }

    #[tokio::test]
    async fn deadline_and_unknown_job_behaviour() {
        let harness = services();
        let now = harness.clock.now();
        let expiring =
            envelope().with(JobOptions::default().with_deadline(now + TimeDelta::seconds(10)));
        harness
            .services
            .submit_durable(expiring.clone())
            .await
            .expect("submit");
        let disposition = run(&harness, &expiring, "worker-exec-1")
            .await
            .expect("run");
        assert_eq!(
            disposition,
            JobRunDisposition::Executed(JobExecutionDisposition::Succeeded),
            "a live deadline admits execution"
        );

        let registry = Arc::new(JobHandlerRegistry::new());
        let (base, store, _dispatcher) = JobsServices::memory(registry);
        let unknown = JobEnvelope::for_parts(
            "orders.missing",
            1,
            serde_json::json!({ "order_id": "o-1" }),
            "orders-notifications",
            uuid::Uuid::now_v7(),
        )
        .unwrap();
        base.submit_durable(unknown.clone()).await.unwrap();
        let disposition = base
            .executor
            .run(
                &unknown,
                "worker-exec-1",
                base.clock.as_ref(),
                base.store.as_ref(),
                base.publications.as_ref(),
                base.locks.as_ref(),
            )
            .await
            .unwrap();
        assert_eq!(
            disposition,
            JobRunDisposition::Executed(JobExecutionDisposition::FailedPermanently {
                code: failure_codes::UNKNOWN_JOB.to_owned()
            })
        );
        let _ = store;
    }

    #[tokio::test]
    async fn attempt_history_stays_ordered_and_bounded() {
        let registry = Arc::new(JobHandlerRegistry::new());
        registry
            .register_typed::<Confirmation, _, _>(|_: Confirmation, _| async {
                Err(JobExecutionFailure::retryable("always-busy"))
            })
            .expect("register");
        let (base, store, _dispatcher) = JobsServices::memory(registry);
        let clock = Arc::new(FakeJobClock::starting(chrono::Utc::now()));
        let services = JobsServices {
            clock: clock.clone(),
            ..base
        };
        let harness = Harness {
            services: services.clone(),
            store: store.clone(),
            dispatcher: Arc::new(FakeJobDispatcher::new()),
            clock: clock.clone(),
        };
        let envelope = envelope().with(JobOptions::default().with_retry(RetryPolicy::fixed(60, 1)));
        services
            .submit_durable(envelope.clone())
            .await
            .expect("submit");
        for round in 0..30 {
            run(&harness, &envelope, &format!("worker-exec-{round}"))
                .await
                .expect("run");
            clock.advance(TimeDelta::seconds(2));
        }
        let record = services.store.get(envelope.job_id).await.unwrap().unwrap();
        assert_eq!(record.attempts.len(), MAX_ATTEMPT_HISTORY);
        assert!(
            record.attempts.first().expect("oldest").attempt
                < record.attempts.last().expect("newest").attempt
        );
        assert_eq!(record.attempt_count, 30);
    }
}
