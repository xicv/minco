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
