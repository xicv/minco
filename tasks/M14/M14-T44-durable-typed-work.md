---
id: M14-T44
title: Add durable typed work and explicit scheduling
milestone: M14
status: active
priority: high
area: plugins/jobs/workers/scheduling
depends_on: [M14-T41]
operations:
  - placeOrder
owned_paths:
  - Cargo.lock
  - Cargo.toml
  - crates/minco-cli/src/jobs_cmd.rs
  - crates/minco-cli/src/lib.rs
  - crates/minco-cli/src/main.rs
  - crates/minco-plan/src/durable_work.rs
  - crates/minco-plan/src/lib.rs
  - crates/minco-plan/src/model.rs
  - crates/minco/Cargo.toml
  - crates/minco/src/lib.rs
  - docs/DECISIONS.md
  - docs/adrs/0048-durable-typed-work.md
  - docs/reference/generated/**
  - docs-site/.vitepress/config.mts
  - docs-site/next/guides/durable-jobs.md
  - docs-site/next/plugins/index.md
  - docs-site/tests/docs-discovery.spec.mts
  - examples/orders/adapters/src/jobs.rs
  - examples/orders/adapters/src/lib.rs
  - examples/orders/adapters/src/postgres.rs
  - examples/orders/adapters/src/sqlite.rs
  - examples/orders/application/src/jobs.rs
  - examples/orders/application/src/lib.rs
  - examples/orders/domain/src/lib.rs
  - examples/orders/migrations/postgres/**
  - examples/orders/migrations/sqlite/**
  - examples/orders/service/src/bin/jobs_worker.rs
  - examples/orders/service/src/lib.rs
  - extensions/minco-aws-adapters/src/jobs_sqs.rs
  - extensions/minco-aws-adapters/src/lib.rs
  - extensions/minco-aws-adapters/src/sqs.rs
  - extensions/minco-aws-adapters/Cargo.toml
  - extensions/minco-aws-adapters/tests/rustack.rs
  - extensions/minco-aws-worker/src/jobs.rs
  - extensions/minco-aws-worker/src/lib.rs
  - extensions/minco-aws-worker/Cargo.toml
  - extensions/minco-sqlx-postgres/src/jobs.rs
  - extensions/minco-sqlx-postgres/src/lib.rs
  - extensions/minco-sqlx-postgres/src/plugin_adapters.rs
  - extensions/minco-sqlx-postgres/migrations/plugins/**
  - extensions/minco-sqlx-sqlite/src/jobs.rs
  - extensions/minco-sqlx-sqlite/src/lib.rs
  - extensions/minco-sqlx-sqlite/src/plugin_adapters.rs
  - extensions/minco-sqlx-sqlite/migrations/plugins/**
  - plugins/catalog.toml
  - plugins/minco-plugin-jobs/**
  - roadmap/tasks.mmd
  - tasks/M14/M14-T44-durable-typed-work.md
checks:
  - cargo test -p minco-plugin-jobs --all-features --locked
  - cargo clippy -p minco-plugin-jobs --all-targets --all-features --locked -- -D warnings
  - cargo test -p minco-sqlx-postgres --all-features --locked -- --test-threads=1
  - cargo test -p minco-sqlx-sqlite --all-features --locked -- --test-threads=1
  - cargo clippy -p minco-sqlx-postgres -p minco-sqlx-sqlite --all-targets --all-features --locked -- -D warnings
  - cargo test -p minco-aws-adapters --features sqs,jobs --locked
  - cargo clippy -p minco-aws-adapters --all-targets --features sqs,jobs --locked -- -D warnings
  - cargo test -p minco-aws-worker --all-features --locked
  - cargo clippy -p minco-aws-worker --all-targets --all-features --locked -- -D warnings
  - cargo test -p minco-plan --all-features --locked
  - cargo clippy -p minco-plan --all-targets --all-features --locked -- -D warnings
  - cargo test -p minco --all-features --locked
  - cargo clippy -p minco -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo test -p orders-application -p orders-adapters -p orders-service --locked
  - scripts/docs/generate-reference.sh --check
---

# M14-T44 - Add durable typed work and explicit scheduling

## Goal

Add the missing application-level durable typed work lifecycle to Minco: typed
jobs with one registered handler, transactionally safe durable dispatch,
worker profiles and queues, Lambda worker execution with retry, timeout,
deduplication and overlap protection, permanent-failure recovery, explicit
schedules, and Plan/IAM/cost/topology evidence — as one coherent change
governed by ADR-0048.

## Acceptance

- `plugins/minco-plugin-jobs` ships a provider-neutral, publishable crate with
  typed job contracts, an explicit handler registry with duplicate detection,
  a bounded versioned envelope that redacts payload and metadata values from
  `Debug`, dispatch modes (inline/queued/durable), retry policies with
  overflow-safe backoff, memory stores, a deterministic fake dispatcher and a
  deterministic clock;
- PostgreSQL and SQLite adapters persist job state, publication intent,
  attempts, leases, deduplication and overlap locks with explicit migrations,
  atomic bounded claims (`FOR UPDATE SKIP LOCKED` / `BEGIN IMMEDIATE`),
  compare-and-set transitions, affected-row verification and concurrent
  behavioural tests;
- the business mutation and durable job commit and roll back atomically in one
  transaction on both engines, and commit leaves exactly one recoverable
  publication intent;
- an `SqsJobDispatcher` publishes envelopes with exact queue-target and body
  validation shared with the event publisher, Standard/FIFO support,
  deterministic deduplication identity, bounded delays within provider range,
  and redacted errors;
- the existing Lambda SQS worker gains an opt-in typed jobs path that decodes
  the envelope, resolves the handler, claims the execution lease, suppresses
  duplicate delivery of completed or cancelled jobs, enforces deadline and
  timeout, records attempts, persists retries and permanent failures before
  acknowledgement, and preserves every existing batch/FIFO behaviour;
- schedules render through an additive `durable_work` Plan sidecar that
  creates Scheduler-to-SQS resources with fresh per-invocation identity,
  least-privilege IAM, cost and wake-source evidence, validates against all
  schema-2 rules, and renders zero resources when disabled;
- the facade, plugin catalog, generated references, distribution manifest and
  docs integrate the new opt-in plugin without adding it to `default-plugins`;
- the Orders example proves the full slice durably and idempotently, and the
  operator surface can inspect, retry, cancel and recover guarded by a
  plan/apply digest contract;
- no hidden daemon, scheduler, poller, Redis dependency, NAT Gateway,
  provisioned concurrency or fixed compute is introduced.

## Progress evidence

Verified green at the time of writing, each against the exact working tree:

- `cargo test -p minco-plugin-jobs --all-features --locked`: 34 passed
  (envelope limits/redaction/closed shape, registry duplicates/upcasters,
  retry arithmetic, executor lifecycle, memory store claims, dedupe,
  overlap, deadlines, dispatch backoff).
- `MINCO_TEST_POSTGRES_URL=postgres://minco:minco@127.0.0.1:55432/minco_orders
  cargo test -p minco-sqlx-postgres --locked jobs -- --test-threads=1`:
  9 passed against real PostgreSQL, including rollback-with-business-mutation,
  commit-with-exactly-one-recoverable-intent, single-owner concurrent claims,
  stale-owner protection, dedupe determinism, revision-guarded operator
  transitions, one-authoritative concurrent retry, disjoint publication
  claims, and retry-through-republish round trip.
- `cargo test -p minco-sqlx-sqlite --locked jobs -- --test-threads=1`: 8
  always-on TempDir tests pass, including both atomicity proofs and the
  bounded attempt history.
- `cargo test -p minco-aws-adapters --features jobs --locked`: 12 passed
  (shared queue-target validation, delay range, FIFO delay rejection,
  deterministic groups/dedup identity, error redaction).
- `cargo test -p minco-aws-worker --all-features --locked`: 13 passed
  (durable execute-and-ack, duplicate suppression without re-execution,
  poison-envelope policy, unknown-job permanence before ack, durable retry
  reschedule, queued inline mode, standard-batch partial failure).
- `cargo clippy` with `-D warnings` and changed-file `rustfmt --check` pass
  for `minco-plugin-jobs`, `minco-sqlx-postgres`, `minco-sqlx-sqlite`,
  `minco-aws-adapters` (`jobs`) and `minco-aws-worker` (`jobs`).

## Non-goals

- exactly-once delivery claims;
- Step Functions, workflow/DAG engines, batches or chains;
- an always-running dispatcher or hidden wake source;
- publishing `minco-plugin-jobs` to crates.io from this task (first
  publication happens in the next coordinated lock-step release);
- contacting AWS, deploying, tagging or mutating production.

## Starting evidence

Task starts from merged `main`
`fc6483ccb42f86a7247dd65e1500716ed7132313` (1.10.0 family, 36 published
packages). The handoff's proposed task id M14-T42 was already taken on main by
the contract request boundary, and M14-T43 was locally occupied by an active
1.11.0 release-preparation workspace, so this task uses M14-T44. While the
feature was being implemented, Minco 1.11.0 was released on main
(`219884b06cf990afe54fd42eab34c79ada4a2bd0`); the change was rebased onto it,
so `minco-plugin-jobs` registers at workspace version 1.11.0 and its first
crates.io publication belongs to the next coordinated release. No AWS
request, deployment or production mutation is performed by this task.

## Additional verified evidence

- The worker decodes Scheduler `scheduled_trigger` deliveries (closed shape,
  schema 1), mints a fresh durable job per invocation deduplicated on
  `schedule_id:execution_id`, and routes malformed triggers to the queue's
  poison policy (`minco-aws-worker` jobs tests: 15 passed, all-features).
- The Plan sidecar synthesizes job queues, worker functions and
  event-source mappings into the schema-2 collections and re-derives
  `local_aws_services` and `iam_intents` exactly; enabling it introduces no
  new validation errors over the disabled baseline; `render_sam_with_durable_work`
  emits one `AWS::Scheduler::Schedule` per schedule plus one least-privilege
  `JobsSchedulerRole` (`sqs:SendMessage` only) with context-attribute inputs
  (`minco-plan`: 52 + 53 tests; clippy and changed-file `rustfmt --check`
  clean).
- The facade exposes `plugin-jobs` (opt-in, in `official-plugins` only), the
  catalog lists the plugin, `cargo minco plugin validate` passes with zero
  findings and `cargo minco plugin test jobs` passes the offline conformance
  boundary; generated references are regenerated and current.
- The Orders example commits one `orders.send-confirmation` job and one
  pending publication intent atomically with `placeOrder` on SQLite; the
  worker reaches the handler exactly once per order, duplicate deliveries
  produce no second effect, transient sink failures retry durably, and
  attempt exhaustion becomes an inspectable permanent failure
  (`orders-adapters` SQLite suite: 11 passed).
- `orders-jobs-worker` builds in `jobs-worker` and `jobs-worker,jobs-sqs`
  feature variants; dispatch binds `SqsJobDispatcher` when `JOBS_QUEUE_URL`
  is set and `FailClosedDispatcher` otherwise.

## Not run

- `cargo minco jobs` operator plan/apply CLI commands are not implemented;
  the store methods (`cancel`, `retry_failed`, `recover_expired_leases`,
  `list_failed`) are implemented, tested and revision-guarded, but no CLI
  surface wraps them yet.
- `scripts/quality.sh`, `scripts/ci/local-release.sh`, the Rustack SQS
  integration run, `sam validate` against a generated durable-work template,
  `cargo package`/semver checks, docs build/browser tests and the independent
  review pass were not run for this candidate; the focused gates above are
  the qualification boundary reached.
