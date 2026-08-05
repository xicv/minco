#![forbid(unsafe_code)]

use lambda_runtime::{Error, LambdaEvent, service_fn};
use minco_aws_adapters::appsync_events::AppSyncEventsPublisher;
use minco_plugin_realtime::RealtimePublisher;
use minco_realtime_appsync_live_proof::publish_request;
use serde_json::Value;
use std::{env, sync::Arc, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();

    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let credentials = config
        .credentials_provider()
        .ok_or("AWS credentials provider is unavailable")?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let publisher = AppSyncEventsPublisher::new(
        client,
        credentials,
        env::var("MINCO_REALTIME_HTTP_ENDPOINT")?,
        env::var("MINCO_REALTIME_NAMESPACE")?,
        env::var("AWS_REGION")?,
    )?;
    let publisher: Arc<dyn RealtimePublisher> = Arc::new(publisher);

    lambda_runtime::run(service_fn(move |event: LambdaEvent<Value>| {
        let publisher = publisher.clone();
        async move {
            let result = publish_request(publisher, event.payload).await?;
            tracing::info!(outcome = "accepted", "live proof publication completed");
            Ok::<Value, Error>(result)
        }
    }))
    .await
}
