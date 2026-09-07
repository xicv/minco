---
id: M14-T67
title: Bounded search, knowledge links and one-shot CSAT
milestone: M14
status: active
priority: high
area: plugins/ticketing
depends_on: [M14-T66]
operations: [listTicketingAgentSearch, replaceTicketingKnowledgeLinks, submitTicketingCsat]
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0069-bounded-search-knowledge-csat.md
  - plugins/minco-plugin-ticketing/minco-plugin.json
  - plugins/minco-plugin-ticketing/migrations/sqlite/0009_ticketing_knowledge_csat.sql
  - plugins/minco-plugin-ticketing/openapi/openapi.yaml
  - plugins/minco-plugin-ticketing/src/generated.rs
  - plugins/minco-plugin-ticketing/src/http.rs
  - plugins/minco-plugin-ticketing/src/model.rs
  - plugins/minco-plugin-ticketing/src/persistence.rs
  - plugins/minco-plugin-ticketing/src/plugin.rs
  - plugins/minco-plugin-ticketing/src/service.rs
  - plugins/minco-plugin-ticketing/src/store.rs
  - tasks/M14/M14-T67-bounded-search-knowledge-csat.md
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

# M14-T67 - Bounded search, knowledge links and one-shot CSAT

Stage E slice 4 — closes Stage E. Bounded substring search, bounded
revision-gated knowledge links, and the requester's one-shot
satisfaction rating.

## Goal

- `GET /agent/search?q=`: 2..=200 trimmed characters, no control
  characters; LIKE-wildcard escaping (`ESCAPE '\'`) so queries stay
  literals; subject + display reference + description only, never
  message bodies; ordinary summary pagination.
- `PUT /agent/tickets/{id}/knowledge-links`: ≤16 links, unique bounded
  article ids, https URLs, full-list replacement gated by If-Match.
- `POST /requester/tickets/{id}/csat`: requester-owned, resolved/closed
  only, exactly once, score 1..=5 + optional bounded comment; immutable
  after write.
- SQLite migration 0009 (`knowledge_links_json`, `csat_json`); Ticket
  and RequesterTicket schemas extended; boundary re-synced; descriptor
  and distribution inventories aligned.

## Non-goals

- Full-text/ranked search, a knowledge-base plugin or article store, CSAT
  aggregation/dashboards, surveys (Stage E stays small-helpdesk).

## Evidence

Run 2026-08-26 in the `minco-task-m14-t67` workspace:

- `cargo test -p minco-plugin-ticketing --all-features` — ok,
  **97 passed** (new: service search matrix incl. fail-closed short
  query; atomic link replacement incl. duplicate-article refusal; CSAT
  one-shot/ownership/state matrix; sqlite search parity incl.
  literal-% escaping).
- `cargo clippy ... -D warnings` clean; `cargo fmt --all -- --check`
  clean; generated boundary current.
- Evidence chain: static/publish validation, source manifest stable,
  baseline re-bound, operational evidence PASS, deep review rerun.
- `cargo minco check --with-cargo` — result recorded at finish.
