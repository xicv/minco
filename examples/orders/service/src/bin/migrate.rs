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
        "memory" => bail!("memory storage has no migrations"),
        other => bail!("unsupported DATABASE_KIND {other}"),
    }
}

#[cfg(feature = "sqlite")]
async fn migrate_sqlite() -> Result<()> {
    let path = std::env::var("SQLITE_PATH").unwrap_or_else(|_| "target/minco/orders.db".into());
    if let Some(parent) = std::path::Path::new(&path).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let pool = minco_sqlx_sqlite::connect(&SqlitePoolConfig::file(path)).await?;
    minco_sqlx_sqlite::migrate(&pool, "examples/orders/migrations/sqlite").await?;
    pool.close().await;
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
    let pool = minco_sqlx_postgres::connect(&PostgresPoolConfig::serverless(url)).await?;
    minco_sqlx_postgres::migrate(&pool, "examples/orders/migrations/postgres").await?;
    pool.close().await;
    Ok(())
}

#[cfg(not(feature = "postgres"))]
async fn migrate_postgres() -> Result<()> {
    bail!("the postgres feature is disabled")
}
