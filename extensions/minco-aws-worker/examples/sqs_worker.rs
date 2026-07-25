use async_trait::async_trait;
use minco_aws_worker::{
    MessageHandler, WorkerConfig, WorkerFailure, WorkerMessage, run_sqs_worker,
};
use std::sync::Arc;

#[derive(Debug)]
struct ExampleHandler;

#[async_trait]
impl MessageHandler for ExampleHandler {
    async fn handle(&self, message: WorkerMessage) -> Result<(), WorkerFailure> {
        tracing::info!(message_id = message.message_id, "processed SQS message");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), lambda_runtime::Error> {
    run_sqs_worker(Arc::new(ExampleHandler), WorkerConfig::default()).await
}
