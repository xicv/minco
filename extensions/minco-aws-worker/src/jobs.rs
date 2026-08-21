//! Typed job execution path for the SQS worker.
//!
//! The adapter decodes a job envelope (or a Scheduler `scheduled_trigger`
//! delivery, from which it mints a fresh durable job), resolves the
//! registered handler and runs it through the lease-based executor. Durable
//! envelopes are claimed, deduplicated, retried and permanently failed in
//! the store before the delivery is acknowledged; queued envelopes execute
//! inline and rely on the queue's redelivery and dead-letter policy. Every
//! existing batch, ordering and partial-batch behaviour of
//! [`crate::process_sqs_event`] is preserved.

use std::sync::Arc;

use minco_plugin_jobs::{
    JobEnvelope, JobError, JobExecutionFailure, JobExecutor, JobRunDisposition, JobSkipReason,
    JobsServices, failure_codes,
};

use crate::{MessageHandler, WorkerFailure, WorkerMessage};

/// Stable worker failure codes surfaced for transport redelivery.
pub mod worker_failure_codes {
    pub const INVALID_ENVELOPE: &str = "jobs-invalid-envelope";
    pub const STORE_UNAVAILABLE: &str = "jobs-store-unavailable";
    pub const QUEUED_FAILURE: &str = "jobs-queued-failure";
    pub const INVALID_SCHEDULED_TRIGGER: &str = "jobs-invalid-scheduled-trigger";
}

/// A Scheduler `scheduled_trigger` delivery.
///
/// The two context attributes are substituted by `EventBridge Scheduler`
/// on every invocation, so each recurrence carries a fresh `execution_id`
/// and no static job identity exists.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduledJobTrigger {
    pub schema_version: u16,
    pub kind: String,
    pub schedule_id: String,
    pub job_name: String,
    pub job_version: u16,
    pub worker_profile: String,
    pub payload: serde_json::Value,
    pub execution_id: String,
    pub scheduled_time: String,
}

impl ScheduledJobTrigger {
    fn validate(&self) -> Result<(), JobError> {
        if self.schema_version != 1 || self.kind != "scheduled_trigger" {
            return Err(JobError::InvalidTransportMessage(
                "scheduled trigger must use schema 1 and the scheduled_trigger kind".into(),
            ));
        }
        if self.schedule_id.trim().is_empty()
            || self.job_name.trim().is_empty()
            || self.execution_id.trim().is_empty()
            || self.scheduled_time.trim().is_empty()
        {
            return Err(JobError::InvalidTransportMessage(
                "scheduled trigger requires schedule, job and invocation identities".into(),
            ));
        }
        Ok(())
    }

    /// Mint the durable job for this recurrence: a fresh identity, deduped
    /// on `schedule_id:execution_id` so a Scheduler retry of the same
    /// invocation cannot execute twice.
    fn into_envelope(self) -> Result<JobEnvelope, JobError> {
        self.validate()?;
        let correlation = uuid::Uuid::now_v7();
        let mut envelope = JobEnvelope::for_parts(
            self.job_name,
            self.job_version,
            self.payload,
            self.worker_profile,
            correlation,
        )?;
        envelope.metadata.insert(
            "schedule-id".into(),
            self.schedule_id.chars().take(64).collect(),
        );
        envelope.metadata.insert(
            "scheduled-time".into(),
            self.scheduled_time.chars().take(64).collect(),
        );
        envelope.dedupe_key = Some(format!("{}:{}", self.schedule_id, self.execution_id));
        Ok(envelope)
    }
}

enum TransportJob {
    Envelope(JobEnvelope),
    Scheduled(ScheduledJobTrigger),
}

fn decode_transport_body(body: &str) -> Result<TransportJob, JobError> {
    let peek: serde_json::Value = serde_json::from_str(body).map_err(|error| {
        JobError::InvalidTransportMessage(format!("body decode failed: {error}"))
    })?;
    if peek.get("kind").and_then(serde_json::Value::as_str) == Some("scheduled_trigger") {
        let trigger: ScheduledJobTrigger = serde_json::from_value(peek)
            .map_err(|error| JobError::InvalidTransportMessage(format!("{error}")))?;
        return Ok(TransportJob::Scheduled(trigger));
    }
    JobEnvelope::from_json_bytes(body.as_bytes()).map(TransportJob::Envelope)
}

/// A [`MessageHandler`] executing typed job envelopes.
#[derive(Clone)]
pub struct JobMessageHandler {
    worker_id: String,
    durable: Option<JobsServices>,
    executor: Arc<JobExecutor>,
}

