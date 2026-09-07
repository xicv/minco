---
id: M14-T52
title: Add the optional Ticketing-to-Jobs bridge with transactional notification enqueue
milestone: M14
status: active
priority: high
area: plugins/ticketing/jobs
depends_on: [M14-T51]
operations: []
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0054-optional-ticketing-jobs-bridge.md
  - plugins/minco-plugin-ticketing/**
  - tasks/M14/M14-T52-jobs-bridge.md
  - verification/1.9-performance-baseline.json
  - verification/source-manifest.json
  - verification/operational-evidence-validation.json
  - verification/static-validation.json
  - verification/publish-validation.json
  - verification/deep-review.json
  - docs/reference/generated/plugins.md
checks:
  - cargo test -p minco-plugin-ticketing --all-features --locked
  - cargo test -p minco-plugin-ticketing --no-default-features --locked
  - cargo clippy -p minco-plugin-ticketing --all-targets --all-features --locked -- -D warnings
  - cargo minco plugin validate
  - cargo package -p minco-plugin-ticketing --locked
  - cargo minco check --with-cargo
---

# M14-T52 - Add the optional Ticketing-to-Jobs bridge with transactional notification enqueue

Stage C of the Ticketing sequence (ADR-0054). Reuses `minco-plugin-jobs`
(ADR-0048) exactly as released; Ticketing adds only an optional,
feature-gated bridge — no second queue, lease, retry or scheduling system.

## Goal

- New optional `jobs` feature (never in the default set): one typed
  command `ticketing.deliver-public-notification` v1 with a real handler
  that delivers through the notifications plugin's `NotificationService`
  port — no fake success, no placeholder.
- Envelope policy per the continuation contract: dedupe
  `notification:{ticket}:{message}`, overlap `ticket:{ticket}`, partition
  = project routing reference, bounded exponential retry, one-hour
  deadline so stale acknowledgements never send, causation = the
  triggering correlation ID. Payload carries bounded identifiers only —
  never message bodies or credentials.
- Pattern A transactional coupling: when a public agent reply carries a
  notification job, the job record is enqueued in the same SQL transaction
  as the ticket mutation through a new ticketing-owned
  `TicketingJobEnqueue` port (memory profile records the records for
  inspection). Jobs present without a configured sink fail closed; nothing
  is silently dropped. `commit + submit_durable` is never claimed atomic.
- Static explicit handler registration: the composition root registers
  ticketing handlers into its `JobHandlerRegistry` before building
  `JobsServices`; the plugin adds no hidden worker, queue, schedule or
  provider resource, and with the feature disabled the plugin is
  byte-for-byte its previous behavior (no new dependencies in the default
  build).

## Non-goals

- `transcribe-audio` and `classify-ticket` commands (their capabilities
  arrive with the media/AI stages; policies are reserved in ADR-0054, no
  dead handlers are registered).
- Schedules, SQS wiring, worker binaries, operator CLI.
- Any change to the jobs plugin itself.

## Evidence

Run 2026-08-24 in the `minco-task-m14-t52` workspace:

- `cargo test -p minco-plugin-ticketing --all-features --locked` — ok,
  **60 passed** (real temporary SQLite; +8 bridge proofs).
- Feature matrix — default (43, unchanged from before the bridge),
  `--no-default-features` (20), sqlite-only (30), jobs-only (25), `full`
  (60) — all ok; the default build has no jobs dependency at all.
- `cargo clippy -p minco-plugin-ticketing --all-targets --all-features
  --locked -- -D warnings` — clean; `rustfmt --check` over changed files —
  clean.
- `cargo minco plugin validate` — `[]`; `cargo package` verified;
  `contract sync --check` passes; docs reference current; stub-marker scan
  over `jobs.rs` clean.
- `cargo minco check --with-cargo` — result recorded at finish.

Behavioral proofs:

- **Envelope policy**: dedupe `notification:{ticket}:{message}`, overlap
  `ticket:{ticket}`, partition = project, deadline set, profile
  `ticketing-mail`; payload carries identifiers only (no body content);
  envelope Debug reveals nothing.
- **Handler is real**: executed through the released `JobsServices`
  inline path against a `MemoryNotificationSink` — the requester receives
  the public message body with ticket reference; a missing ticket/message
  is a permanent `ticketing.notification_target_missing` with nothing
  sent (no fake success).
- **Pattern A atomicity (real SQLite)**: with the released
  `SqliteJobStore` adapted behind `TicketingJobEnqueue` on one shared
  pool, a successful append commits exactly one `minco_jobs` row; a stale
  append rolls the whole transaction back leaving no row; more than 8
  records fails closed before any write; records without a configured
  sink fail closed.
- **Service coupling**: with `notify_requester_on_public_reply` enabled,
  a public agent reply attaches exactly one notification job; internal
  notes never notify; the default configuration attaches nothing.
- No new HTTP operations, descriptor capabilities or topology: the bridge
  is invisible to applications that do not enable the feature (proven by
  the byte-identical default feature test count and the unchanged
  manifest).
