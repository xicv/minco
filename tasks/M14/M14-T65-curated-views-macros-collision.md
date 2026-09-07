---
id: M14-T65
title: Curated views, revision-aware saved replies and collision indication
milestone: M14
status: active
priority: high
area: plugins/ticketing
depends_on: [M14-T64]
operations: [listTicketingAgentView, listTicketingAgentMacros, createTicketingAgentMacro, updateTicketingAgentMacro]
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0067-curated-views-macros-collision.md
  - plugins/minco-plugin-ticketing/migrations/sqlite/0007_ticketing_views_macros.sql
  - plugins/minco-plugin-ticketing/minco-plugin.json
  - plugins/minco-plugin-ticketing/openapi/openapi.yaml
  - plugins/minco-plugin-ticketing/src/generated.rs
  - plugins/minco-plugin-ticketing/src/http.rs
  - plugins/minco-plugin-ticketing/src/model.rs
  - plugins/minco-plugin-ticketing/src/persistence.rs
  - plugins/minco-plugin-ticketing/src/plugin.rs
  - plugins/minco-plugin-ticketing/src/service.rs
  - plugins/minco-plugin-ticketing/src/store.rs
  - tasks/M14/M14-T65-curated-views-macros-collision.md
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

# M14-T65 - Curated views, revision-aware saved replies and collision indication

Stage E slice 2. Five curated agent views, a revision-aware shared
macro library, and advisory collision indication on the agent detail.

## Goal

- `GET /agent/views/{viewId}`: closed set (`new-unassigned`,
  `pending-requester`, `pending-internal`, `mine`, `recently-resolved`)
  over the existing summary pagination; unknown ids 404; summary filter
  gains an explicit `unassigned` flag (SQL + memory).
- Macros: project-scoped shared library; list is agent-read, create and
  full-replacement update require `ticketing.manage`; updates gated by
  `If-Match` strong tags; duplicate titles conflict; plain text stays
  editable before submission (application is client-side).
- Collision indication: agent detail records the viewer and returns
  `other_recent_viewers` (≤8, 5-minute window, never self); If-Match
  stays the collision authority.
- SQLite migration 0007 (views + macros with unique project/title);
  descriptor + distribution inventory carries the four new operations;
  generated boundary re-synced.

## Non-goals

- Assignment modes, deadline snapshots, bounded search, knowledge
  links, CSAT (later Stage E slices); realtime presence; a query DSL.

## Evidence

Run 2026-08-25 in the `minco-task-m14-t65` workspace:

- `cargo test -p minco-plugin-ticketing --all-features` — ok,
  **89 passed** (new end-to-end test: curated filtering + unknown view
  404, macro create/list/revision-conflict/duplicate-title, cross-agent
  viewer surfacing; the management etag test now also asserts the
  detail envelope).
- `cargo clippy ... -D warnings` clean; `cargo fmt --all -- --check`
  clean; generated boundary current; descriptor/distribution/contract
  inventories aligned.
- Evidence chain: static/publish validation, source manifest stable,
  baseline re-bound, operational evidence PASS, deep review rerun.
- `cargo minco check --with-cargo` — result recorded at finish.
