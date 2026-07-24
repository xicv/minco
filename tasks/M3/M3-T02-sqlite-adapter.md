---
id: M3-T02
title: Verify SQLite idempotent order persistence
milestone: M3
status: active
priority: high
area: persistence/sqlite
depends_on: [M1-T01]
operations: [placeOrder, getOrder]
owned_paths:
  - extensions/minco-sqlx-sqlite/**
  - examples/orders/adapters/src/sqlite.rs
  - examples/orders/migrations/sqlite/**
checks:
  - cargo test -p minco-sqlx-sqlite -p orders-adapters --features orders-adapters/sqlite
---

## Goal

Support a file-backed local database with foreign keys, WAL, bounded pooling and the same declared use-case behavior as PostgreSQL.
