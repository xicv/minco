---
title: Durable Jobs and Schedules
description: Dispatch typed work durably with at-least-once delivery, lease-based execution, bounded retries and explicit EventBridge Scheduler dispatch.
---

# Durable Jobs and Schedules

The jobs plugin adds the application-level lifecycle between "this business
mutation happened" and "this side effect ran": typed job commands with one
registered handler, transactionally safe durable dispatch, worker profiles
and queues, Lambda execution with retry, timeout, deduplication and overlap
protection, inspectable permanent failure, and explicit schedules.

## Enable the capability

```bash
cargo minco plugin enable jobs --dry-run --json
```

The plugin is opt-in and never part of the default stack. Disabling it
renders no queue, worker or schedule.

## Jobs are commands, not events

A domain event is a fact with zero-or-many consumers. A job is a command
with exactly one registered handler: `SendOrderConfirmation`,
`ExpireUnpaidOrders`. Jobs never serialize as domain events.

```text
use case transaction -> domain state + job row + publication intent
explicit dispatcher  -> claim due publications -> queue message
worker delivery      -> execution lease -> handler -> durable transition
```

The durable job row owns execution state. The publication row owns pending
transport delivery. The queue message is delivery, never truth.

## Dispatch atomically with the business mutation

The SQL adapters accept the application's transaction directly, so rolling
the business mutation back leaves no job and no publication intent, and
committing leaves exactly one recoverable intent:

```rust
let mut db = pool.begin_with("BEGIN IMMEDIATE").await?;
orders.insert(&mut db, &order).await?;
jobs.enqueue_in(&mut db, pending_record(envelope)).await?;
db.commit().await?;
```

After commit, an explicit bounded pass (`dispatch_due_once`) publishes due
intents. Nothing polls in the background: publication is request-assisted,
worker-piggybacked, or triggered by an explicitly declared dispatcher run.

## Delivery is at least once

Duplicate deliveries are neutralized by an atomic execution claim: only one
worker moves a job from `pending` to `running`; a duplicate delivery of a
completed, cancelled or permanently failed job is acknowledged without
re-executing the handler. Application effects must still be idempotent.

Retries are durable and bounded. The job row owns the attempt count and the
next availability time; fixed or exponential backoff is computed from the
envelope's retry policy and never silently truncated to a queue delay. A
retryable failure persists the retry state before the delivery is
acknowledged. Attempt exhaustion, handler classification, unknown jobs and
unsupported payload versions become inspectable permanent failures —
persisted before acknowledgement.

## Scheduled work uses the same path

Declare schedules in the deployment topology's `durable_work` sidecar.
Minco renders `EventBridge Scheduler -> SQS job queue -> Lambda worker` with
a least-privilege scheduler role (`sqs:SendMessage` only). The scheduler
input embeds the provider's per-invocation context attributes, so every
recurrence carries a fresh execution identity and the worker mints the
durable job at delivery — no static job ID, no dispatcher function. The
worker deduplicates on `schedule_id:execution_id`, so scheduler retries
produce one execution.

Schedules are visible wake sources: they appear in plan validation, cost
evidence and inspection, and a disabled capability renders zero schedule
resources.

## Operators recover through plan/apply

Listing failed jobs, inspecting one job, retrying a permanently failed job
and cancelling a pending job are guarded operations: read-only plans with
exact digests, explicit approval, and expected-state checks, so concurrent
operators cannot double-apply a recovery.
