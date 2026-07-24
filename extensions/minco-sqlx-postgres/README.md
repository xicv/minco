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
