---
id: M3-T01
title: Verify PostgreSQL idempotent order persistence
milestone: M3
status: active
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
---

## Goal

Use bounded SQLx pools, explicit migrations and one atomic transaction to preserve order and idempotency invariants.
