# Plan IR schema 2 migration

Plan IR schema 2 adds explicit API/worker roles, queues and typed triggers. It
is a likely Minco `0.4.0` serialized and public-Rust-API boundary. Minco 0.3.1
continues to accept API-only schema 1 configurations; this change does not
publish or version-bump the crates.

## Compatibility matrix

| Input | Result |
|---|---|
| Schema 1, one API function, no queue/trigger/schedule fields | Remains valid; can migrate deterministically |
| Schema 1 with queue, trigger or legacy schedule data | `MINCO-PLAN-MIGRATE-001` |
| Schema 1 with other than one function | `MINCO-PLAN-MIGRATE-002` |
| Unsupported source schema | `MINCO-PLAN-MIGRATE-003` |
| Schema 2 with `scheduled_wakeups` strings | `MINCO-SCHEDULE-003` |

There is no mutating migration command. `DeploymentPlan::migrate_to_latest`
exists for consumers that already hold a validated plan. Application
configuration should be upgraded explicitly and reviewed.

## Upgrade an API-only configuration

1. Keep the existing function values and add `role = "http_api"`.
2. Set `schema_version = 2`.
3. Add exactly one HTTP trigger referencing that function.
4. Run `cargo minco deploy plan` and review the typed `triggers` and
   `iam_intents` projections.
5. Render and lint SAM before using the new plan as release evidence.

```toml
schema_version = 2

[[functions]]
name = "api"
role = "http_api"
artifact_path = "target/lambda/orders-lambda/bootstrap.zip"
memory_mb = 512
timeout_seconds = 15
reserved_concurrency = 2
provisioned_concurrency = 0
database_connections_per_instance = 2

[[triggers]]
kind = "http_api"
id = "api"
function_id = "api"
```

All OpenAPI operations remain derived from the canonical contract and are
assigned to the single HTTP trigger target. Workers cannot own HTTP operations.

## Add an SQS worker

Every worker has an independent artifact and explicit queue mapping:

```toml
[[functions]]
name = "orders-worker"
role = "worker"
artifact_path = "target/lambda/sqs_worker/bootstrap.zip"
memory_mb = 256
timeout_seconds = 30
reserved_concurrency = 2
provisioned_concurrency = 0
database_connections_per_instance = 1

[[queues]]
id = "orders"
fifo = false
visibility_timeout_seconds = 180
retention_seconds = 345600

[[triggers]]
kind = "sqs"
id = "orders"
function_id = "orders-worker"
queue_id = "orders"
batch_size = 10
batching_window_seconds = 0
report_batch_item_failures = true
maximum_concurrency = 2
```

The visibility minimum is six times the function timeout plus the batching
window. Mapping maximum concurrency is 2 to 1000; the sum of mappings targeting
one worker must fit its reserved concurrency. Standard batches above 10 require
a non-zero batching window. FIFO queues use batches of at most 10 and a zero
batching window. Source and dead-letter queues must agree on FIFO behavior.

`max_receive_count` is an application-owned retry/redrive decision. Minco
bounds it to 1 through 1000 and recommends reviewing AWS's current guidance
instead of treating that bound as product retry policy.

## Replace a legacy schedule

Legacy strings cannot be migrated because they do not identify a target,
enablement state or purpose. Replace each one with a typed trigger:

```toml
[[triggers]]
kind = "schedule"
id = "outbox-recovery"
function_id = "orders-worker"
expression = "rate(15 minutes)"
enabled = false
purpose = "recover stranded outbox records"
```

Schedules accept EventBridge `at(...)`, `rate(...)` or `cron(...)` forms. They
are disabled or rejected under the default minimal-idle policy. To enable one,
set `cost_policy.deny_scheduled_wakeups = false`, then review the emitted wake
and cost diagnostic. Local topology never runs it automatically.

## Database boundary

PostgreSQL-compatible Lambda functions receive exact SSM parameter and optional
customer-managed KMS intents. Local-native and DynamoDB plans do not. DynamoDB
remains access-pattern-specific: a plan can describe a DynamoDB worker and its
local services, but generic SAM rendering rejects it until a selected adapter
and renderer declare the table keys, operations and exact IAM.

## Verification

```bash
cargo minco deploy plan --config path/to/minco.toml
cargo minco deploy render-sam --config path/to/minco.toml
sam validate --lint --template-file infra/aws/generated/template.yaml
cargo minco cost --config path/to/minco.toml
cargo minco perf --config path/to/minco.toml
```

These commands plan, render and inspect. They do not deploy, migrate a database
or publish a crate.
