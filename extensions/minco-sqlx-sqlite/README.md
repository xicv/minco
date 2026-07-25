# minco-sqlx-sqlite

SQLx SQLite runtime support for Minco.

File-backed databases enable foreign keys and WAL. In-memory databases require
exactly one pooled connection so independent connections do not accidentally
observe separate databases.

```rust,no_run
use minco_sqlx_sqlite::{SqlitePoolConfig, connect};

# async fn open() -> Result<(), minco_sqlx_sqlite::SqliteError> {
let pool = connect(&SqlitePoolConfig::memory()).await?;
assert!(minco_sqlx_sqlite::ready(&pool).await);
# Ok(()) }
```

Applications that compose independently versioned migration sets must give
each set its own history table:

```rust,no_run
# use minco_sqlx_sqlite::SqlitePool;
# async fn migrate(pool: &SqlitePool) -> Result<(), minco_sqlx_sqlite::SqliteError> {
minco_sqlx_sqlite::migrate_with_history_table(
    pool,
    "migrations/orders",
    "_minco_orders_migrations",
)
.await?;
# Ok(()) }
```

The history-table name is restricted to a plain ASCII identifier. Reusing
SQLx's default `_sqlx_migrations` table for unrelated migration directories
causes version/checksum collisions.
