---
id: M9-T04
title: Add classified safe seeders and deterministic fixtures
milestone: M9
status: complete
priority: critical
area: persistence/seeding
depends_on: [M9-T03]
operations: []
owned_paths:
  - minco.toml
  - crates/minco-db/**
  - crates/minco-test/**
  - crates/minco-cli/**
  - extensions/minco-sqlx-postgres/**
  - extensions/minco-sqlx-sqlite/**
  - examples/orders/seeds/**
  - docs/DECISIONS.md
  - docs/adrs/**
  - docs/deployment/**
  - docs/reference/cli.md
  - verification/adoption-measurements.json
  - verification/source-manifest.json
  - verification/static-validation.json
  - verification/publish-validation.json
  - verification/deep-review.json
  - tasks/M9/M9-T04-safe-seeders.md
checks:
  - cargo test -p minco-db -p minco-test -p cargo-minco --all-features --locked
  - cargo test -p minco-sqlx-postgres -p minco-sqlx-sqlite --all-features --locked
  - cargo clippy -p minco-db -p minco-test -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco db seed --profile demo --dry-run
  - cargo minco db seed --verify
---

## Goal

Add `reference`, `demo`, `test`, and `bootstrap` seed plans with stable
identity/version, owner, dependencies, environment allowlist, idempotency,
mutable-state ownership, preservation rules, destructive risk, transaction
behavior, verification, digest, and receipts.

## Acceptance

- production demo/test seeding fails closed;
- bootstrap requires explicit environment authority and emits a receipt;
- PostgreSQL and SQLite conformance fixtures preserve backend differences;
- deterministic fixture builders do not require an ORM;
- data backfills remain migrations.

## Non-goals

- hiding destructive reset behind a normal seed command;
- production sample data;
- claiming independent stores share a transaction.

## Review corrections

- The original task ownership omitted the root seed inventory, decision
  register, CLI reference, and generated verification manifests even though
  the required repository-native workflow and quality gate update those files.
  Those bounded paths are now explicit.
- Production and bootstrap gates apply to the complete dependency closure, not
  only the requested root profile. This prevents a differently classified seed
  from hiding a demo/test or bootstrap dependency.

## Evidence

Completed on 2026-07-28 in the isolated `minco-task-m9-t04` JJ workspace
against merged-main parent `6fe9f74c`.

- The focused all-feature tests and strict-Clippy commands passed for
  `minco-db`, `minco-test`, `cargo-minco`, and both SQLx adapters. Regressions
  cover deterministic ordering and digests, duplicate or unknown identities,
  cycles, environment and production closure gates, hidden bootstrap
  dependencies, mixed transaction behavior, source drift before mutation,
  whole-plan rollback, idempotent reruns, exact-digest execution, create-new
  receipts, database URL redaction, and read-only verification.
- A disposable file-backed SQLite database passed the public CLI migration,
  reviewed seed plan, exact-digest execution, verified receipt, and target
  verification journey. SQLite adapter tests additionally proved rollback on a
  later seed failure and rejected mutating verification SQL under
  `PRAGMA query_only`.
- A disposable PostgreSQL 18 container passed the real adapter suite and the
  public CLI migration, reviewed seed plan, exact-digest execution, verified
  receipt, and target verification journey. A mutating verification query was
  rejected by `SET TRANSACTION READ ONLY`. Receipts and errors contained no
  database URL.
- Generated PostgreSQL and SQLite applications compiled and tested with their
  seed catalogs. Package listings proved the hidden seed sidecars, SQL,
  integration tests, and fixture tests are present in the affected crate
  archives. No crate was uploaded, no release was created, and no cloud or
  deployed database was touched.
- `./scripts/quality.sh` passed the complete repository gate, including
  formatting, workspace tests, both generated applications, rustdoc,
  dependency and license policy, RustSec advisories, secret scanning, static
  validation, publish validation, deep review, and source-manifest
  verification.
- Exact-source native ARM64 Orders and SQS worker ZIPs were rebuilt locally for
  the adoption report. Orders was 5,031,958 compressed bytes with SHA-256
  `23c05e30f52e45137e574a294922c00b7334dde21ec5b72879ea6dd9154660e5`;
  the worker was 573,415 compressed bytes with SHA-256
  `13febed2fa7f7858dc3ae0c5f00c624401cfe5f7f5234227b90dcab913d2b5ac`.
- SQLx 0.9.0 transaction, raw-SQL, and connection behavior was checked against
  the exact locally resolved upstream source. Context Hub had no SQLx package,
  so this documentation limitation was not treated as product verification.
