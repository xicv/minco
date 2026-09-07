---
id: M14-T54
title: Dispatch ticketing activity intents as domain events through the required events service
milestone: M14
status: active
priority: high
area: plugins/ticketing/events
depends_on: [M14-T53]
operations: []
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0056-activity-intents-as-domain-events.md
  - plugins/minco-plugin-ticketing/**
  - tasks/M14/M14-T54-activity-event-dispatch.md
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

# M14-T54 - Dispatch ticketing activity intents as domain events

Closes the last two open review findings together: activity intents had no
dispatch lifecycle, and the plugin required `events.publish`/`events.outbox`
capabilities at install while never using the events service.

## Goal

- Every ticket mutation already writes a semantic activity intent in the
  same transaction. A new explicit, bounded service pass —
  `dispatch_pending_activity(project, limit)` — publishes each unpublished
  intent as a `DomainEvent` (type = intent kind, aggregate = the ticket,
  correlation = the intent's correlation id, payload = the bounded intent
  payload) through the events service's publisher, then marks the intent
  published. No hidden schedule, timer or worker: applications call it
  request-assisted, from a worker profile, or as an operator command, per
  the events ADR.
- Publication is at-least-once (publish, then mark): a crash between the
  two replays one event; consumers are already required to be idempotent.
  A dispatch failure stops the pass and leaves remaining intents pending.
- The events service becomes a used dependency: plugin install passes the
  resolved `EventServices` into the ticketing service.

## Non-goals

- Transactional coupling of intent publication to an events outbox (the
  intent row already commits with the mutation; the outbox would add a
  second relay for the same guarantee).
- Any automatic scheduling.

## Evidence

Run 2026-08-24 in the `minco-task-m14-t54` workspace:

- `cargo test -p minco-plugin-ticketing --all-features --locked` — ok,
  **68 passed** (real temporary SQLite; +2 dispatch proofs).
- Feature matrix — default (44), `--no-default-features` (21), sqlite
  (55), `full` (68) — all ok.
- `cargo clippy … -- -D warnings` — clean; `rustfmt --check` clean;
  `plugin validate` `[]`; `cargo package` verified; `contract sync --check`
  passes; docs reference current.
- `cargo minco check --with-cargo` — result recorded at finish.

Behavioral proofs: create + agent reply leaves two intents; one explicit
dispatch pass publishes exactly two domain events (`ticketing.created`,
`ticketing.agent_replied`, aggregate `ticketing.ticket`, correlation
preserved) and marks both published; a second pass publishes nothing;
without the events service the pass fails closed; SQLite records
`published_at`, marks idempotently (second mark false), and pending
queries exclude published rows.
