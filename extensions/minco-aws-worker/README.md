# minco-aws-worker

Opt-in AWS Lambda SQS runtime for one explicitly configured worker invocation.
It returns `ReportBatchItemFailures` responses, validates identifiers and body
limits before application handling, bounds concurrency, and fails forward after
the first FIFO error to preserve ordering.

The crate does not create an event-source mapping, timer, scheduler, queue, AWS
SDK client, or detached background task. Infrastructure remains an explicit
application/deployment concern.

```rust,no_run
use async_trait::async_trait;
use minco_aws_worker::{
    MessageHandler, WorkerConfig, WorkerFailure, WorkerMessage, run_sqs_worker,
};
use std::sync::Arc;

#[derive(Debug)]
struct Handler;

#[async_trait]
impl MessageHandler for Handler {
    async fn handle(&self, message: WorkerMessage) -> Result<(), WorkerFailure> {
        println!("processing {}", message.message_id);
        Ok(())
    }
}

# async fn run() -> Result<(), lambda_runtime::Error> {
run_sqs_worker(Arc::new(Handler), WorkerConfig::default()).await
# }
```

The Lambda SQS event-source mapping must enable
`FunctionResponseTypes: [ReportBatchItemFailures]`. Applications using the
official events plugin may enable this crate's `events` feature and invoke
`dispatch_outbox_once`; Minco never schedules that recovery pass.

Application tests can inject `FakeMessageHandler`, queue a one-shot failure for
one message ID, and pass it through `process_sqs_event`. The fake records typed
attempts in order and exposes no message body or attribute values through
`Debug`; it performs no AWS contact or background work.
