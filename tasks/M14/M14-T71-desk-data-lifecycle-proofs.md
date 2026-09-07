---
id: M14-T71
title: Desk data-lifecycle proofs — backup, restore, retention, job recovery
milestone: M14
status: active
priority: high
area: examples/desk
depends_on: [M14-T70]
operations: []
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0073-desk-data-lifecycle-proofs.md
  - examples/minco-desk/src/lib.rs
  - examples/minco-desk/tests/data_lifecycle_proofs.rs
  - plugins/minco-plugin-ticketing/src/persistence.rs
  - plugins/minco-plugin-ticketing/src/store.rs
  - tasks/M14/M14-T71-desk-data-lifecycle-proofs.md
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

# M14-T71 - Desk data-lifecycle proofs — backup, restore, retention, job recovery

Stage G slice 2: the operational data-lifecycle evidence on the
standalone composition.

## Goal

- Backup via SQLite online `VACUUM INTO` (AssertSqlSafe-wrapped; path
  from tempfile, never request input); restore = compose a fresh desk
  on the backup file and serve every prior ticket through the full
  HTTP stack.
- Retention erasure: `erase_tickets_resolved_before(cutoff, limit)`
  store port (SQL oldest-first bounded DELETE with FK cascade; memory
  parity) exposed as `erase_resolved_before` on the desk; the proof
  verifies exactly the resolved ticket disappears, the open ticket
  survives, and child rows cascade.
- Job recovery across a simulated process death: first process
  composes and submits three durable jobs then is dropped (pool
  included); the second process finds all three still pending and a
  single `dispatch_due_once` claim pass recovers every one.

## Non-goals

- PostgreSQL backup (own tooling, own proof when that profile ships);
  per-subject GDPR erasure (separate decision); scheduled/automatic
  retention (explicit operator operation); upgrade across release
  versions; load; accessibility; security; cost; BFF.

## Evidence

Run 2026-08-26 in the `minco-task-m14-t71` workspace:

- `cargo test -p minco-desk-example` — ok, 6 tests green (3 T70
  proofs + 3 new lifecycle proofs).
- `cargo test -p minco-plugin-ticketing --all-features` — ok, 103
  passed (store port additive).
- `cargo clippy ... -D warnings` clean; fmt clean.
- Evidence chain: static/publish validation, source manifest stable,
  baseline re-bound, operational evidence PASS, deep review rerun.
- `cargo minco check --with-cargo` — result recorded at finish.
