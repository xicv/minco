---
id: M9-T04
title: Add classified safe seeders and deterministic fixtures
milestone: M9
status: planned
priority: critical
area: persistence/seeding
depends_on: [M9-T03]
operations: []
owned_paths:
  - crates/minco-db/**
  - crates/minco-test/**
  - crates/minco-cli/**
  - extensions/minco-sqlx-postgres/**
  - extensions/minco-sqlx-sqlite/**
  - examples/orders/seeds/**
  - docs/adrs/**
  - docs/deployment/**
  - tasks/M9/M9-T04-safe-seeders.md
checks:
  - cargo test -p minco-db -p minco-test -p cargo-minco --all-features --locked
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
