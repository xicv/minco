---
id: M14-T66
title: Assignment modes and SLA deadline snapshots
milestone: M14
status: active
priority: high
area: plugins/ticketing
depends_on: [M14-T65]
operations: [changeTicketAssignment]
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0068-assignment-modes-and-sla-snapshots.md
  - plugins/minco-plugin-ticketing/migrations/sqlite/0008_ticketing_assignment_sla.sql
  - plugins/minco-plugin-ticketing/openapi/openapi.yaml
  - plugins/minco-plugin-ticketing/src/generated.rs
  - plugins/minco-plugin-ticketing/src/http.rs
  - plugins/minco-plugin-ticketing/src/model.rs
  - plugins/minco-plugin-ticketing/src/persistence.rs
  - plugins/minco-plugin-ticketing/src/service.rs
  - plugins/minco-plugin-ticketing/src/store.rs
  - tasks/M14/M14-T66-assignment-modes-sla-snapshots.md
  - verification/1.9-performance-baseline.json
  - verification/source-manifest.json
  - verification/operational-evidence-validation.json
  - verification/static-validation.json
  - verification/publish-validation.json
  - verification/deep-review.json
checks:
  - cargo test -p minco-plugin-ticketing --all-features --locked
  - cargo clippy -p minco-plugin-ticketing --all-targets --all-features --locked -- -D warnings
  - cargo minco check --with-cargo
---

# M14-T66 - Assignment modes and SLA deadline snapshots

Stage E slice 3. `changeTicketAssignment` gains an explicit mode
(manual / round_robin / least_workload) backed by a configured bounded
assignment pool, a durable round-robin cursor and a deterministic
least-workload query; ticket creation snapshots first-response and
resolution deadlines from an optional SLA config.

## Goal

- Required `mode` on the assignment request; manual keeps today's
  set/clear semantics; pool modes pick server-side (`advance_assignment_cursor`
  transactional, `assignee_workload` grouped count with lexicographic
  tie-break); empty pool fails closed.
- `TicketSlaConfig` (hours from creation, 0 disables one) snapshots
  `first_response_deadline`/`resolution_deadline` on API and handoff
  creates; agent ticket + summaries expose them; requester projections
  never do.
- Migration 0008 (deadline columns + cursor table); generated boundary
  re-synced; columnar reads/projection carry the new fields.

## Non-goals

- Bounded search, knowledge links, CSAT (next slice); auto-assignment
  policies on creation or events (assignments stay explicit agent
  decisions); requester-visible SLAs.

## Evidence

Run 2026-08-25 in the `minco-task-m14-t66` workspace:

- `cargo test -p minco-plugin-ticketing --all-features` — ok,
  **93 passed** (new: creation snapshots; round-robin advance a→b,
  least-workload tie-break, manual set/clear; pool-less modes fail
  closed; HTTP mode PATCH + deadline fields on create).
- `cargo clippy ... -D warnings` clean; `cargo fmt --all -- --check`
  clean; generated boundary current.
- Correctness closure found by the gate: the T59 wake test fixture
  stamped a FIXED `eventTime` (`2026-08-25T10:00:00Z`), so the inbound
  envelope's six-hour deadline expired once wall clock passed it — a
  time-bombed test. Fixed with a per-process dynamic stamp shared by
  redelivery pairs (the semantic fingerprint anchors on the arrival
  time); the worker suite is 20/20.
- Evidence chain: static/publish validation, source manifest stable,
  baseline re-bound, operational evidence PASS, deep review rerun.
- `cargo minco check --with-cargo` — result recorded at finish.
