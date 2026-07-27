---
id: M6-T08
title: Isolate PostgreSQL and SQLite SQLx feature graphs
milestone: M6
status: active
priority: high
area: persistence/features
depends_on: [M6-T07]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - plugins/minco-plugin-feedback/Cargo.toml
  - extensions/minco-sqlx-postgres/Cargo.toml
  - extensions/minco-sqlx-sqlite/Cargo.toml
  - examples/orders/adapters/Cargo.toml
  - examples/orders/service/Cargo.toml
  - scripts/test/sqlx_feature_isolation.sh
  - scripts/quality.sh
  - CHANGELOG.md
  - docs/architecture/capability-audit.md
  - tasks/M6/M6-T08-sqlx-feature-isolation.md
  - verification/source-manifest.json
checks:
  - bash scripts/test/sqlx_feature_isolation.sh
  - cargo check -p minco-plugin-feedback --no-default-features --features postgres --locked
  - cargo check -p minco-plugin-feedback --no-default-features --features sqlite --locked
  - cargo test -p minco-plugin-feedback --no-default-features --features postgres --locked
  - cargo test -p minco-plugin-feedback --no-default-features --features sqlite --locked
  - cargo check -p orders-adapters --no-default-features --features postgres --locked
  - cargo check -p orders-adapters --no-default-features --features sqlite --locked
  - cargo check -p orders-service --no-default-features --features postgres --locked
  - cargo check -p orders-service --no-default-features --features sqlite --locked
  - ./scripts/quality.sh
---

## Goal

Ensure a PostgreSQL-only Minco consumer does not compile SQLx SQLite or
`libsqlite3-sys`, and a SQLite-only consumer does not compile SQLx PostgreSQL.
Keep the all-feature workspace capable of exercising both backends.

## Why

The first real CGSP Feedback pilot selected only the `postgres` feature but
found that Minco 0.3.0's workspace-level SQLx feature set also activated SQLite
compile-time dependencies. Runtime behavior remained PostgreSQL-only, but the
resolved dependency graph did not honor Minco's small, explicit feature
boundary.

## Design boundary

- keep SQLx runtime, TLS, UUID, chrono, JSON, and migration support in the
  workspace's shared dependency contract;
- move `postgres` and `sqlite` activation to the exact adapter/plugin Cargo
  features that require them;
- retain one SQLx version and Cargo feature unification when both databases are
  deliberately selected;
- make both the Orders adapter and service dependencies feature-gated so the
  reference application proves the complete consumer graph;
- do not change persistence traits, migrations, database behavior, or public
  Rust APIs;
- do not represent DynamoDB as a relational SQLx backend.

## Acceptance

- Feedback `postgres` contains `sqlx-postgres` and excludes `sqlx-sqlite` plus
  `libsqlite3-sys`;
- Feedback `sqlite` contains `sqlx-sqlite` and excludes `sqlx-postgres`;
- official PostgreSQL and SQLite extensions retain only their selected backend;
- the complete Orders adapter and service graphs retain the same isolation;
- focused compile/test gates and the complete quality gate pass on the pinned
  toolchain;
- `Cargo.lock` is unchanged unless Cargo proves a legitimate resolution change;
- source-manifest and package dry-run evidence are refreshed before completion.

## Current state

The source correction and a deterministic `cargo tree` regression are present
on the draft branch. This task remains active until a Rust-enabled local Codex
workspace executes every acceptance command and refreshes exact-source evidence.
