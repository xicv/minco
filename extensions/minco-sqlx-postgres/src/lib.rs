//! Bounded `SQLx` `PostgreSQL` pools for local servers and serverless runtimes.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
pub use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::{path::Path, time::Duration};
use thiserror::Error;

pub mod plugin_adapters;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostgresPoolConfig {
    pub url: String,
    pub max_connections: u32,
    pub acquire_timeout_seconds: u64,
    pub idle_timeout_seconds: u64,
}

impl std::fmt::Debug for PostgresPoolConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PostgresPoolConfig")
            .field("url", &"[REDACTED DATABASE URL]")
            .field("max_connections", &self.max_connections)
            .field("acquire_timeout_seconds", &self.acquire_timeout_seconds)
            .field("idle_timeout_seconds", &self.idle_timeout_seconds)
            .finish()
    }
}

impl PostgresPoolConfig {
    pub fn serverless(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            max_connections: 2,
            acquire_timeout_seconds: 5,
            idle_timeout_seconds: 60,
        }
    }
    pub fn validate(&self) -> Result<(), PostgresError> {
        if self.url.trim().is_empty() {
            return Err(PostgresError::InvalidConfig("database URL is empty".into()));
        }
        if self.max_connections == 0 {
            return Err(PostgresError::InvalidConfig(
                "max_connections must be at least 1".into(),
            ));
        }
        Ok(())
    }
}

pub async fn connect(config: &PostgresPoolConfig) -> Result<PgPool, PostgresError> {
    config.validate()?;
    Ok(PgPoolOptions::new()
        .min_connections(0)
        .max_connections(config.max_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_seconds))
        .idle_timeout(Some(Duration::from_secs(config.idle_timeout_seconds)))
        .connect(&config.url)
        .await?)
}

pub fn connect_lazy(config: &PostgresPoolConfig) -> Result<PgPool, PostgresError> {
    config.validate()?;
    Ok(PgPoolOptions::new()
        .min_connections(0)
        .max_connections(config.max_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_seconds))
        .idle_timeout(Some(Duration::from_secs(config.idle_timeout_seconds)))
        .connect_lazy(&config.url)?)
}

pub async fn migrate(pool: &PgPool, path: impl AsRef<Path>) -> Result<(), PostgresError> {
    let migrator = sqlx::migrate::Migrator::new(path.as_ref()).await?;
    migrator.run(pool).await?;
    Ok(())
}

pub async fn migrate_with_history_table(
    pool: &PgPool,
    path: impl AsRef<Path>,
    history_table: &'static str,
) -> Result<(), PostgresError> {
    validate_identifier(history_table, "migration history table")?;
    let mut migrator = sqlx::migrate::Migrator::new(path.as_ref()).await?;
    migrator.dangerous_set_table_name(history_table);
    migrator.run(pool).await?;
    Ok(())
}

pub async fn ready(pool: &PgPool) -> bool {
    matches!(
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(pool)
            .await,
        Ok(1)
    )
}

fn validate_identifier(value: &str, description: &str) -> Result<(), PostgresError> {
    let mut bytes = value.bytes();
    let valid_start = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
    if !valid_start
        || value.len() > 63
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(PostgresError::InvalidConfig(format!(
            "{description} must be a PostgreSQL identifier of at most 63 ASCII characters"
        )));
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum PostgresError {
    #[error("invalid PostgreSQL configuration: {0}")]
    InvalidConfig(String),
    #[error("PostgreSQL error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("PostgreSQL migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn serverless_defaults_bound_connection_pressure() {
        let config = PostgresPoolConfig::serverless("postgres://example.invalid/db");
        assert_eq!(config.max_connections, 2);
        assert_eq!(config.acquire_timeout_seconds, 5);
    }

    #[test]
    fn pool_configuration_debug_redacts_database_credentials() {
        let config =
            PostgresPoolConfig::serverless("postgres://minco:secret-password@example.invalid/db");
        let debug = format!("{config:?}");
        assert!(!debug.contains("secret-password"));
        assert!(!debug.contains("postgres://"));
    }

    #[tokio::test]
    async fn migration_history_table_rejects_dynamic_sql_tokens() {
        let config = PostgresPoolConfig::serverless("postgres://example.invalid/db");
        let pool = connect_lazy(&config).expect("lazy pool");
        let result =
            migrate_with_history_table(&pool, Path::new("missing"), "_migrations;DROP").await;
        assert!(matches!(result, Err(PostgresError::InvalidConfig(_))));
    }
}
