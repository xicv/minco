---
title: Queues and Workers
description: Add bounded SQS-driven work without hidden polling, schedules, or fixed compute.
---

# Queues and Workers

Minco models background work as an explicit runtime and trigger. Installing a
plugin never silently creates a queue, mapping, dead-letter queue, schedule,
IAM grant, or running worker.

## Compile the runtime

```toml
[dependencies]
minco = { version = "1.6.0", features = ["plan", "aws-worker"] }
```

Generate an application-owned worker boundary with a dry run first:

```bash
cargo minco make worker order-receipts --dry-run --json
```

The application owns the message contract and use case. The worker maps an SQS
record to that use case and maps failures to the partial-batch response; it does
not contain business persistence policy.

## Declare the topology

Plan IR names the worker function, queue, event-source mapping, retry and DLQ
policy, timeout, reserved concurrency, batch behavior, IAM, and any database
connection budget. `ReportBatchItemFailures` must be enabled.

Review the graph before rendering provider artifacts:

```bash
cargo minco deploy plan --json
cargo minco cost --json
cargo minco perf --json
```

## Failure behavior

The runtime supports:

- partial batch failures so successful records are not retried;
- FIFO fail-forward handling that stops after the first failed group boundary;
- bounded concurrent record work;
- redacted message diagnostics;
- deterministic batch item identifiers.

Queue visibility must exceed the function timeout with a reviewed retry margin.
Reserved concurrency and batch concurrency must also fit the selected database
connection budget.

## Cost and wake boundary

Lambda worker compute is `zero_compute` at idle. Queue storage can remain, and
message delivery is a `queue_message` wake source with request-driven charges.
The default minimal profile rejects schedules; a selected schedule must appear
explicitly with its cleanup and residual-cost behavior.

## Verify by layer

```bash
cargo test --locked -p minco-aws-worker
cargo test --locked -p minco-plan --test multi_runtime
```

These prove runtime mapping and structural planning locally. A provider smoke
must separately create an authorized bounded queue/function/mapping, send known
messages, observe results and redrive behavior, then prove cleanup.
