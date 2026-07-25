---
id: M3-T02
title: Verify SQLite idempotent order persistence
milestone: M3
status: complete
priority: high
area: persistence/sqlite
depends_on: [M1-T01]
operations: [placeOrder, getOrder]
owned_paths:
  - extensions/minco-sqlx-sqlite/**
  - examples/orders/adapters/src/sqlite.rs
  - examples/orders/adapters/tests/sqlite.rs
  - examples/orders/migrations/sqlite/**
checks:
  - cargo test -p minco-sqlx-sqlite -p orders-adapters --features orders-adapters/sqlite
  - cargo clippy -p minco-sqlx-sqlite -p orders-adapters --features orders-adapters/sqlite --all-targets --locked -- -D warnings
---

## Goal

Support a file-backed local database with foreign keys, WAL, bounded pooling and the same declared use-case behavior as PostgreSQL.

## Evidence

On 2026-07-24, the scoped extension and adapter suites passed against real
file-backed SQLite databases. Behavioral tests prove replay of the original
result, fingerprint conflict rejection, concurrent single-commit behavior and
persistence across pool restarts. Orders also uses a validated
`_minco_orders_migrations` history table so independently versioned migration
sets cannot collide in SQLx's default table; unsafe dynamic table identifiers
are rejected before migration loading.
