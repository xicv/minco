# Use persistent local SQLite

The SQLite profile gives one local process durable relational storage without a
database service. It is useful for a fast feedback loop and small single-host
deployments whose concurrency and backup requirements fit SQLite.

## Features

Enable only `sqlx-sqlite` for the adapter boundary. The reference service's
`sqlite` feature does not pull in PostgreSQL or AWS dependencies.

## Provider assumptions

The plan and tests are local. SQLite uses a local file; migrations remain an
explicit command and startup never migrates implicitly.

## Cost and wake behavior

The database is `storage_only`: the file and backups may consume storage while
the application is idle. There is no schedule or provider wake source. A local
process runs only when the developer starts it.

Inspect the selected topology before starting services:

```bash
cargo minco deploy plan --config examples/orders/config/minco.local-sqlite.toml --stdout --json
cargo test --locked -p orders-adapters --no-default-features --features sqlite
bash scripts/test/sqlx_feature_isolation.sh
```

For interactive development, select the `sqlite` development profile and run
the explicit migration/seed lifecycle described by `cargo minco db plan` and
`cargo minco db seed --dry-run` before any apply.

## Verification

The matrix executes `local-sqlite-plan`, `orders-sqlite`, and
`sqlx-isolation`. It proves the selected plan, real SQLite transaction behavior,
and absence of accidental PostgreSQL feature leakage.

## Unsupported gates

SQLite proof does not cover PostgreSQL locks, network partitions, multi-instance
writers, managed backups, failover, or production hosting. Those are separate
profile and operational decisions.
