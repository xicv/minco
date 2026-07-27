---
id: M9-T03
title: Add database status plan migrate and verification
milestone: M9
status: planned
priority: critical
area: persistence/lifecycle
depends_on: [M9-T02]
operations: []
owned_paths:
  - crates/minco-db/**
  - crates/minco-cli/**
  - extensions/minco-sqlx-postgres/**
  - extensions/minco-sqlx-sqlite/**
  - examples/orders/migrations/**
  - docs/adrs/**
  - docs/deployment/**
  - tasks/M9/M9-T03-database-lifecycle.md
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
