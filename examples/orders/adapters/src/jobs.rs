//! Order confirmation jobs: the durable dispatch bound to `placeOrder`.
//!
//! The adapter layer owns the envelope translation because the application
//! crate stays free of plugin dependencies. The job is enqueued inside the
//! same transaction as the order mutation, so both commit or neither does.

use orders_application::OrderConfirmationJob;
use uuid::Uuid;

use minco_plugin_jobs::{JobEnvelope, JobOptions, RetryPolicy};

/// Stable logical name of the confirmation job.
pub const CONFIRMATION_JOB_NAME: &str = "orders.send-confirmation";
/// Current confirmation payload version.
pub const CONFIRMATION_JOB_VERSION: u16 = 1;
/// Worker profile that owns confirmation delivery.
pub const CONFIRMATION_WORKER_PROFILE: &str = "orders-notifications";

#[derive(Debug, thiserror::Error)]
#[error("confirmation job translation failed: {0}")]
pub struct ConfirmationJobError(String);

/// Build the durable confirmation envelope for one placed order.
///
/// The correlation identity threads the originating request through
/// delivery, and the order identity is the overlap boundary so two
/// confirmations for one order never execute concurrently.
pub fn confirmation_envelope(
    job: &OrderConfirmationJob,
) -> Result<JobEnvelope, ConfirmationJobError> {
    JobEnvelope::for_parts(
        CONFIRMATION_JOB_NAME,
        CONFIRMATION_JOB_VERSION,
        serde_json::json!({ "order_id": job.order_id.into_uuid().to_string() }),
        CONFIRMATION_WORKER_PROFILE,
        Uuid::now_v7(),
    )
    .map_err(|error| ConfirmationJobError(error.to_string()))
    .map(|envelope| {
        envelope.with(
            JobOptions::default()
                .with_causation(job.correlation_id)
                .with_retry(RetryPolicy::exponential(5, 30, 900))
                .with_overlap_key(format!("orders.confirm:{}", job.order_id.into_uuid())),
        )
    })
}

/// The typed confirmation payload carried by [`CONFIRMATION_JOB_NAME`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SendOrderConfirmation {
    pub order_id: String,
}

impl minco_plugin_jobs::Job for SendOrderConfirmation {
    const NAME: &'static str = CONFIRMATION_JOB_NAME;
    const VERSION: u16 = CONFIRMATION_JOB_VERSION;
}

/// Application-owned destination for one confirmation effect. Production
/// composition binds the notifications plugin; tests bind a deterministic
/// recording sink.
#[async_trait::async_trait]
pub trait ConfirmationSink: Send + Sync + std::fmt::Debug {
    async fn send_confirmation(&self, confirmation: &SendOrderConfirmation) -> Result<(), String>;
}

/// Build the explicit handler registry for the confirmation job. The sink
/// decides retryability: a `ConfirmationSink` error is retryable so the
/// durable retry policy governs redelivery.
pub fn confirmation_registry(
    sink: std::sync::Arc<dyn ConfirmationSink>,
) -> minco_plugin_jobs::JobHandlerRegistry {
    let registry = minco_plugin_jobs::JobHandlerRegistry::new();
    registry
        .register_typed::<SendOrderConfirmation, _, _>(move |job: SendOrderConfirmation, _| {
            let sink = sink.clone();
            async move {
                sink.send_confirmation(&job)
                    .await
                    .map_err(minco_plugin_jobs::JobExecutionFailure::retryable)
            }
        })
        .expect("register the confirmation handler once");
    registry
}
