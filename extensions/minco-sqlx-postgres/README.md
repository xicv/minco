# minco-sqlx-postgres

Bounded SQLx PostgreSQL runtime support for Minco.

The serverless profile defaults to a small pool with zero minimum connections,
a bounded acquisition timeout, and an idle timeout. Runtime and migration
credentials should remain separate in deployed environments.

```rust,no_run
use minco_sqlx_postgres::{PostgresPoolConfig, connect};

# async fn open() -> Result<(), minco_sqlx_postgres::PostgresError> {
let config = PostgresPoolConfig::serverless("postgresql://localhost/app");
let pool = connect(&config).await?;
assert!(minco_sqlx_postgres::ready(&pool).await);
# Ok(()) }
```

Applications that compose independently versioned migration sets must give
each set its own history table:

```rust,no_run
# use minco_sqlx_postgres::PgPool;
# async fn migrate(pool: &PgPool) -> Result<(), minco_sqlx_postgres::PostgresError> {
minco_sqlx_postgres::migrate_with_history_table(
    pool,
    "migrations/orders",
    "_minco_orders_migrations",
)
.await?;
# Ok(()) }
```

The history-table name is restricted to a plain PostgreSQL identifier. Reusing
SQLx's default `_sqlx_migrations` table for unrelated migration directories
causes version/checksum collisions.

The framework lifecycle API accepts a validated `minco_db::MigrationSet`:

```rust,no_run
# use minco_db::MigrationSet;
# use minco_sqlx_postgres::PgPool;
# use std::path::Path;
# async fn lifecycle(
#   pool: &PgPool,
#   project_root: &Path,
#   set: &MigrationSet,
# ) -> Result<(), minco_sqlx_postgres::PostgresError> {
let before = minco_sqlx_postgres::migration_target_state(pool, set).await?;
minco_sqlx_postgres::apply_migration_set(pool, project_root, set).await?;
let missing = minco_sqlx_postgres::verify_migration_tables(pool, set).await?;
assert!(before.dirty_version.is_none());
assert!(missing.is_empty());
# Ok(()) }
```

The adapter revalidates backend, SQL identifiers, project containment and
resolved SQLx checksums immediately before execution. SQLx's PostgreSQL
advisory migration lock remains enabled for the whole run.
