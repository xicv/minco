---
id: M3-T01
title: Verify PostgreSQL idempotent order persistence
milestone: M3
status: complete
priority: high
area: persistence/postgres
depends_on: [M1-T01]
operations: [placeOrder, getOrder]
owned_paths:
  - extensions/minco-sqlx-postgres/**
  - examples/orders/adapters/src/postgres.rs
  - examples/orders/migrations/postgres/**
checks:
  - cargo test -p minco-sqlx-postgres -p orders-adapters --features orders-adapters/postgres
  - MINCO_ORDERS_TEST_POSTGRES_URL='<local-test-postgres-url>' cargo test -p orders-adapters --features postgres --test postgres -- --ignored
---

## Goal

Use bounded SQLx pools, explicit migrations and one atomic transaction to preserve order and idempotency invariants.

## Evidence

On 2026-07-24, the scoped extension and adapter suites passed against
PostgreSQL 18. Real-engine tests prove original-result replay, fingerprint
conflict rejection and concurrent single-commit behavior. The first real run
also found that independently versioned migration directories collided in
SQLx's default history table; Orders now uses a validated
`_minco_orders_migrations` table, and a regression test rejects unsafe dynamic
history-table identifiers.
