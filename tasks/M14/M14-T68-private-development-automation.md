---
id: M14-T68
title: Private development automation as reviewed proposals
milestone: M14
status: active
priority: high
area: plugins/ticketing
depends_on: [M14-T67]
operations: [requestTicketingAutomation, listTicketingAutomationProposals, decideTicketingAutomationProposal]
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0070-private-development-automation.md
  - plugins/minco-plugin-ticketing/minco-plugin.json
  - plugins/minco-plugin-ticketing/migrations/sqlite/0010_ticketing_automation_proposals.sql
  - plugins/minco-plugin-ticketing/openapi/openapi.yaml
  - plugins/minco-plugin-ticketing/src/generated.rs
  - plugins/minco-plugin-ticketing/src/http.rs
  - plugins/minco-plugin-ticketing/src/jobs.rs
  - plugins/minco-plugin-ticketing/src/model.rs
  - plugins/minco-plugin-ticketing/src/persistence.rs
  - plugins/minco-plugin-ticketing/src/plugin.rs
  - plugins/minco-plugin-ticketing/src/service.rs
  - plugins/minco-plugin-ticketing/src/store.rs
  - tasks/M14/M14-T68-private-development-automation.md
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

# M14-T68 - Private development automation as reviewed proposals

Stage F slice 1. The automation skeleton the continuation prompt
constrains: profile-gated durable commands, proposals never authority,
default exclusions enforced in code, human decisions exactly once,
agent-only privacy.

## Goal

- `AutomationConfig {profile (off default), review (always default)}`;
  review disabled with any enabled profile fails config validation
  (trusted verification does not ship).
- `ticketing.run-development-automation` durable command (envelope
  discipline per ADR-0054) whose handler fails closed on off-profile,
  assembles a deterministic proposal from ticket context, validates
  requested actions against the fixed exclusion list, and stores it
  `awaiting_review`.
- Agent surface: `POST /agent/tickets/{id}/automation` (202 + correlation
  id), `GET /agent/tickets/{id}/automation-proposals`, `PATCH
  /agent/automation-proposals/{id}` (accept/reject exactly once).
  Requester projections and public schemas carry nothing.
- SQLite migration 0010 (proposals table, ticket FK); descriptor and
  distribution inventories aligned; boundary re-synced.

## Non-goals

- Clarification drafts/checkpointed resume (next Stage F slice), real
  model adapters, execution of accepted proposals, local-CI evidence
  binding, risk-based review policies.

## Evidence

Run 2026-08-26 in the `minco-task-m14-t68` workspace:

- `cargo test -p minco-plugin-ticketing --all-features` — ok,
  **101 passed** (new: profile-off fails closed at trigger AND handler,
  review-disabled unconfigurable, the full exclusion list refuses and
  safe actions pass, decide-exactly-once, requester privacy of proposal
  state, and inline durable execution storing one awaiting-review
  proposal).
- `cargo clippy ... -D warnings` clean; `cargo fmt --all -- --check`
  clean; generated boundary current.
- Evidence chain: static/publish validation, source manifest stable,
  baseline re-bound, operational evidence PASS, deep review rerun.
- `cargo minco check --with-cargo` — result recorded at finish.
