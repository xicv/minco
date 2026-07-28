---
id: M9-T03
title: Add database status plan migrate and verification
milestone: M9
status: complete
priority: critical
area: persistence/lifecycle
depends_on: [M9-T02]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - minco.toml
  - README.md
  - docs/DECISIONS.md
  - crates/minco-db/**
  - crates/minco-cli/**
  - crates/minco/**
  - extensions/minco-sqlx-postgres/**
  - extensions/minco-sqlx-sqlite/**
  - examples/orders/migrations/**
  - roadmap/tasks.mmd
  - scripts/test/generated_apps.sh
  - scripts/test/scaffold_templates.py
  - scripts/aws/create-temp-rds.sh
  - scripts/aws/run-bounded-smoke.sh
  - scripts/dev/migrate.sh
  - docs/adrs/**
  - docs/deployment/**
  - docs/development/using-minco-crate.md
  - docs/reference/cli.md
  - tasks/M9/M9-T03-database-lifecycle.md
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/publish-validation.json
  - verification/repository-truth.toml
  - verification/rust-dependency-hygiene.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p minco-db -p minco-sqlx-postgres -p minco-sqlx-sqlite -p cargo-minco --all-features --locked
  - cargo clippy -p minco-db -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco db status
  - cargo minco db plan
  - cargo minco db verify
---

## Goal

Model migration identity, ownership, ordering, digest, state, drift,
destructive risk, direct migration connection, advisory lock, verification, and
execution receipt for PostgreSQL and SQLite.

## Acceptance

- status, plan, migrate, and verify are separate commands with JSON output;
- production startup never migrates;
- concurrent migration attempts are locked safely;
- plugin and application histories remain attributable;
- irreversible SQL is reported honestly rather than given a fake rollback;
- deployment receipts can bind the exact migration plan and result.

## Non-goals

- one least-common-denominator database abstraction;
- automatic production rollback;
- DynamoDB table evolution through relational migration semantics.

## Review corrections

- The original task ownership omitted `Cargo.toml`, `Cargo.lock`, and
  `minco.toml`, even though the required `minco-db` workspace crate and
  migration-set inventory cannot exist without those files. They are now
  explicit owned paths.
- Existing migration SQL is immutable release state. Lifecycle metadata must be
  carried by sidecar manifests so adding risk, ownership, verification, or
  dependency metadata cannot change an already-applied SQLx checksum.

## Evidence

Completed on 2026-07-28 in the isolated `minco-task-m9-t03` JJ workspace
against merged-main parent `d4c4dd96`.

- The focused all-feature test and strict-Clippy commands passed for
  `minco-db`, the facade, both SQLx adapters, and `cargo-minco`. Regressions
  cover required risk metadata, deterministic dependency closure, duplicate
  identities and history tables, cycles, checksum drift, orphaned history,
  dirty state, identifier validation, path and symlink containment, receipt
  containment, and rejection of direct database URL arguments.
- A disposable file-backed SQLite database passed pending status, exact-digest
  migration with a create-new receipt, and post-migration table verification.
  The SQLite adapter also proved that in-memory targets are refused and a
  second process cannot bypass the database-adjacent file lock, including by
  reaching the same database through a filesystem symlink.
- A disposable PostgreSQL 18 container passed the adapter suite with two
  concurrent lifecycle attempts, then the public CLI plan, exact-digest
  migrate, receipt, and verify sequence. The receipt contained the plan and
  source identities, before/after state, newly observed applied versions, and
  verification result without a database URL. A same-named view was rejected
  as a verification table. The container, database, volume, network, and test
  receipt were removed afterward.
- Both generated PostgreSQL and SQLite projects passed template validation and
  generated-project checks. `cargo package -p minco-db --allow-dirty --locked`
  packaged and compiled the new crate; package listings proved the hidden
  sidecar manifests are included in both SQLx adapter archives. No crate was
  uploaded, no release was created, and no AWS resource was touched.
- Exact-source native ARM64 Orders and SQS worker ZIPs were rebuilt locally for
  the adoption report. The final clean cross-release build observations were
  118 seconds for Orders and 14 seconds for the worker.
- Current SQLx 0.9.0 migration behavior was checked against the exact locally
  resolved upstream source. The preferred Context CLI could not load its native
  module because it was built for Node ABI 137 while the active runtime expects
  ABI 147, and Context Hub had no SQLx package; this documentation-tooling
  limitation was not treated as product verification.
