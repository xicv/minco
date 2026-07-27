# Deploying an explicit SQS worker

`minco-aws-worker` is runtime code only. An application owns the queue, DLQ,
event-source mapping, IAM, retry visibility, reserved concurrency and cost
policy. Minco never creates or schedules these resources at startup.

Plan IR schema 2 models those choices explicitly. The event-source mapping must
enable partial-batch responses:

```yaml
WorkerFunction:
  Type: AWS::Serverless::Function
  Properties:
    Architectures: [arm64]
    Runtime: provided.al2023
    Handler: bootstrap
    CodeUri: ../../../target/lambda/worker/bootstrap.zip
    Timeout: 30
    MemorySize: 256
    Events:
      Queue:
        Type: SQS
        Properties:
          Queue: !GetAtt WorkerQueue.Arn
          BatchSize: 10
          FunctionResponseTypes:
            - ReportBatchItemFailures
```

Review these values together:

- queue visibility timeout is at least six function timeouts plus the batching
  window;
- batch size fits the configured `WorkerConfig::max_batch_size`;
- per-record bodies fit `max_message_bytes` and Lambda invocation size;
- each mapping's `maximum_concurrency` is bounded by downstream connection/rate
  limits and the sum for one worker fits reserved concurrency;
- FIFO batches always contain `MessageGroupId`; Minco processes in order and
  fails forward after the first failure;
- the DLQ/redrive policy, retention and alarms are explicit;
- reserved/provisioned concurrency, schedules, NAT and fixed capacity remain
  absent unless separately justified in Plan/cost review.

Use the `events` Cargo feature only when the application intentionally invokes
one bounded `dispatch_outbox_once` pass. It does not add a loop or timer.

AWS Lambda documents `FunctionResponseTypes`, batching and mapping scaling in
its
[SQS event-source configuration](https://docs.aws.amazon.com/lambda/latest/dg/services-sqs-configure.html)
and recommends a visibility timeout of at least six function timeouts plus any
batching window. Its
[partial-batch guidance](https://docs.aws.amazon.com/lambda/latest/dg/services-sqs-errorhandling.html)
requires FIFO processors to stop after the first failure and return failed and
unprocessed records.

See
[`plan-schema-v2-migration.md`](plan-schema-v2-migration.md) for a complete
configuration example, schedule policy and stable migration rejections.
