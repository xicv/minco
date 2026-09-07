---
id: M14-T73
title: Desk operational readiness — load, cost, BFF boundary, database identity
milestone: M14
status: active
priority: high
area: examples/desk
depends_on: [M14-T72]
operations: []
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0075-desk-operational-readiness-proofs.md
  - examples/minco-desk/tests/operational_readiness_proofs.rs
  - tasks/M14/M14-T73-desk-operational-readiness-proofs.md
  - verification/1.9-performance-baseline.json
  - verification/source-manifest.json
  - verification/operational-evidence-validation.json
  - verification/static-validation.json
  - verification/publish-validation.json
  - verification/deep-review.json
checks:
  - cargo test -p minco-desk-example --locked
  - cargo clippy -p minco-desk-example --all-targets --locked -- -D warnings
  - cargo minco check --with-cargo
---

# M14-T73 - Desk operational readiness — load, cost, BFF boundary, database identity

Stage G slice 4: the final local-profile evidence completing the
standalone private-beta claim.

## Goal

- Bounded load: 100 sequential creates + interleaved listing; full
  cursor walk collects exactly the corpus; local per-ticket latency
  envelope (<500ms) guards against quadratic behavior.
- Cost topology: composition declares no provisioned concurrency, no
  NAT gateway, no scheduled wakeups; database is a local SQLite file.
- BFF boundary: a BFF service identity reads the agent surface;
  foreign-origin CORS preflights are never echoed; wildcards forbidden.
- Database identity: two desks with different database URLs carry
  fully isolated data.

## Non-goals

- Hosted Linux performance qualification (no provider contact);
  editing PeoplePlanner (forbidden by the continuation prompt);
  PostgreSQL profile.

## Evidence

Run 2026-08-26 in the `minco-task-m14-t73` workspace:

- `cargo test -p minco-desk-example` — ok, 13 tests green (9 prior +
  4 new: load/pagination, cost topology, BFF boundary, DB identity).
- `cargo clippy ... -D warnings` clean; fmt clean.
- Evidence chain: static/publish validation, source manifest stable,
  baseline re-bound, operational evidence PASS, deep review rerun.
- `cargo minco check --with-cargo` — result recorded at finish.
