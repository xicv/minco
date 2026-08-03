# Add an explicit bounded SQS worker

Workers are opt-in functions in Plan IR schema 2. Installing a plugin never
silently creates a queue, event-source mapping, dead-letter queue, schedule, IAM
grant, or running worker.

## Features

Enable `aws-worker` and `plan`. Add the specific application/plugin features
used by the worker; do not enable the full facade merely to obtain a runtime.

## Provider assumptions

The checked recipe runs Rust tests only. Application code must explicitly name
the worker function, queue, mapping, retry/DLQ policy, timeout, reserved
concurrency, batch behavior, and any database connection budget.

## Cost and wake behavior

Lambda worker compute is `zero_compute` at idle and invocation/queue operations
are `request_only`. An SQS message is an explicit `queue_message` wake source.
Enabled schedules remain rejected by the default minimal-idle policy.

```bash
cargo test --locked -p minco-aws-worker
cargo test --locked -p minco-plan --test multi_runtime
```

The Plan tests enforce partial batch responses, queue visibility relative to
function timeout, bounded mapping concurrency, FIFO/DLQ compatibility, exact
IAM intent, and visible cost/wake diagnostics.

## Verification

The matrix executes `worker-runtime` and `worker-plan`.

## Unsupported gates

This recipe creates no queue, sends no message, deploys no Lambda, enables no
schedule, and proves no live delivery, redrive, alarm, or production recovery
behavior.
