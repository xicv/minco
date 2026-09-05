---
id: M14-T69
title: Durable clarification loop with resume checkpoints
milestone: M14
status: active
priority: high
area: plugins/ticketing
depends_on: [M14-T68]
operations: [createTicketingClarification, listTicketingClarifications, sendTicketingClarification, listTicketingRequesterClarifications, replyTicketingClarification]
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0071-clarification-loop-with-checkpoints.md
  - plugins/minco-plugin-ticketing/minco-plugin.json
  - plugins/minco-plugin-ticketing/migrations/sqlite/0011_ticketing_clarifications.sql
  - plugins/minco-plugin-ticketing/openapi/openapi.yaml
  - plugins/minco-plugin-ticketing/src/generated.rs
  - plugins/minco-plugin-ticketing/src/http.rs
  - plugins/minco-plugin-ticketing/src/model.rs
  - plugins/minco-plugin-ticketing/src/persistence.rs
  - plugins/minco-plugin-ticketing/src/plugin.rs
  - plugins/minco-plugin-ticketing/src/service.rs
  - plugins/minco-plugin-ticketing/src/store.rs
  - tasks/M14/M14-T69-clarification-loop-checkpoints.md
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

# M14-T69 - Durable clarification loop with resume checkpoints

Stage F slice 2. The continuation prompt's clarification loop made
concrete: durable drafts, a human send decision, exactly-once requester
answers, and checkpoints for explicit resume.

## Goal

- `Clarification` domain type: reason (missing/contradictory
  requirement), 1..8 bounded unique questions, opaque bounded
  checkpoint, state machine `draft → sent → answered` (+ `withdrawn`
  for unsent drafts) with exactly-once domain-validated transitions.
- Drafting is agent-manage and invisible to requesters; sending is the
  human decision (`ticketing.manage`); requesters answer once, own
  tickets only, one answer per question; answering a draft is
  not-found, never a state leak. Requester projections never carry
  checkpoints.
- Five operations (agent create/list/send, requester list/reply)
  aligned across contract/descriptor/distribution; SQLite migration
  0011 columnar; memory parity.

## Non-goals

- Auto-resume from checkpoints (resuming stays an explicit ADR-0070
  decision); automation-authored drafting wired into the
  `ticketing.run-development-automation` handler; notification fan-out
  on send.

## Evidence

Run 2026-08-26 in the `minco-task-m14-t69` workspace:

- `cargo test -p minco-plugin-ticketing --all-features` — ok,
  **103 passed** (new: full lifecycle — draft invisible, draft-reply is
  not-found, human send makes it visible, double-send/stale-reply/
  stranger-reply/partial-answers all fail closed, requester projection
  carries no checkpoint, agent view exposes checkpoint after answer;
  sqlite columnar round-trip through all three states).
- `cargo clippy ... -D warnings` clean; `cargo fmt --all -- --check`
  clean; generated boundary current.
- Evidence chain: static/publish validation, source manifest stable,
  baseline re-bound, operational evidence PASS, deep review rerun.
- `cargo minco check --with-cargo` — result recorded at finish.
