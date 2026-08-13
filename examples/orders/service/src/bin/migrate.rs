#[cfg(feature = "postgres")]
use anyhow::Context;
use anyhow::{Result, bail};
#[cfg(feature = "postgres")]
use minco_sqlx_postgres::PostgresPoolConfig;
#[cfg(feature = "sqlite")]
use minco_sqlx_sqlite::SqlitePoolConfig;

#[tokio::main]
async fn main() -> Result<()> {
    let kind = std::env::var("DATABASE_KIND").unwrap_or_else(|_| "sqlite".into());
    match kind.as_str() {
        "sqlite" => migrate_sqlite().await,
        "postgres" => migrate_postgres().await,
        "dynamodb" => bail!(
            "DynamoDB tables are deployed from the explicit Plan; no runtime migration is supported"
        ),
        "memory" => bail!("memory storage has no migrations"),
        other => bail!("unsupported DATABASE_KIND {other}"),
    }
}

#[cfg(feature = "sqlite")]
async fn migrate_sqlite() -> Result<()> {
    let path = std::env::var("SQLITE_PATH").unwrap_or_else(|_| "target/minco/orders.db".into());
    let audit_path = std::env::var("AUDIT_SQLITE_PATH")
        .unwrap_or_else(|_| "target/minco/orders-audit.db".into());
    if path == audit_path {
        bail!("AUDIT_SQLITE_PATH must identify a distinct SQLite file");
    }
    if let Some(parent) = std::path::Path::new(&path).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if let Some(parent) = std::path::Path::new(&audit_path).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let pool = minco_sqlx_sqlite::connect(&SqlitePoolConfig::file(path)).await?;
    minco_sqlx_sqlite::migrate(&pool, "examples/orders/migrations/sqlite").await?;
    minco_sqlx_sqlite::migrate_with_history_table(
        &pool,
        "extensions/minco-sqlx-sqlite/migrations/plugins",
        "_minco_plugin_migrations",
    )
    .await?;
    let audit_pool = minco_sqlx_sqlite::connect(&SqlitePoolConfig::file(audit_path)).await?;
    minco_sqlx_sqlite::audit_v2::validate_separate_audit_pools(&pool, &audit_pool).await?;
    minco_sqlx_sqlite::audit_v2::migrate_audit_ledger(&audit_pool).await?;
    pool.close().await;
    audit_pool.close().await;
    Ok(())
}

#[cfg(not(feature = "sqlite"))]
async fn migrate_sqlite() -> Result<()> {
    bail!("the sqlite feature is disabled")
}

#[cfg(feature = "postgres")]
async fn migrate_postgres() -> Result<()> {
    let url = std::env::var("MIGRATION_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .context("MIGRATION_DATABASE_URL or DATABASE_URL is required")?;
    let audit_url = std::env::var("AUDIT_MIGRATION_DATABASE_URL")
        .or_else(|_| std::env::var("AUDIT_DATABASE_URL"))
        .context("AUDIT_MIGRATION_DATABASE_URL or AUDIT_DATABASE_URL is required")?;
    if url == audit_url {
        bail!("the audit migration URL must identify a distinct PostgreSQL database");
    }
    let pool = minco_sqlx_postgres::connect(&PostgresPoolConfig::serverless(url)).await?;
    minco_sqlx_postgres::migrate(&pool, "examples/orders/migrations/postgres").await?;
    minco_sqlx_postgres::migrate_with_history_table(
        &pool,
        "extensions/minco-sqlx-postgres/migrations/plugins",
        "_minco_plugin_migrations",
    )
    .await?;
    let audit_pool =
        minco_sqlx_postgres::connect(&PostgresPoolConfig::serverless(audit_url)).await?;
    minco_sqlx_postgres::audit_v2::validate_separate_audit_pools(&pool, &audit_pool).await?;
    minco_sqlx_postgres::audit_v2::migrate_audit_ledger(&audit_pool).await?;
    pool.close().await;
    audit_pool.close().await;
    Ok(())
}

#[cfg(not(feature = "postgres"))]
async fn migrate_postgres() -> Result<()> {
    bail!("the postgres feature is disabled")
}
