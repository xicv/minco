---
id: M14-T64
title: Ticket types and typed form answers
milestone: M14
status: active
priority: high
area: plugins/ticketing
depends_on: [M14-T63]
operations: [createTicket, exchangeTicketingHandoff]
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0066-ticket-types-and-typed-forms.md
  - plugins/minco-plugin-ticketing/migrations/sqlite/0006_ticketing_types_forms.sql
  - plugins/minco-plugin-ticketing/openapi/openapi.yaml
  - plugins/minco-plugin-ticketing/src/generated.rs
  - plugins/minco-plugin-ticketing/src/http.rs
  - plugins/minco-plugin-ticketing/src/model.rs
  - plugins/minco-plugin-ticketing/src/persistence.rs
  - plugins/minco-plugin-ticketing/src/service.rs
  - plugins/minco-plugin-ticketing/src/store.rs
  - plugins/minco-plugin-ticketing/src/jobs.rs
  - tasks/M14/M14-T64-ticket-types-and-forms.md
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

# M14-T64 - Ticket types and typed form answers

Stage E slice 1. Tickets gain a bounded taxonomy and typed, bounded
form answers at creation, contract-visible through the generated
request boundary. No form-definition DSL, no per-type registry.

## Goal

- `TicketType` (`question` default, `incident`, `problem`, `task`) on
  the ticket, agent summary and every requester projection.
- `TicketFormAnswer`: unique bounded `field_id` (max 16), `kind`
  (text/number/boolean/date_time), exactly one value slot; date_time is
  RFC 3339; numbers are f64-safe integers; fail-closed at the generated
  boundary (422) and in the domain.
- OpenAPI: `CreateTicket`/`ExchangeHandoff` accept both fields;
  `Ticket`/`TicketSummary`/`RequesterTicket`/`RequesterTicketSummary`
  expose them; standalone `TicketType`/`TicketFormValueKind` enum
  schemas; boundary re-synced.
- SQLite migration 0006 (`ticket_type`, `form_answers_json` columns,
  columnar reads); summary projection reads the type.

## Non-goals

- Curated views, macros, assignment modes, deadline snapshots, bounded
  search, collision indication, knowledge links, CSAT (later Stage E
  slices); any form-definition registry or automation DSL.

## Evidence

Run 2026-08-25 in the `minco-task-m14-t64` workspace:

- `cargo test -p minco-plugin-ticketing --all-features` — ok,
  **88 passed** (4 new: domain taxonomy+answers and fail-closed shapes;
  HTTP typed create echoing type/answers, default `question`, 422 for
  two-slot answers; sqlite columnar round-trip incl. summary type).
- Generated boundary current (contract digest re-synced; inline enums
  promoted to standalone schemas so the generator emits real enums).
- `cargo clippy -p minco-plugin-ticketing --all-targets --all-features
  --locked -- -D warnings` — clean; `cargo fmt --all -- --check` clean.
- Evidence chain: static/publish validation, source manifest stable,
  baseline re-bound, operational evidence PASS, deep review rerun.
- `cargo minco check --with-cargo` — result recorded at finish.
