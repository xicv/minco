# ADR 0048: Durable typed work, at-least-once delivery and explicit scheduling

## Status

Accepted.

## Context

Minco has SQS queues, Lambda workers, queue Plan IR, transactional event
outboxes, idempotency and audit ledgers, but no application-level job
lifecycle connecting them. Applications that need "do this work later, safely"
must today hand-roll a queue message and hope: the message may be lost between
a database commit and the SQS send (dual write), a crash mid-flight produces
unknowable state, retries are governed only by queue redelivery, and there is
no operator answer for "what failed and how do I rerun it".

Laravel (queues docs, 13.x, 2026-08) demonstrates the semantics applications
expect: dispatch-after-commit, unique jobs, overlap middleware, bounded
attempts, fixed/exponential backoff, retry deadlines, timeouts that consume an
attempt, and a persisted failed-job table with retry/forget/prune. Its runtime
assumptions (Redis-backed locks, always-running workers, a per-minute cron
scheduler, pcntl alarms) do not transfer to Minco's zero-daemon, Lambda/SQS
profile.

AWS (SQS/Lambda/Scheduler docs, 2026-08) supplies the transport truths: SQS
messages are at-least-once and now bounded at 1 MiB; per-message delay is at
most 900 seconds and is unavailable on FIFO queues; visibility timeout is at
most 12 hours; Lambda validates visibility timeout against function timeout
and recommends at least six times the function timeout; partial batch
responses report per-item failures and FIFO processing stops after the first
failure; EventBridge Scheduler targets can embed per-invocation context
attributes (`<aws.scheduler.execution-id>`, `<aws.scheduler.scheduled-time>`)
in a static target input, giving each recurrence a fresh identity without an
intermediate function.

Apalis (1.0-rc line, 2026-08) validates the lease-column claim model — a
single `UPDATE ... WHERE id IN (SELECT ... FOR UPDATE SKIP LOCKED) RETURNING`
claim with `status/lock_by/lock_at` columns and a `run_at` retry time — but its
worker registration and keep-alive heartbeats assume a daemon. Tower
contributes the classification pattern (retryable versus permanent decided by
a typed policy over the error) rather than any specific middleware. SQLx 0.9
supports both claim dialects Minco needs: PostgreSQL `FOR UPDATE SKIP LOCKED`
CTEs and SQLite `BEGIN IMMEDIATE` write transactions with WAL and
`busy_timeout`.

## Decision

Minco adds a provider-neutral jobs plugin (`minco-plugin-jobs`), SQL job
stores, an SQS job dispatcher, a typed Lambda worker path, an explicit
scheduling topology and a guarded operator surface. Jobs are commands with one
registered handler, distinct from domain events (facts with zero-or-many
consumers); they never serialize as `DomainEvent`.

**Durable state owns execution; the transport only delivers.** A job row
(`pending`, `running`, `succeeded`, `pending` after retry, `failed_permanent`,
`cancelled`) plus a publication row (`pending`, `claimed`, `published`,
`failed`) are written atomically with the business mutation through
`enqueue_in(&mut Transaction<DB>, ...)`. Rollback leaves no job and no
publication intent; commit leaves exactly one recoverable intent. The SQS
message is delivery, not state.

**Dispatch modes are explicit.** `Inline` executes in-process for explicit use
and tests. `Queued` serializes and publishes directly, accepting at-least-once
with no durable row. `Durable` commits the row plus publication intent and
recovers publication after process failure. Inline is never a hidden fallback
for failed durable infrastructure.

**Delivery is at least once; effects are idempotent.** Duplicate deliveries
are neutralized by a compare-and-set execution claim: only one worker moves a
job `pending -> running`; duplicates observe a terminal or leased state and are
acknowledged without executing the handler. Ambiguous SQS sends recover by
releasing the publication claim for redelivery.

**Retries are durable and bounded.** The job row owns `attempt_count` and
`available_at`; backoff (fixed or exponential, saturating, capped) is computed
from the envelope's retry policy, never silently truncated by SQS's 900-second
delay ceiling. A retryable failure persists the retry state before the
delivery is acknowledged. Permanent failure — handler classification, attempt
exhaustion, unknown job, unsupported payload version or expired deadline — is
persisted before acknowledgement and stays inspectable. `Queued` delayed
dispatch uses SQS `DelaySeconds` when and only when the delay fits the
provider range, and rejects FIFO delays before provider contact.

**Execution leases and overlap locks are separate.** The running claim carries
an owner and expiry; expired claims are recovered deterministically.
`without_overlapping` uses a narrow lock table keyed by an explicit overlap
key with acquire/refresh/release and TTL expiry — never a process mutex, and
never a general cache abstraction.

**Submissions may be deduplicated.** An optional dedupe key is unique across
all states: a duplicate submission returns the existing job identity, and the
same key with a different payload fails closed.

**Payloads version explicitly.** The envelope is closed (`deny_unknown_fields`,
`schema_version = 1`); unknown envelope fields or unsupported payload versions
fail deterministically to permanent failure rather than guessing. Payloads are
open, tagged with `job_version`, and upgraded through registered upcasters
within a supported compatibility window (current and previous version). Large
content uses object-storage references, never queue payloads. Envelope bytes
are bounded below the provider ceiling; payload, metadata and secrets never
appear in `Debug`.

**Schedules produce fresh identities and remain visible wake sources.** An
`EventBridge Scheduler -> SQS job queue -> Lambda worker` topology is rendered
with the scheduler execution role granted least-privilege `sqs:SendMessage`.
The scheduler input embeds the documented context attributes, so each
recurrence carries a distinct `execution-id`; the worker mints the durable job
at delivery and deduplicates on `schedule_id:execution_id`. No per-recurrence
job ID is ever static, no dispatcher Lambda is required, and `TriggerPlan::Schedule`
semantics are unchanged: durable work is an additive, optional
`durable_work` Plan sidecar (`DurableWorkTopology`) whose queues, workers,
mappings and schedules validate through the existing schema-2 rules, IAM
derivation, logical-ID collision map, cost evidence and wake-source inspection.
Disabling the capability renders zero resources.

**Operators recover through a plan/apply contract.** Inspecting, retrying,
cancelling and lease recovery go through read-only plans with exact digests
and explicit approval, guarded by expected state, using the repository's
existing safe target resolution. No command mutates an arbitrary production
database URL.

## Rejected alternatives

- A general workflow/DAG engine, Step Functions abstraction, batches or chains
  in this task: single-command jobs with explicit scheduling cover the
  lifecycle; orchestration would add state Minco cannot honestly own.
- Redis or a cache abstraction for locks: violates the no-Redis requirement
  and generalizes one need into a second source of truth.
- An always-running dispatcher, scheduler or poller: hidden wake sources are
  forbidden; dispatch is request-assisted, worker-piggybacked or explicitly
  scheduled and cost-evidenced.
- Holding the SQL transaction open across handler execution (or heartbeats à
  la Apalis): Lambda executions are short and may vanish; leases survive them.
- `deny_unknown_fields` on payloads: breaks the new-writer/old-reader
  compatibility window the envelope version already governs.
- A dispatcher Lambda for schedules: the documented Scheduler context
  attributes already provide fresh per-invocation identity; the extra request
  and cost are unnecessary.
- Exactly-once claims: SQS is at-least-once; Minco makes effects idempotent
  instead of promising what the transport cannot.