impl std::fmt::Debug for JobMessageHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobMessageHandler")
            .field("worker_id", &self.worker_id)
            .field("durable", &self.durable.is_some())
            .field("executor", &self.executor)
            .finish_non_exhaustive()
    }
}

impl JobMessageHandler {
    /// A durable worker: deliveries claim execution leases in the store,
    /// duplicate deliveries of terminal jobs are acknowledged without
    /// re-execution, and retries and permanent failures are persisted before
    /// acknowledgement.
    #[must_use]
    pub fn durable(worker_id: impl Into<String>, services: JobsServices) -> Self {
        Self {
            worker_id: worker_id.into(),
            executor: services.executor.clone(),
            durable: Some(services),
        }
    }

    /// A queued worker: envelopes execute inline with no durable state; any
    /// handler failure returns the message to the queue, whose redrive
    /// policy owns retries and the dead-letter path.
    #[must_use]
    pub fn queued(worker_id: impl Into<String>, executor: Arc<JobExecutor>) -> Self {
        Self {
            worker_id: worker_id.into(),
            executor,
            durable: None,
        }
    }

    fn inline_failure(failure: &JobExecutionFailure) -> WorkerFailure {
        WorkerFailure::new(format!(
            "{}-{}",
            worker_failure_codes::QUEUED_FAILURE,
            failure.code()
        ))
    }
}

