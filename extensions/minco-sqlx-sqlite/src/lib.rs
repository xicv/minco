//! `SQLx` `SQLite` pools with explicit file-backed versus in-memory behavior.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
pub use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::{path::Path, str::FromStr, time::Duration};
use thiserror::Error;

pub mod plugin_adapters;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqlitePoolConfig {
    pub url: String,
    pub max_connections: u32,
    pub acquire_timeout_seconds: u64,
}

impl SqlitePoolConfig {
    pub fn file(path: impl AsRef<Path>) -> Self {
        Self {
            url: format!("sqlite://{}", path.as_ref().display()),
            max_connections: 4,
            acquire_timeout_seconds: 5,
        }
    }
    pub fn memory() -> Self {
        Self {
            url: "sqlite::memory:".into(),
            max_connections: 1,
            acquire_timeout_seconds: 5,
        }
    }
    pub fn is_memory(&self) -> bool {
        self.url == "sqlite::memory:" || self.url.contains("mode=memory")
    }
    pub fn validate(&self) -> Result<(), SqliteError> {
        if self.url.trim().is_empty() {
            return Err(SqliteError::InvalidConfig("database URL is empty".into()));
        }
        if self.max_connections == 0 {
            return Err(SqliteError::InvalidConfig(
                "max_connections must be at least 1".into(),
            ));
        }
        if self.is_memory() && self.max_connections != 1 {
            return Err(SqliteError::InvalidConfig(
                "in-memory SQLite requires exactly one pooled connection".into(),
            ));
        }
        Ok(())
    }
}

pub async fn connect(config: &SqlitePoolConfig) -> Result<SqlitePool, SqliteError> {
    config.validate()?;
    let mut options = SqliteConnectOptions::from_str(&config.url)?
        .create_if_missing(!config.is_memory())
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(config.acquire_timeout_seconds));
    if !config.is_memory() {
        options = options.journal_mode(SqliteJournalMode::Wal);
    }
    Ok(SqlitePoolOptions::new()
        .max_connections(config.max_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_seconds))
        .connect_with(options)
        .await?)
}

pub async fn migrate(pool: &SqlitePool, path: impl AsRef<Path>) -> Result<(), SqliteError> {
    let migrator = sqlx::migrate::Migrator::new(path.as_ref()).await?;
    migrator.run(pool).await?;
    Ok(())
}

pub async fn migrate_with_history_table(
    pool: &SqlitePool,
    path: impl AsRef<Path>,
    history_table: &'static str,
) -> Result<(), SqliteError> {
    validate_identifier(history_table, "migration history table")?;
    let mut migrator = sqlx::migrate::Migrator::new(path.as_ref()).await?;
    migrator.dangerous_set_table_name(history_table);
    migrator.run(pool).await?;
    Ok(())
}

pub async fn ready(pool: &SqlitePool) -> bool {
    matches!(
        sqlx::query_scalar::<_, i64>("SELECT 1")
            .fetch_one(pool)
            .await,
        Ok(1)
    )
}

fn validate_identifier(value: &str, description: &str) -> Result<(), SqliteError> {
    let mut bytes = value.bytes();
    let valid_start = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
    if !valid_start
        || value.len() > 63
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(SqliteError::InvalidConfig(format!(
            "{description} must be a SQLite identifier of at most 63 ASCII characters"
        )));
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum SqliteError {
    #[error("invalid SQLite configuration: {0}")]
    InvalidConfig(String),
    #[error("SQLite error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("SQLite migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memory_profile_rejects_multiple_connections() {
        let mut config = SqlitePoolConfig::memory();
        config.max_connections = 2;
        assert!(config.validate().is_err());
    }

    #[tokio::test]
    async fn migration_history_table_rejects_dynamic_sql_tokens() {
        let pool = connect(&SqlitePoolConfig::memory())
            .await
            .expect("in-memory pool");
        let result =
            migrate_with_history_table(&pool, Path::new("missing"), "_migrations;DROP").await;
        assert!(matches!(result, Err(SqliteError::InvalidConfig(_))));
    }
}
