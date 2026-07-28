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

The framework lifecycle API accepts a validated `minco_db::MigrationSet` and
the same configuration used to open the pool:

```rust,no_run
# use minco_db::MigrationSet;
# use minco_sqlx_sqlite::{SqlitePool, SqlitePoolConfig};
# use std::path::Path;
# async fn lifecycle(
#   pool: &SqlitePool,
#   config: &SqlitePoolConfig,
#   project_root: &Path,
#   set: &MigrationSet,
# ) -> Result<(), minco_sqlx_sqlite::SqliteError> {
let before = minco_sqlx_sqlite::migration_target_state(pool, set).await?;
minco_sqlx_sqlite::apply_migration_set(pool, config, project_root, set).await?;
let missing = minco_sqlx_sqlite::verify_migration_tables(pool, set).await?;
assert!(before.dirty_version.is_none());
assert!(missing.is_empty());
# Ok(()) }
```

Execution requires file-backed SQLite. The adapter revalidates backend, SQL
identifiers, project containment and SQLx checksums, then holds an adjacent
operating-system file lock for the whole migration run.

Validated `minco_db::SeedPlan` values retain SQLite-specific SQL. A required
plan uses one transaction and source digests are rechecked before mutation.
`verify_seed_plan` uses a connection-local SQLite read-only guard and closes
that guarded connection instead of returning its state to the pool.
