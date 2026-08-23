//! The durable-jobs Lambda worker for the Orders example.
//!
//! Runs the existing SQS worker runtime with the typed jobs adapter in
//! durable mode: every delivery claims an execution lease in the orders
//! database, duplicate deliveries of terminal jobs are acknowledged without
//! re-execution, and retries and permanent failures are persisted before
//! acknowledgement. The worker never schedules anything; `EventBridge
//! Scheduler` deliveries arrive through the same queue as manual dispatch.

use anyhow::{Context as _, Result};
use std::sync::Arc;

use minco_aws_worker::{WorkerConfig, run_sqs_worker};
use minco_plugin_jobs::JobsServices;
use orders_service::AppConfig;

/// Confirmation effects flow through the composition's notification
/// binding; the default worker logs the bounded effect without payload
/// content.
#[derive(Debug, Default)]
struct LoggedConfirmationSink;

#[async_trait::async_trait]
impl orders_adapters::jobs::ConfirmationSink for LoggedConfirmationSink {
    async fn send_confirmation(
        &self,
        confirmation: &orders_adapters::jobs::SendOrderConfirmation,
    ) -> Result<(), String> {
        tracing::info!(order_id = %confirmation.order_id, "order confirmation delivered");
        Ok(())
    }
}

fn confirmation_registry() -> Arc<minco_plugin_jobs::JobHandlerRegistry> {
    let sink: Arc<dyn orders_adapters::jobs::ConfirmationSink> = Arc::new(LoggedConfirmationSink);
    Arc::new(orders_adapters::jobs::confirmation_registry(sink))
}

/// The dispatcher bound to the worker. When `JOBS_QUEUE_URL` is set the
/// worker can republish due retry intents through SQS explicitly; without
/// it the dispatcher fails closed so no delivery is silently dropped into
/// inline execution.
#[cfg(feature = "jobs-sqs")]
async fn dispatcher() -> Result<Arc<dyn minco_plugin_jobs::JobDispatcher>> {
    let Ok(queue_url) = std::env::var("JOBS_QUEUE_URL") else {
        return Ok(Arc::new(minco_plugin_jobs::FailClosedDispatcher));
    };
    let fifo = queue_url.as_str().to_ascii_lowercase().ends_with(".fifo");
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    Ok(Arc::new(
        minco_aws_adapters::jobs_sqs::SqsJobDispatcher::new(
            aws_sdk_sqs::Client::new(&config),
            queue_url,
            fifo,
        )
        .context("validate the jobs queue target")?,
    ))
}

#[cfg(not(feature = "jobs-sqs"))]
async fn dispatcher() -> Result<Arc<dyn minco_plugin_jobs::JobDispatcher>> {
    Ok(Arc::new(minco_plugin_jobs::FailClosedDispatcher))
}

#[cfg(feature = "sqlite")]
async fn jobs_services(config: &AppConfig) -> Result<JobsServices> {
    let pool = minco_sqlx_sqlite::connect(&minco_sqlx_sqlite::SqlitePoolConfig::file(
        &config.sqlite_path,
    ))
    .await
    .context("connect the SQLite jobs store")?;
    let store = Arc::new(minco_sqlx_sqlite::jobs::SqliteJobStore::new(pool));
    Ok(JobsServices::new(
        store.clone(),
        store.clone(),
        dispatcher().await?,
        store.clone(),
        Arc::new(minco_plugin_jobs::SystemJobClock),
        Arc::new(minco_plugin_jobs::JobExecutor::new(confirmation_registry())),
    ))
}

#[cfg(all(feature = "postgres", not(feature = "sqlite")))]
async fn jobs_services(config: &AppConfig) -> Result<JobsServices> {
    let url = std::env::var("DATABASE_URL").context("DATABASE_URL is required for PostgreSQL")?;
    let pool = minco_sqlx_postgres::PgPool::connect(&url)
        .await
        .context("connect the PostgreSQL jobs store")?;
    let store = Arc::new(minco_sqlx_postgres::jobs::PostgresJobStore::new(pool));
    Ok(JobsServices::new(
        store.clone(),
        store.clone(),
        dispatcher().await?,
        store.clone(),
        Arc::new(minco_plugin_jobs::SystemJobClock),
        Arc::new(minco_plugin_jobs::JobExecutor::new(confirmation_registry())),
    ))
}

#[cfg(not(any(feature = "sqlite", feature = "postgres")))]
async fn jobs_services(_config: &AppConfig) -> Result<JobsServices> {
    anyhow::bail!("the jobs worker requires the sqlite or postgres feature")
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_env()?;
    let worker_id = std::env::var("JOBS_WORKER_ID").unwrap_or_else(|_| "orders-jobs-worker".into());
    let services = jobs_services(&config).await?;
    let handler = Arc::new(minco_aws_worker::jobs::JobMessageHandler::durable(
        worker_id, services,
    ));
    let config = WorkerConfig {
        max_batch_size: 10,
        max_message_bytes: 262_144,
        max_concurrency: 2,
    };
    config.validate().context("invalid jobs worker config")?;
    run_sqs_worker(handler, config)
        .await
        .map_err(|error| anyhow::anyhow!("SQS jobs worker failed: {error}"))
}
