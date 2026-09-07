---
id: M14-T56
title: Route inbound email to tickets by threading and submit the durable ingest job
milestone: M14
status: active
priority: high
area: plugins/ticketing/email-ingress
depends_on: [M14-T55]
operations: []
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0058-inbound-thread-routing.md
  - plugins/minco-plugin-ticketing/**
  - tasks/M14/M14-T56-inbound-thread-routing.md
  - verification/1.9-performance-baseline.json
  - verification/source-manifest.json
  - verification/operational-evidence-validation.json
  - verification/static-validation.json
  - verification/publish-validation.json
  - verification/deep-review.json
  - docs/reference/generated/plugins.md
checks:
  - cargo test -p minco-plugin-ticketing --all-features --locked
  - cargo clippy -p minco-plugin-ticketing --all-targets --all-features --locked -- -D warnings
  - cargo minco plugin validate
  - cargo package -p minco-plugin-ticketing --locked
  - cargo minco check --with-cargo
---

# M14-T56 - Route inbound email to tickets by threading and submit the durable ingest job

Stage D2 slice 1 (ADR-0058): the engine-neutral routing half of the
inbound bridge. The S3/SQS wake and SES receiving wiring is slice 2; no
provider is contacted and no topology is added here.

## Goal

- New store port `find_ticket_by_message_identity`: resolve a ticket (and
  its current revision) from a stored external message's
  `internet_message_id`, memory and SQLite.
- New service use case `submit_inbound_email` (jobs feature): given a
  verified raw-object reference (provider, mailbox scope, external id,
  content digest, object key) and bounded threading headers, resolve the
  target ticket — `in_reply_to` first, then `references` newest-first —
  and submit the `ticketing.process-inbound-email` job durably with the
  resolved ticket id and current revision. Unresolved threading fails
  closed (`ticketing.inbound_thread_unresolved`); the caller decides
  whether to open a new ticket. No subject-only threading.
- Portal services gain the optional `JobsServices` handle (jobs feature,
  resolved at install from the registry where the jobs plugin registered
  it); handler registration unchanged.

## Non-goals

- New-ticket creation from unmatched email (later slice).
- S3 event/Lambda wiring, SES receiving rules, SQS wake, Rustack seams,
  Plan topology.
- Outbound email.

## Evidence

Run 2026-08-24 in the `minco-task-m14-t56` workspace:

- `cargo test -p minco-plugin-ticketing --all-features --locked` — ok,
  **72 passed** (real temporary SQLite; +3 routing proofs).
- Feature matrix — default (45), `--no-default-features` (21), sqlite
  (33), `full` (72) — all ok.
- `cargo clippy … -- -D warnings` — clean; `rustfmt --check` clean;
  `plugin validate` `[]`; `cargo package` verified; `contract sync
  --check` passes; docs reference current.
- `cargo minco check --with-cargo` — result recorded at finish.

Behavioral proofs: `in_reply_to` resolves the ticket through the stored
external identity and the durable job carries the resolved ticket id and
current revision with the exact D1 dedupe key; the `references` chain
resolves newest-first when `In-Reply-To` is absent; resubmission of the
same external identity with the same arrival anchor returns the existing
durable job (the deadline derives from `arrived_at`, making the semantic
fingerprint stable across wake retries — discovered when a fresh
`Utc::now()` deadline made identical submissions conflict fail-closed);
unresolved threading fails closed with nothing submitted; without the
jobs handle the use case fails closed; SQLite resolution matches memory
including provider/project isolation misses.

Security scan (blocker closure): mimosa deep scan completed on the PR
branch workspace — sealed scan
`scan-2026-08-24T11-28-54.757Z-0f5a18f84605`, 28 findings, 1 dependency
advisory match, static-only evidence boundary. Triage: every finding is
pre-existing on published 1.12.0 main (four are duplicates inside
`target/` build artifacts, not source; the flagged
`assets/support-entry.js` lines predate this PR); none were introduced
by the ticketing stages. The high-severity items sit in
waffo/proofs/scripts paths owned by other tasks and are reported for
their owners rather than fixed inside this task.
