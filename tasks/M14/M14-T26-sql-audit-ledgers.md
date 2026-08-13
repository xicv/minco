---
id: M14-T26
title: Add transactional SQL audit journals and separate ledgers
milestone: M14
status: done
priority: critical
area: adapters/audit
depends_on: [M14-T25]
operations: []
owned_paths:
  - Cargo.lock
  - plugins/minco-plugin-audit/src/journal.rs
  - plugins/minco-plugin-audit/src/lib.rs
  - plugins/minco-plugin-audit/src/v2.rs
  - extensions/minco-sqlx-postgres/Cargo.toml
  - extensions/minco-sqlx-postgres/README.md
  - extensions/minco-sqlx-postgres/migrations/plugins/**
  - extensions/minco-sqlx-postgres/migrations/audit-ledger/**
  - extensions/minco-sqlx-postgres/src/audit_v2.rs
  - extensions/minco-sqlx-postgres/src/lib.rs
  - extensions/minco-sqlx-postgres/src/plugin_adapters.rs
  - extensions/minco-sqlx-sqlite/Cargo.toml
  - extensions/minco-sqlx-sqlite/README.md
  - extensions/minco-sqlx-sqlite/migrations/plugins/**
  - extensions/minco-sqlx-sqlite/migrations/audit-ledger/**
  - extensions/minco-sqlx-sqlite/src/audit_v2.rs
  - extensions/minco-sqlx-sqlite/src/lib.rs
  - extensions/minco-sqlx-sqlite/src/plugin_adapters.rs
  - roadmap/tasks.mmd
  - tasks/M14/M14-T26-sql-audit-ledgers.md
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p minco-plugin-audit --all-features --locked
  - cargo clippy -p minco-plugin-audit --all-targets --all-features --locked -- -D warnings
  - cargo test -p minco-sqlx-postgres --all-features --locked
  - cargo clippy -p minco-sqlx-postgres --all-targets --all-features --locked -- -D warnings
  - cargo test -p minco-sqlx-sqlite --all-features --locked
  - cargo clippy -p minco-sqlx-sqlite --all-targets --all-features --locked -- -D warnings
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/source_manifest.py --check
---

## Goal

Implement the SQL profile of ADR-0040: domain adapters can place a bounded V2
audit intent in the operational transaction, an explicit relay can claim and
deliver batches safely, and permanent queryable history lives in a distinct
PostgreSQL database or SQLite file.

## Acceptance

- provider-neutral journal and relay contracts use atomic claims, expiring
  leases, idempotent ledger batches, retry/quarantine classification and batch
  acknowledgement after ledger commit;
- PostgreSQL and SQLite expose transaction-aware `enqueue_in` helpers so a
  domain mutation and intent roll back or commit together;
- source migrations contain only bounded journal state while separate ledger
  migrations contain canonical records and related-resource projections;
- ledger batch append is atomic and treats same-ID/same-content as a duplicate
  while same-ID/different-content fails the entire batch;
- readers paginate direct and explicitly related resource history with the V2
  cursor and never join to an operational table;
- PostgreSQL claims are disjoint under concurrent workers and SQLite uses an
  immediate transaction to preserve its single-writer semantics;
- health reports journal backlog and ledger bytes, including filesystem free
  space for SQLite, without exposing database URLs or provider errors; and
- database tests prove rollback, retry-after-ambiguous-ack, query pagination,
  relationship gathering and migration separation when their explicit engines
  are available.

## Non-goals

- DynamoDB, Plan/SAM/IAM or AWS provider changes;
- Orders application or HTTP adoption;
- hidden schedules, automatic retention deletion, database triggers or CDC;
- cross-database distributed transactions; or
- migrating or deleting the legacy `minco_audit` table.

## Evidence

Record provider-neutral and adapter tests, strict Clippy, static validation and
source-manifest verification. PostgreSQL proof remains explicitly conditional
on both `MINCO_TEST_POSTGRES_URL` and `MINCO_TEST_POSTGRES_AUDIT_URL` resolving
to distinct databases; SQLite proof must run locally against distinct
file-backed source and ledger databases.

## Recorded evidence

- `cargo test -p minco-plugin-audit --all-features --locked`: 12 passed.
- `cargo test -p minco-sqlx-sqlite --all-features --locked`: 15 unit and 4
  integration tests passed against distinct temporary source/ledger files.
- `cargo test -p minco-sqlx-postgres --all-features --locked`: 11 unit and 2
  integration tests passed; the new two-database behavioral cases compiled and
  explicitly skipped because both PostgreSQL test URLs were unset.
- Strict all-target/all-feature Clippy passed for all three crates with
  `-D warnings`.
- `uv run --locked python scripts/validate_static.py`: status `ok`, zero errors
  and zero warnings.
- `uv run --locked python scripts/source_manifest.py --check`: verified.
- Security review: SQL values are bound, source/ledger identities fail closed,
  provider errors and database URLs are redacted, records are validated before
  persistence, and public worker/failure identifiers are bounded.
- `cargo minco check --with-cargo` passed static validation, 53 repository-truth
  tests, deployment assurance, eight deployment tests and five product-truth
  tests, then stopped at the pre-existing exact boundary `checked
  operational-evidence receipt is stale`. That receipt belongs to the active
  M14-T10 evidence task and was not rewritten here.
