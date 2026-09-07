---
id: M14-T72
title: Desk security, accessibility and upgrade proofs
milestone: M14
status: active
priority: high
area: examples/desk
depends_on: [M14-T71]
operations: []
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0074-desk-security-accessibility-upgrade-proofs.md
  - examples/minco-desk/Cargo.toml
  - examples/minco-desk/tests/security_accessibility_upgrade_proofs.rs
  - plugins/minco-plugin-ticketing/migrations/sqlite/0002_ticketing_agent_summary.sql
  - plugins/minco-plugin-ticketing/migrations/sqlite/0004_ticketing_columnar_authority.sql
  - plugins/minco-plugin-ticketing/src/http.rs
  - tasks/M14/M14-T72-desk-security-accessibility-upgrade-proofs.md
  - verification/1.9-performance-baseline.json
  - verification/source-manifest.json
  - verification/operational-evidence-validation.json
  - verification/static-validation.json
  - verification/publish-validation.json
  - verification/deep-review.json
checks:
  - cargo test -p minco-desk-example --locked
  - cargo test -p minco-plugin-ticketing --all-features --locked
  - cargo clippy -p minco-desk-example -p minco-plugin-ticketing --all-targets --locked -- -D warnings
  - cargo minco check --with-cargo
---

# M14-T72 - Desk security, accessibility and upgrade proofs

Stage G slice 3: the security surface, requester isolation and additive
schema upgrade on the desk composition.

## Goal

- Hardened public surfaces: strict CSP + nosniff + no-referrer on the
  agent console; exact content types with nosniff on every script and
  stylesheet; the bootstrap carries no secret material. The public
  support-entry script was missing nosniff/no-referrer — fixed by
  routing through `hardened_asset`.
- Requester isolation on the composition: anonymous agent bootstrap is
  401; a stranger reading another requester's ticket is 404/403.
- Upgrade from the first-generation schema preserves data: hand-apply
  0001, create real data, record the migration as applied (correct
  SHA-384 checksum), run the current migrator, prove the ticket
  survives and the upgraded database serves through the full stack.

## Correctness closures found by the proofs

- The public support-entry script lacked `X-Content-Type-Options:
  nosniff` and `Referrer-Policy: no-referrer` (fixed via
  `hardened_asset`).
- Migration backfills in 0002 and 0004 used `json_extract` without
  COALESCE on NOT NULL columns — any first-generation row whose JSON
  predated those fields crashed the upgrade with a constraint failure
  (both fixed with COALESCE to column defaults).

## Non-goals

- Load/performance, cost topology, PeoplePlanner BFF, separate
  database/release identity (later Stage G slices).

## Evidence

Run 2026-08-26 in the `minco-task-m14-t72` workspace:

- `cargo test -p minco-desk-example` — ok, 9 tests green (6 prior +
  3 new: hardened surfaces, isolation, upgrade).
- `cargo test -p minco-plugin-ticketing --all-features` — ok, 103
  passed.
- `cargo clippy ... -D warnings` clean; fmt clean.
- Evidence chain: static/publish validation, source manifest stable,
  baseline re-bound, operational evidence PASS, deep review rerun.
- `cargo minco check --with-cargo` — result recorded at finish.
