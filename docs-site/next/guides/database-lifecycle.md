---
title: Migrations and Seeders
description: Plan, apply, and verify attributable database changes with explicit risk and preservation policy.
---

# Migrations and Seeders

Production database work is an explicit release operation. Lambda startup does
not migrate or seed, and source deployment never implies data mutation.

## Migration sets

Every migration root has a `.minco-migrations.toml` sidecar that records a
stable set ID, owner, backend, history table, verification tables, and
per-version risk.

```toml
schema = 1
id = "orders-postgres"
owner = "orders"
backend = "postgres"
history_table = "_minco_migrations"
verification_tables = ["orders", "order_lines"]

[[migrations]]
version = 1
name = "create_orders"
risk = "additive"
path = "0001_create_orders.sql"
```

The plan hashes the ordered files and metadata. An edited SQL file produces a
new digest and invalidates the previous approval.

## Inspect without mutation

```bash
cargo minco db plan --set orders-postgres --json
cargo minco db status --json
cargo minco db verify --json
```

Planning is offline. Status and verification are read-only database operations
when a connection is explicitly supplied.

## Apply an exact migration plan

```bash
cargo minco db plan \
  --set orders-postgres \
  --environment production \
  --json
```

After reviewing the target, history, ordered steps, risks, and digest, pass that
exact digest to the mutating command:

```bash
cargo minco db migrate \
  --set orders-postgres \
  --environment production \
  --approve-digest REVIEWED_DIGEST \
  --json
```

The runner records durable receipts. Target/environment mismatches, changed
files, missing approvals, or incompatible history fail before mutation.

## Classify seed data

Seed sets declare whether data is reference, demo, test fixture, or another
reviewed class, plus the allowed environments and preservation behavior.

```bash
cargo minco db seed \
  --profile demo \
  --environment local \
  --dry-run \
  --json
```

A mutating run requires the classification and environment gates to pass. Use
verification as a separate read-only stage:

```bash
cargo minco db seed --verify --json
```

## Choose an adapter

- `sqlx-postgres` supports bounded pools for PostgreSQL connection models such
  as Neon, self-hosted PostgreSQL, RDS, and Aurora; the deployment profile owns
  connection and cost assumptions.
- `sqlx-sqlite` supports local, desktop, and persistent single-process profiles
  with explicit durability limits.
- DynamoDB needs access-pattern-specific ports and adapters. Minco does not
  emulate relational SQL semantics over it.

Use real-engine behavioral tests for adapter claims. Compiler success alone is
not transaction, locking, migration, or provider evidence.
