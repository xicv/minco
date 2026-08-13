---
id: M14-T27
title: Add the AWS-default DynamoDB audit ledger
milestone: M14
status: done
priority: critical
area: adapters/audit/aws
depends_on: [M14-T26]
operations: []
owned_paths:
  - Cargo.lock
  - extensions/minco-aws-dynamodb/Cargo.toml
  - extensions/minco-aws-dynamodb/README.md
  - extensions/minco-aws-dynamodb/minco-plugin.json
  - extensions/minco-aws-dynamodb/src/audit_v2.rs
  - extensions/minco-aws-dynamodb/src/lib.rs
  - extensions/minco-aws-dynamodb/tests/**
  - crates/minco-plan/src/model.rs
  - crates/minco-plan/src/sam.rs
  - crates/minco-plan/src/cost.rs
  - crates/minco-plan/tests/**
  - docs/reference/generated/diagnostics.md
  - docs/reference/generated/plugins.md
  - docs/reference/generated/schemas.md
  - docs/research/audit-ledger-costs-2026-08.md
  - roadmap/tasks.mmd
  - tasks/M14/M14-T27-dynamodb-audit-ledger.md
  - verification/cost-regression-baseline.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p minco-aws-dynamodb --all-features --locked
  - cargo clippy -p minco-aws-dynamodb --all-targets --all-features --locked -- -D warnings
  - cargo test -p minco-plan --all-features --locked
  - cargo clippy -p minco-plan --all-targets --all-features --locked -- -D warnings
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/source_manifest.py --check
---

## Goal

Implement ADR-0043's AWS-default profile: a physically separate on-demand
DynamoDB audit table can participate in the operational DynamoDB transaction,
serve schema-agnostic resource history, expose bounded storage health and render
explicit Plan/SAM/IAM/cost consequences.

## Acceptance

- the audit table stores one canonical immutable event plus bounded direct and
  related-resource query projections using fixed `pk`/`sk` string keys;
- callers can add source mutations and audit puts to one `TransactWriteItems`
  request, and provider batch append is idempotent after ambiguous outcomes;
- total transaction items and bytes are validated before provider contact;
- direct and relationship history uses opaque V2 cursors and never reads the
  operational table;
- the Plan renders a retained, encrypted, point-in-time-recoverable audit table,
  its environment variable and least-privilege table IAM separately from the
  operational table;
- the health snapshot reports provider table size/item count without exposing
  provider errors, with the documented metric freshness limitation; and
- the current Sydney cost model accounts for transactional writes, record size,
  projection fan-out, reads, storage and PITR rather than calling the table
  infinite or free.

## Non-goals

- Orders application/domain adoption (M14-T28);
- SQL rotation/archive execution;
- DynamoDB Streams, Lambda relays, database triggers or hidden schedules;
- automatic TTL deletion; or
- cross-account or cross-region transactions.

## Evidence

Record adapter serialization/idempotency/query/health tests, Plan/SAM/IAM
snapshots, strict Clippy, static validation and source-manifest verification.
Rustack behavioral proof remains explicit and conditional on disposable source
and audit table environment variables.

## Recorded evidence

- `cargo test -p minco-aws-dynamodb --all-features --locked`: four ledger unit
  tests and three provider tests passed; the two-table Rustack transaction test
  compiled and remained explicitly ignored without disposable table inputs.
- `scripts/dev/rustack-dynamodb-smoke.sh` passed both ignored behavioral tests
  against disposable source and audit tables, then absence-verified both tables
  and the pinned Rustack container/network as removed.
- `cargo test -p minco-plan --all-features --locked`: 27 unit and 53
  multi-runtime Plan/SAM/IAM tests passed.
- Strict all-target/all-feature Clippy passed for both crates with `-D warnings`.
- `cargo check --workspace --all-targets --all-features --locked` passed,
  including the Orders DynamoDB adapter and all consumers of the additive Plan
  model.
- Security review: resource identities are hashed in keys, all provider errors
  and table identifiers remain redacted, record and transaction bounds fail
  before provider contact, writes are conditional/immutable, audit IAM is
  table-scoped, and the audit table is retained, encrypted, PITR-enabled and
  deletion-protected.
- Sydney rates were refreshed from the AWS Price List API on 2026-08-13 and the
  research note prices canonical/projection fan-out plus PITR explicitly.
- The generated DynamoDB Plan and SAM snapshots are current and deterministic;
  hosted performance and current live-provider evidence remain `NOT RUN`.
