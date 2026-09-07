---
id: M14-T70
title: Standalone Minco Desk example application
milestone: M14
status: active
priority: high
area: examples/desk
depends_on: [M14-T69]
operations: []
owned_paths:
  - Cargo.toml
  - docs/DECISIONS.md
  - docs/adrs/0072-standalone-desk-example.md
  - examples/minco-desk/Cargo.toml
  - examples/minco-desk/src/lib.rs
  - examples/minco-desk/src/bin/local.rs
  - examples/minco-desk/src/bin/migrate.rs
  - examples/minco-desk/tests/standalone_proof.rs
  - examples/recipes.toml
  - plugins/minco-plugin-ticketing/src/http.rs
  - tasks/M14/M14-T70-standalone-desk-example.md
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

# M14-T70 - Standalone Minco Desk example application

Stage G slice 1: the composition everything else in Stage G builds on.

## Goal

- `examples/minco-desk` workspace crate: `minco-desk-local` (one native
  process, one `SQLite` database, zero provider contact) and
  `minco-desk-migrate` (clean install + migration in one command,
  idempotent).
- Explicit composition root: ticketing on one pool with the released
  jobs store and registered handlers (ingest-only worker identity),
  memory objects and notifications; plugin graph with health,
  observability, identity, sessions, idempotency, notifications,
  events, audit selected explicitly; health checks over the live pool.
- In-process proofs: clean install creates every table (ticketing
  through migration 0011, jobs storage), re-migration is idempotent,
  the public support entry and authenticated agent bootstrap serve
  through the full stack, and an end-to-end ticket lifecycle (create →
  search → collision-aware detail) runs on one database.
- Correctness closure found by the proof: agent search fed its own `q`
  into the pagination parser (unknown-parameter rejection → 422 on
  every real search). Fixed: `q` is consumed before delegation.

## Non-goals

- Upgrade/backup/restore, retention, email replay/DLQ, job recovery,
  load, accessibility, security review, cost topology, PeoplePlanner
  BFF, separate database/release identity (later Stage G slices);
  PostgreSQL profile; static site composition.

## Evidence

Run 2026-08-26 in the `minco-task-m14-t70` workspace:

- `cargo test -p minco-desk-example` — ok, 3 proof tests green
  (clean-install tables + idempotent migrations; composition graph +
  public entry + agent bootstrap; end-to-end lifecycle).
- `cargo test -p minco-plugin-ticketing --all-features` — ok, 103
  passed (search fix covered by the existing search suite).
- `cargo clippy -p minco-desk-example -p minco-plugin-ticketing
  --all-targets --locked -- -D warnings` — clean; fmt clean.
- Evidence chain: static/publish validation, source manifest stable,
  baseline re-bound, operational evidence PASS, deep review rerun.
- `cargo minco check --with-cargo` — result recorded at finish.
