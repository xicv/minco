//! Composition root shared by the local and Lambda entrypoints.
#![forbid(unsafe_code)]

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use axum::Router;
use minco_core::{ApplicationGraph, PluginId, PluginManager, PluginSelection};
use minco_http::{HttpRuntimeConfig, apply_standard_middleware};
use minco_plugin_health::{HealthCheck, HealthPlugin, HealthRegistry, HealthResult};
use minco_plugin_idempotency::IdempotencyPlugin;
use minco_plugin_observability::{ObservabilityConfig, ObservabilityPlugin};
use orders_adapters::MemoryOrderStore;
use orders_api::ApiState;
use orders_application::{OrderStore, SystemClock};
use std::{env, net::IpAddr, path::PathBuf, str::FromStr, sync::Arc, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseKind {
    Memory,
    Sqlite,
    Postgres,
}

impl FromStr for DatabaseKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "memory" => Ok(Self::Memory),
            "sqlite" => Ok(Self::Sqlite),
            "postgres" => Ok(Self::Postgres),
            other => {
                bail!("unsupported DATABASE_KIND {other}; expected memory, sqlite, or postgres")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub environment: String,
    pub host: IpAddr,
    pub port: u16,
    pub database_kind: DatabaseKind,
    pub database_url: Option<String>,
    pub sqlite_path: PathBuf,
    pub database_max_connections: u32,
    pub allowed_origins: Vec<String>,
    pub allow_development_headers: bool,
    pub disabled_plugins: Vec<String>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        Self::from_env_with_database_url(None)
    }

    pub fn from_env_with_database_url(database_url_override: Option<String>) -> Result<Self> {
        let environment = env::var("APP_ENV").unwrap_or_else(|_| "local".into());
        let host = env::var("API_HOST")
            .unwrap_or_else(|_| "127.0.0.1".into())
            .parse()
            .context("API_HOST must be an IP address")?;
        let port = env::var("API_PORT")
            .unwrap_or_else(|_| "3000".into())
            .parse()
            .context("API_PORT must be an integer")?;
        let database_kind = env::var("DATABASE_KIND")
            .unwrap_or_else(|_| "sqlite".into())
            .parse()?;
        let database_url = database_url_override.or_else(|| env::var("DATABASE_URL").ok());
        let sqlite_path = env::var("SQLITE_PATH")
            .map_or_else(|_| PathBuf::from("target/minco/orders.db"), PathBuf::from);
        let database_max_connections = env::var("DATABASE_MAX_CONNECTIONS")
            .unwrap_or_else(|_| "2".into())
            .parse()
            .context("DATABASE_MAX_CONNECTIONS must be an integer")?;
        let allowed_origins = env::var("ALLOWED_ORIGINS")
            .unwrap_or_else(|_| "http://127.0.0.1:5173,http://localhost:5173".into())
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let allow_development_headers =
            parse_bool("ALLOW_DEVELOPMENT_HEADERS", environment == "local")?;
        let disabled_plugins = env::var("MINCO_DISABLED_PLUGINS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect();
        let config = Self {
            environment,
            host,
            port,
            database_kind,
            database_url,
            sqlite_path,
            database_max_connections,
            allowed_origins,
            allow_development_headers,
            disabled_plugins,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.environment == "production" && self.allow_development_headers {
            bail!("production rejects ALLOW_DEVELOPMENT_HEADERS=true");
        }
        if self.allowed_origins.is_empty() {
            bail!("at least one exact ALLOWED_ORIGINS value is required");
        }
        if self.allowed_origins.iter().any(|origin| origin == "*") {
            bail!("wildcard CORS origins are not supported");
        }
        if self.database_kind == DatabaseKind::Postgres
            && self.database_url.as_deref().is_none_or(str::is_empty)
        {
            bail!("DATABASE_URL is required when DATABASE_KIND=postgres");
        }
        if self.database_max_connections == 0 {
            bail!("DATABASE_MAX_CONNECTIONS must be at least one");
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct BuiltApplication {
    pub router: Router,
    pub graph: ApplicationGraph,
}

pub async fn build_application(config: &AppConfig) -> Result<BuiltApplication> {
    let mut manager = PluginManager::default();
    manager.register(HealthPlugin)?;
    manager.register(ObservabilityPlugin::new(ObservabilityConfig {
        service_name: "minco-orders".into(),
        json: config.environment != "local",
        default_filter: "info,tower_http=info,sqlx=warn".into(),
    }))?;
    manager.register(IdempotencyPlugin::memory())?;
    let mut selection = PluginSelection::default();
    selection.enabled.insert(PluginId::new("health")?);
    for plugin in &config.disabled_plugins {
        selection.disabled.insert(PluginId::new(plugin.clone())?);
    }
    let composed = manager.compose(&selection)?;
    if let Ok(observability) = composed.services.get::<ObservabilityConfig>() {
        let _ = observability.init();
    }
    let health = composed.services.get::<HealthRegistry>()?;
    let store = build_store(config).await?;
    health.register(Arc::new(StoreHealthCheck {
        store: Arc::clone(&store),
    }));
    let state = ApiState::new(
        store,
        Arc::new(SystemClock),
        health,
        config.allow_development_headers,
    );
    let router = orders_api::build_router(state);
    let router = apply_standard_middleware(
        router,
        &HttpRuntimeConfig {
            allowed_origins: config.allowed_origins.clone(),
            allow_credentials: false,
            timeout: Duration::from_secs(15),
            max_request_body_bytes: 1024 * 1024,
            compression: true,
        },
    )?;
    Ok(BuiltApplication {
        router,
        graph: composed.graph,
    })
}

async fn build_store(config: &AppConfig) -> Result<Arc<dyn OrderStore>> {
    match config.database_kind {
        DatabaseKind::Memory => Ok(Arc::new(MemoryOrderStore::new())),
        DatabaseKind::Sqlite => build_sqlite_store(config).await,
        DatabaseKind::Postgres => build_postgres_store(config).await,
    }
}

#[cfg(feature = "sqlite")]
async fn build_sqlite_store(config: &AppConfig) -> Result<Arc<dyn OrderStore>> {
    use minco_sqlx_sqlite::SqlitePoolConfig;
    use orders_adapters::SqliteOrderStore;
    if let Some(parent) = config.sqlite_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut database = SqlitePoolConfig::file(&config.sqlite_path);
    database.max_connections = config.database_max_connections;
    let store = SqliteOrderStore::connect(&database).await?;
    store.migrate("examples/orders/migrations/sqlite").await?;
    Ok(Arc::new(store))
}

#[cfg(not(feature = "sqlite"))]
async fn build_sqlite_store(_config: &AppConfig) -> Result<Arc<dyn OrderStore>> {
    bail!("the orders-service sqlite feature is disabled")
}

#[cfg(feature = "postgres")]
async fn build_postgres_store(config: &AppConfig) -> Result<Arc<dyn OrderStore>> {
    use minco_sqlx_postgres::PostgresPoolConfig;
    use orders_adapters::PostgresOrderStore;
    let mut database = PostgresPoolConfig::serverless(
        config
            .database_url
            .clone()
            .context("DATABASE_URL is required")?,
    );
    database.max_connections = config.database_max_connections;
    let store = PostgresOrderStore::connect(&database).await?;
    Ok(Arc::new(store))
}

#[cfg(not(feature = "postgres"))]
async fn build_postgres_store(_config: &AppConfig) -> Result<Arc<dyn OrderStore>> {
    bail!("the orders-service postgres feature is disabled")
}

fn parse_bool(name: &str, default: bool) -> Result<bool> {
    match env::var(name) {
        Ok(value) => match value.as_str() {
            "1" | "true" | "yes" => Ok(true),
            "0" | "false" | "no" => Ok(false),
            _ => bail!("{name} must be true or false"),
        },
        Err(env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(error.into()),
    }
}

struct StoreHealthCheck {
    store: Arc<dyn OrderStore>,
}

impl std::fmt::Debug for StoreHealthCheck {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StoreHealthCheck")
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl HealthCheck for StoreHealthCheck {
    fn id(&self) -> &'static str {
        "orders-store"
    }
    async fn check(&self) -> HealthResult {
        let ready = self.store.ready().await;
        HealthResult {
            id: self.id().into(),
            ready,
            critical: true,
            detail: (!ready).then(|| "order persistence is unavailable".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_rejects_development_identity_headers() {
        let config = AppConfig {
            environment: "production".into(),
            host: "127.0.0.1".parse().expect("IP"),
            port: 3000,
            database_kind: DatabaseKind::Memory,
            database_url: None,
            sqlite_path: "orders.db".into(),
            database_max_connections: 1,
            allowed_origins: vec!["https://app.example.invalid".into()],
            allow_development_headers: true,
            disabled_plugins: Vec::new(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn wildcard_cors_is_rejected() {
        let config = AppConfig {
            environment: "local".into(),
            host: "127.0.0.1".parse().expect("IP"),
            port: 3000,
            database_kind: DatabaseKind::Memory,
            database_url: None,
            sqlite_path: "orders.db".into(),
            database_max_connections: 1,
            allowed_origins: vec!["*".into()],
            allow_development_headers: true,
            disabled_plugins: Vec::new(),
        };
        assert!(config.validate().is_err());
    }
}