#[async_trait::async_trait]
impl MessageHandler for JobMessageHandler {
    async fn handle(&self, message: WorkerMessage) -> Result<(), WorkerFailure> {
        let transport = decode_transport_body(&message.body).map_err(|error| {
            tracing::warn!(
                message_id = %message.message_id,
                code = worker_failure_codes::INVALID_ENVELOPE,
                detail = error.stable_code(),
                "job transport message is not a valid envelope; queue redrive owns it"
            );
            WorkerFailure::new(worker_failure_codes::INVALID_ENVELOPE)
        })?;
        let envelope = match transport {
            TransportJob::Envelope(envelope) => envelope,
            TransportJob::Scheduled(trigger) => {
                let envelope = trigger.into_envelope().map_err(|error| {
                    tracing::warn!(
                        message_id = %message.message_id,
                        code = worker_failure_codes::INVALID_SCHEDULED_TRIGGER,
                        detail = error.stable_code(),
                        "scheduled trigger is invalid; queue redrive owns it"
                    );
                    WorkerFailure::new(worker_failure_codes::INVALID_SCHEDULED_TRIGGER)
                })?;
                // A durable worker records the minted job so the store's
                // dedupe key neutralizes Scheduler retries of the same
                // invocation; a queued worker executes inline without one.
                if let Some(services) = &self.durable {
                    let minted = envelope.clone();
                    match services.submit_durable(minted).await {
                        Ok(minco_plugin_jobs::DurableSubmission::Inserted(_)) => envelope,
                        // The dedupe key already exists: continue with the
                        // existing job's identity so its durable state
                        // governs duplicate suppression.
                        Ok(minco_plugin_jobs::DurableSubmission::Duplicate(existing)) => {
                            match services.store.get(existing).await {
                                Ok(Some(record)) => record.envelope,
                                Ok(None) | Err(JobError::Infrastructure(_)) => {
                                    return Err(WorkerFailure::new(
                                        worker_failure_codes::STORE_UNAVAILABLE,
                                    ));
                                }
                                Err(error) => {
                                    return Err(WorkerFailure::new(
                                        error.stable_code().to_lowercase(),
                                    ));
                                }
                            }
                        }
                        Err(JobError::Infrastructure(_)) => {
                            return Err(WorkerFailure::new(
                                worker_failure_codes::STORE_UNAVAILABLE,
                            ));
                        }
                        Err(error) => {
                            return Err(WorkerFailure::new(error.stable_code().to_lowercase()));
                        }
                    }
                } else {
                    envelope
                }
            }
        };
        let now = chrono::Utc::now();
        let Some(services) = &self.durable else {
            return self
                .executor
                .run_inline(&envelope, now)
                .await
                .map_err(|failure| Self::inline_failure(&failure));
        };
        match self
            .executor
            .run(
                &envelope,
                &self.worker_id,
                now,
                services.store.as_ref(),
                services.publications.as_ref(),
                services.locks.as_ref(),
            )
            .await
        {
            // Every executed disposition is already durably recorded, and
            // terminal, live-leased or not-yet-available duplicates are
            // acknowledged without re-execution; the queue stops
            // redelivering either way.
            Ok(
                JobRunDisposition::Executed(_)
                | JobRunDisposition::Skipped(
                    JobSkipReason::NotExecutable(_) | JobSkipReason::NotYetAvailable,
                ),
            ) => Ok(()),
            Ok(JobRunDisposition::Skipped(JobSkipReason::Missing)) => {
                // The envelope has no durable row here: execute it inline so
                // a queued submission is never silently dropped. Effects
                // must be idempotent under at-least-once delivery.
                self.executor
                    .run_inline(&envelope, now)
                    .await
                    .map_err(|failure| Self::inline_failure(&failure))
            }
            Err(JobError::Infrastructure(_)) => {
                tracing::warn!(
                    job_id = %envelope.job_id,
                    code = worker_failure_codes::STORE_UNAVAILABLE,
                    "durable job transition failed; returning the delivery to the queue"
                );
                Err(WorkerFailure::new(worker_failure_codes::STORE_UNAVAILABLE))
            }
            Err(error) => {
                tracing::warn!(
                    job_id = %envelope.job_id,
                    code = error.stable_code(),
                    "job delivery failed deterministically; acknowledging the poison disposition"
                );
                let _ = failure_codes::PAYLOAD_DECODE;
                Err(WorkerFailure::new(error.stable_code().to_lowercase()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_plugin_jobs::{
        Job, JobExecutionFailure as Failure, JobHandlerRegistry, JobOptions, JobStatus, RetryPolicy,
    };
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    #[derive(serde::Serialize, serde::Deserialize)]
    struct Confirmation {
        order_id: String,
    }

    impl Job for Confirmation {
        const NAME: &'static str = "orders.send-confirmation";
        const VERSION: u16 = 1;
    }

    fn registry<F>(handler: F) -> Arc<JobHandlerRegistry>
    where
        F: Fn(
                Confirmation,
                minco_plugin_jobs::JobContext,
            )
                -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), Failure>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        let registry = Arc::new(JobHandlerRegistry::new());
        registry
            .register_typed::<Confirmation, _, _>(handler)
            .expect("register");
        registry
    }

    fn envelope_bytes(payload: serde_json::Value) -> String {
        let envelope = minco_plugin_jobs::JobEnvelope::for_parts(
            "orders.send-confirmation",
            1,
            payload,
            "orders-notifications",
            uuid::Uuid::now_v7(),
        )
        .expect("valid envelope");
        String::from_utf8(envelope.to_json_bytes().expect("serialize")).expect("utf-8")
    }

    fn message(body: String) -> WorkerMessage {
        WorkerMessage {
            message_id: "m-1".into(),
            body,
            attributes: BTreeMap::new(),
            message_group_id: None,
        }
    }

    use std::collections::BTreeMap;

    #[tokio::test]
    async fn durable_delivery_executes_and_acknowledges() {
        let ran = Arc::new(AtomicBool::new(false));
        let flag = ran.clone();
        let registry = registry(move |job: Confirmation, _| {
            let flag = flag.clone();
            Box::pin(async move {
                assert_eq!(job.order_id, "o-1");
                flag.store(true, Ordering::SeqCst);
                Ok(())
            })
        });
        let services = minco_plugin_jobs::JobsServices::memory(registry).0;
        let envelope = minco_plugin_jobs::JobEnvelope::for_parts(
            "orders.send-confirmation",
            1,
            serde_json::json!({ "order_id": "o-1" }),
            "orders-notifications",
            uuid::Uuid::now_v7(),
        )
        .unwrap();
        services
            .submit_durable(envelope.clone())
            .await
            .expect("submit");
        let handler = JobMessageHandler::durable("worker-1", services.clone());
        let body = String::from_utf8(envelope.to_json_bytes().unwrap()).unwrap();
        handler
            .handle(message(body.clone()))
            .await
            .expect("acknowledged");
        assert!(ran.load(Ordering::SeqCst));
        // A duplicate delivery of the completed job is acknowledged without
        // re-execution.
        handler.handle(message(body)).await.expect("duplicate ack");
        let record = services.store.get(envelope.job_id).await.unwrap().unwrap();
        assert_eq!(record.status, JobStatus::Succeeded);
        assert_eq!(record.attempt_count, 1, "handler ran exactly once");
    }

    #[tokio::test]
    async fn malformed_envelope_follows_the_queue_poison_policy() {
        let registry = Arc::new(JobHandlerRegistry::new());
        let services = minco_plugin_jobs::JobsServices::memory(registry).0;
        let handler = JobMessageHandler::durable("worker-1", services);
        let error = handler
            .handle(message("not-an-envelope".into()))
            .await
            .expect_err("poison");
        assert_eq!(error.code(), worker_failure_codes::INVALID_ENVELOPE);
    }

    #[tokio::test]
    async fn unknown_job_is_persisted_permanent_before_acknowledgement() {
        let registry = Arc::new(JobHandlerRegistry::new());
        let services = minco_plugin_jobs::JobsServices::memory(registry).0;
        let envelope = minco_plugin_jobs::JobEnvelope::for_parts(
            "orders.missing",
            1,
            serde_json::json!({ "order_id": "o-1" }),
            "orders-notifications",
            uuid::Uuid::now_v7(),
        )
        .unwrap();
        services.submit_durable(envelope.clone()).await.unwrap();
        let handler = JobMessageHandler::durable("worker-1", services.clone());
        let body = String::from_utf8(envelope.to_json_bytes().unwrap()).unwrap();
        handler.handle(message(body)).await.expect("acknowledged");
        let record = services.store.get(envelope.job_id).await.unwrap().unwrap();
        assert_eq!(record.status, JobStatus::FailedPermanently);
        assert_eq!(
            record.failure_code.as_deref(),
            Some(failure_codes::UNKNOWN_JOB)
        );
    }

    #[tokio::test]
    async fn transient_failure_is_durably_rescheduled_and_acknowledged() {
        let attempts = Arc::new(AtomicU32::new(0));
        let counter = attempts.clone();
        let registry = registry(move |_: Confirmation, _| {
            let counter = counter.clone();
            Box::pin(async move {
                if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err(Failure::retryable("notification-unavailable"))
                } else {
                    Ok(())
                }
            })
        });
        let services = minco_plugin_jobs::JobsServices::memory(registry).0;
        let envelope = minco_plugin_jobs::JobEnvelope::for_parts(
            "orders.send-confirmation",
            1,
            serde_json::json!({ "order_id": "o-1" }),
            "orders-notifications",
            uuid::Uuid::now_v7(),
        )
        .unwrap()
        .with(JobOptions::default().with_retry(RetryPolicy::fixed(2, 60)));
        services.submit_durable(envelope.clone()).await.unwrap();
        let handler = JobMessageHandler::durable("worker-1", services.clone());
        let body = String::from_utf8(envelope.to_json_bytes().unwrap()).unwrap();
        handler
            .handle(message(body.clone()))
            .await
            .expect("retry is acknowledged");
        let record = services.store.get(envelope.job_id).await.unwrap().unwrap();
        assert_eq!(record.status, JobStatus::Pending, "retry is durable");
        // The redelivered message before the retry time is not executed.
        handler
            .handle(message(body))
            .await
            .expect("early duplicate ack");
        assert_eq!(attempts.load(Ordering::SeqCst), 1, "no premature retry");
    }

    #[tokio::test]
    async fn queued_mode_executes_inline_and_returns_failures_to_the_queue() {
        let ran = Arc::new(AtomicBool::new(false));
        let flag = ran.clone();
        let registry = registry(move |job: Confirmation, _| {
            let flag = flag.clone();
            Box::pin(async move {
                assert_eq!(job.order_id, "o-9");
                flag.store(true, Ordering::SeqCst);
                Ok(())
            })
        });
        let executor = Arc::new(JobExecutor::new(registry));
        let handler = JobMessageHandler::queued("worker-q", executor);
        handler
            .handle(message(envelope_bytes(
                serde_json::json!({ "order_id": "o-9" }),
            )))
            .await
            .expect("inline success");
        assert!(ran.load(Ordering::SeqCst));
        let failing = Arc::new(JobHandlerRegistry::new());
        failing
            .register_typed::<Confirmation, _, _>(|_: Confirmation, _| async {
                Err(Failure::permanent("notify-rejected"))
            })
            .unwrap();
        let queued_handler =
            JobMessageHandler::queued("worker-q", Arc::new(JobExecutor::new(failing)));
        let error = queued_handler
            .handle(message(envelope_bytes(
                serde_json::json!({ "order_id": "o-1" }),
            )))
            .await
            .expect_err("queued failures return to the queue");
        assert!(
            error
                .code()
                .starts_with(worker_failure_codes::QUEUED_FAILURE),
            "{}",
            error.code()
        );
    }

    #[tokio::test]
    async fn standard_batch_reports_only_failed_ids_with_the_jobs_handler() {
        let registry = registry(|_: Confirmation, _| Box::pin(async { Ok(()) }));
        let services = minco_plugin_jobs::JobsServices::memory(registry).0;
        let handler = Arc::new(JobMessageHandler::durable("worker-1", services));
        let mut batch = aws_lambda_events::event::sqs::SqsEvent::default();
        batch.records = vec![
            record(
                "m-1",
                &envelope_bytes(serde_json::json!({ "order_id": "a" })),
            ),
            record("m-2", "poison"),
            record(
                "m-3",
                &envelope_bytes(serde_json::json!({ "order_id": "b" })),
            ),
        ];
        let event = crate::process_sqs_event(batch, handler, crate::WorkerConfig::default())
            .await
            .expect("batch");
        assert_eq!(event.batch_item_failures.len(), 1);
        assert_eq!(event.batch_item_failures[0].item_identifier, "m-2");
    }

    fn record(id: &str, body: &str) -> aws_lambda_events::event::sqs::SqsMessage {
        let mut record = aws_lambda_events::event::sqs::SqsMessage::default();
        record.message_id = Some(id.to_owned());
        record.body = Some(body.to_owned());
        record
    }

    #[tokio::test]
    async fn scheduled_trigger_mints_a_fresh_durable_job_per_recurrence() {
        let runs = Arc::new(AtomicU32::new(0));
        let counter = runs.clone();
        let registry = registry(move |_: Confirmation, _| {
            let counter = counter.clone();
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        });
        let services = minco_plugin_jobs::JobsServices::memory(registry).0;
        let handler = JobMessageHandler::durable("worker-1", services.clone());
        let body = |execution_id: &str| {
            serde_json::json!({
                "schema_version": 1,
                "kind": "scheduled_trigger",
                "schedule_id": "orders-nightly",
                "job_name": "orders.send-confirmation",
                "job_version": 1,
                "worker_profile": "orders-notifications",
                "payload": { "order_id": "o-1" },
                "execution_id": execution_id,
                "scheduled_time": "2026-08-21T13:00:00Z",
            })
            .to_string()
        };
        handler
            .handle(message(body("exec-1")))
            .await
            .expect("first recurrence ack");
        handler
            .handle(message(body("exec-2")))
            .await
            .expect("second recurrence ack");
        assert_eq!(runs.load(Ordering::SeqCst), 2, "each recurrence executes");
        // A Scheduler retry of the SAME invocation is deduped by the store.
        handler
            .handle(message(body("exec-1")))
            .await
            .expect("duplicate invocation ack");
        assert_eq!(
            runs.load(Ordering::SeqCst),
            2,
            "duplicate invocation does not re-execute"
        );
        let store = services.store;
        let failed = store.list_failed(10).await.unwrap();
        assert!(failed.is_empty());
    }

    #[tokio::test]
    async fn malformed_scheduled_trigger_follows_the_poison_policy() {
        let registry = Arc::new(JobHandlerRegistry::new());
        let services = minco_plugin_jobs::JobsServices::memory(registry).0;
        let handler = JobMessageHandler::durable("worker-1", services);
        let error = handler
            .handle(message(
                serde_json::json!({
                    "schema_version": 1,
                    "kind": "scheduled_trigger",
                    "schedule_id": "",
                    "job_name": "orders.send-confirmation",
                    "job_version": 1,
                    "worker_profile": "orders-notifications",
                    "payload": {},
                    "execution_id": "exec-1",
                    "scheduled_time": "2026-08-21T13:00:00Z",
                })
                .to_string(),
            ))
            .await
            .expect_err("invalid trigger");
        assert_eq!(
            error.code(),
            worker_failure_codes::INVALID_SCHEDULED_TRIGGER
        );
    }

    #[tokio::test]
    async fn store_memory_is_shared_not_duplicated() {
        // The durable handler must observe the same store the submitter used,
        // proving the composition shares one `MemoryJobStore`.
        let registry = Arc::new(JobHandlerRegistry::new());
        registry
            .register_typed::<Confirmation, _, _>(|_: Confirmation, _| async { Ok(()) })
            .unwrap();
        let (services, store, _dispatcher) = minco_plugin_jobs::JobsServices::memory(registry);
        let envelope = minco_plugin_jobs::JobEnvelope::for_parts(
            "orders.send-confirmation",
            1,
            serde_json::json!({ "order_id": "o-1" }),
            "orders-notifications",
            uuid::Uuid::now_v7(),
        )
        .unwrap();
        services.submit_durable(envelope.clone()).await.unwrap();
        assert_eq!(store.records().len(), 1);
        let handler = JobMessageHandler::durable("worker-1", services);
        let body = String::from_utf8(envelope.to_json_bytes().unwrap()).unwrap();
        handler.handle(message(body)).await.expect("ack");
        assert_eq!(store.records().len(), 1);
        assert_eq!(store.records()[0].status, JobStatus::Succeeded);
    }
}
