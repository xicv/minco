//! Composition root shared by the local and Lambda entrypoints.
#![forbid(unsafe_code)]

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use axum::Router;
use minco_core::{ApplicationGraph, PluginId, PluginManager, PluginSelection};
use minco_http::{HttpRuntimeConfig, apply_standard_middleware};
use minco_plugin_audit::{
    AuditHealthSeverity, AuditJournalStore, AuditLedgerServices, AuditLedgerWriter,
    AuditLifecyclePolicy, AuditPlugin, AuditReader, AuditRelay, AuditRelayReport,
    AuditStorageInspector, MemoryAuditSink,
};
use minco_plugin_health::{HealthCheck, HealthPlugin, HealthRegistry, HealthResult};
use minco_plugin_idempotency::IdempotencyPlugin;
use minco_plugin_observability::{ObservabilityConfig, ObservabilityPlugin};
use orders_adapters::{MemoryOrderStore, OrderAuditReader};
use orders_api::ApiState;
use orders_application::{
    DeleteOrderPort, GetOrderPort, ListOrderAuditHistoryPort, ListOrdersPort, OrderReadiness,
    PlaceOrderPort, SystemClock, UpdateOrderPort,
};
use std::{env, net::IpAddr, path::PathBuf, str::FromStr, sync::Arc, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseKind {
    Memory,
    Sqlite,
    Postgres,
    DynamoDb,
}

impl FromStr for DatabaseKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "memory" => Ok(Self::Memory),
            "sqlite" => Ok(Self::Sqlite),
            "postgres" => Ok(Self::Postgres),
            "dynamodb" => Ok(Self::DynamoDb),
            other => {
                bail!(
                    "unsupported DATABASE_KIND {other}; expected memory, sqlite, postgres, or dynamodb"
                )
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
    pub audit_sqlite_path: PathBuf,
    pub database_max_connections: u32,
    pub audit_database_url: Option<String>,
    pub dynamodb_table_name: Option<String>,
    pub audit_dynamodb_table_name: Option<String>,
    pub dynamodb_endpoint_url: Option<String>,
    pub aws_region: String,
    pub allowed_origins: Vec<String>,
    pub allow_development_headers: bool,
    pub disabled_plugins: Vec<String>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        Self::from_env_with_database_urls(None, None)
    }

    pub fn from_env_with_database_url(database_url_override: Option<String>) -> Result<Self> {
        Self::from_env_with_database_urls(database_url_override, None)
    }

    pub fn from_env_with_database_urls(
        database_url_override: Option<String>,
        audit_database_url_override: Option<String>,
    ) -> Result<Self> {
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
        let audit_sqlite_path = env::var("AUDIT_SQLITE_PATH").map_or_else(
            |_| PathBuf::from("target/minco/orders-audit.db"),
            PathBuf::from,
        );
        let database_max_connections = env::var("DATABASE_MAX_CONNECTIONS")
            .unwrap_or_else(|_| "2".into())
            .parse()
            .context("DATABASE_MAX_CONNECTIONS must be an integer")?;
        let dynamodb_table_name = env::var("DYNAMODB_TABLE_NAME").ok();
        let audit_database_url =
            audit_database_url_override.or_else(|| env::var("AUDIT_DATABASE_URL").ok());
        let audit_dynamodb_table_name = env::var("AUDIT_DYNAMODB_TABLE_NAME").ok();
        let dynamodb_endpoint_url = env::var("DYNAMODB_ENDPOINT_URL")
            .ok()
            .or_else(|| env::var("AWS_ENDPOINT_URL").ok());
        let aws_region = env::var("AWS_REGION")
            .ok()
            .or_else(|| env::var("AWS_DEFAULT_REGION").ok())
            .unwrap_or_else(|| "ap-southeast-2".into());
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
            audit_sqlite_path,
            database_max_connections,
            audit_database_url,
            dynamodb_table_name,
            audit_dynamodb_table_name,
            dynamodb_endpoint_url,
            aws_region,
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
        if self.database_kind == DatabaseKind::Postgres
            && self.audit_database_url.as_deref().is_none_or(str::is_empty)
        {
            bail!("AUDIT_DATABASE_URL is required when DATABASE_KIND=postgres");
        }
        if self.database_kind == DatabaseKind::Postgres
            && self.database_url == self.audit_database_url
        {
            bail!("AUDIT_DATABASE_URL must identify a distinct PostgreSQL database");
        }
        if self.database_kind == DatabaseKind::Sqlite && self.sqlite_path == self.audit_sqlite_path
        {
            bail!("AUDIT_SQLITE_PATH must identify a distinct SQLite file");
        }
        if self.database_kind == DatabaseKind::DynamoDb
            && self
                .dynamodb_table_name
                .as_deref()
                .is_none_or(str::is_empty)
        {
            bail!("DYNAMODB_TABLE_NAME is required when DATABASE_KIND=dynamodb");
        }
        if self.database_kind == DatabaseKind::DynamoDb
            && self
                .audit_dynamodb_table_name
                .as_deref()
                .is_none_or(str::is_empty)
        {
            bail!("AUDIT_DYNAMODB_TABLE_NAME is required when DATABASE_KIND=dynamodb");
        }
        if self.database_kind == DatabaseKind::DynamoDb
            && self.dynamodb_table_name == self.audit_dynamodb_table_name
        {
            bail!("AUDIT_DYNAMODB_TABLE_NAME must identify a distinct table");
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
    let store = build_store(config).await?;
    let mut manager = PluginManager::default();
    manager.register(HealthPlugin)?;
    manager.register(ObservabilityPlugin::new(ObservabilityConfig {
        service_name: "minco-orders".into(),
        json: config.environment != "local",
        default_filter: "info,tower_http=info,sqlx=warn".into(),
    }))?;
    manager.register(IdempotencyPlugin::memory())?;
    manager.register(
        AuditPlugin::new(Arc::new(MemoryAuditSink::default()))
            .with_ledger_services(store.audit_services.clone()),
    )?;
    let mut selection = PluginSelection::default();
    selection.enabled.insert(PluginId::new("health")?);
    selection.enabled.insert(PluginId::new("audit")?);
    for plugin in &config.disabled_plugins {
        selection.disabled.insert(PluginId::new(plugin.clone())?);
    }
    let composed = manager.compose(&selection)?;
    if let Ok(observability) = composed.services.get::<ObservabilityConfig>() {
        let _ = observability.init();
    }
    let health = composed.services.get::<HealthRegistry>()?;
    health.register(Arc::new(StoreHealthCheck {
        store: Arc::clone(&store.readiness),
    }));
    health.register(Arc::new(AuditHealthCheck {
        inspector: Arc::clone(&store.audit_services.inspector),
    }));
    let state = ApiState::from_ports(
        store.place_orders,
        store.get_orders,
        store.list_orders,
        store.update_orders,
        store.delete_orders,
        store.audit_history,
        Arc::new(SystemClock),
        health,
        config.allow_development_headers,
    );
    let router = orders_api::build_router(state);
    let mut header_policy = minco_http::HttpHeaderPolicy::default();
    if config.allow_development_headers {
        for name in ["x-minco-subject", "x-minco-permissions"] {
            header_policy.allow_request_header_name(name)?;
            header_policy.mark_request_header_name_sensitive(name)?;
        }
    }
    let router = apply_standard_middleware(
        router,
        &HttpRuntimeConfig {
            allowed_origins: config.allowed_origins.clone(),
            allow_credentials: false,
            timeout: Duration::from_secs(15),
            max_request_body_bytes: 1024 * 1024,
            compression: true,
            header_policy,
        },
    )?;
    Ok(BuiltApplication {
        router,
        graph: composed.graph,
    })
}

pub async fn dispatch_audit_once(
    config: &AppConfig,
    worker_id: &str,
    limit: usize,
) -> Result<AuditRelayReport> {
    match config.database_kind {
        DatabaseKind::Sqlite => dispatch_sqlite_audit(config, worker_id, limit).await,
        DatabaseKind::Postgres => dispatch_postgres_audit(config, worker_id, limit).await,
        DatabaseKind::Memory | DatabaseKind::DynamoDb => {
            bail!("the selected database commits directly to its audit ledger and has no relay")
        }
    }
}

#[cfg(feature = "sqlite")]
async fn dispatch_sqlite_audit(
    config: &AppConfig,
    worker_id: &str,
    limit: usize,
) -> Result<AuditRelayReport> {
    use minco_sqlx_sqlite::{
        SqlitePoolConfig,
        audit_v2::{SqliteAuditJournal, SqliteAuditLedger, validate_separate_audit_pools},
    };
    let source = minco_sqlx_sqlite::connect(&SqlitePoolConfig::file(&config.sqlite_path)).await?;
    let ledger =
        minco_sqlx_sqlite::connect(&SqlitePoolConfig::file(&config.audit_sqlite_path)).await?;
    validate_separate_audit_pools(&source, &ledger).await?;
    let journal: Arc<dyn AuditJournalStore> = Arc::new(SqliteAuditJournal::new(source));
    let writer: Arc<dyn AuditLedgerWriter> = Arc::new(SqliteAuditLedger::new(ledger));
    AuditRelay::new(journal, writer)
        .dispatch_once(worker_id, limit, chrono::TimeDelta::minutes(1))
        .await
        .map_err(Into::into)
}

#[cfg(not(feature = "sqlite"))]
async fn dispatch_sqlite_audit(
    _config: &AppConfig,
    _worker_id: &str,
    _limit: usize,
) -> Result<AuditRelayReport> {
    bail!("the orders-service sqlite feature is disabled")
}

#[cfg(feature = "postgres")]
async fn dispatch_postgres_audit(
    config: &AppConfig,
    worker_id: &str,
    limit: usize,
) -> Result<AuditRelayReport> {
    use minco_sqlx_postgres::{
        PostgresPoolConfig,
        audit_v2::{PostgresAuditJournal, PostgresAuditLedger, validate_separate_audit_pools},
    };
    let source = minco_sqlx_postgres::connect(&PostgresPoolConfig::serverless(
        config
            .database_url
            .clone()
            .context("DATABASE_URL is required")?,
    ))
    .await?;
    let ledger = minco_sqlx_postgres::connect(&PostgresPoolConfig::serverless(
        config
            .audit_database_url
            .clone()
            .context("AUDIT_DATABASE_URL is required")?,
    ))
    .await?;
    validate_separate_audit_pools(&source, &ledger).await?;
    let journal: Arc<dyn AuditJournalStore> = Arc::new(PostgresAuditJournal::new(source));
    let writer: Arc<dyn AuditLedgerWriter> = Arc::new(PostgresAuditLedger::new(ledger));
    AuditRelay::new(journal, writer)
        .dispatch_once(worker_id, limit, chrono::TimeDelta::minutes(1))
        .await
        .map_err(Into::into)
}

#[cfg(not(feature = "postgres"))]
async fn dispatch_postgres_audit(
    _config: &AppConfig,
    _worker_id: &str,
    _limit: usize,
) -> Result<AuditRelayReport> {
    bail!("the orders-service postgres feature is disabled")
}

struct StoreBundle {
    place_orders: Arc<dyn PlaceOrderPort>,
    get_orders: Arc<dyn GetOrderPort>,
    list_orders: Arc<dyn ListOrdersPort>,
    update_orders: Arc<dyn UpdateOrderPort>,
    delete_orders: Arc<dyn DeleteOrderPort>,
    audit_history: Arc<dyn ListOrderAuditHistoryPort>,
    readiness: Arc<dyn OrderReadiness>,
    audit_services: AuditLedgerServices,
}

impl StoreBundle {
    fn new<S>(store: Arc<S>) -> Self
    where
        S: PlaceOrderPort
            + GetOrderPort
            + ListOrdersPort
            + UpdateOrderPort
            + DeleteOrderPort
            + ListOrderAuditHistoryPort
            + OrderReadiness
            + AuditLedgerWriter
            + AuditReader
            + AuditStorageInspector
            + 'static,
    {
        let place_orders: Arc<dyn PlaceOrderPort> = store.clone();
        let get_orders: Arc<dyn GetOrderPort> = store.clone();
        let list_orders: Arc<dyn ListOrdersPort> = store.clone();
        let update_orders: Arc<dyn UpdateOrderPort> = store.clone();
        let delete_orders: Arc<dyn DeleteOrderPort> = store.clone();
        let audit_history: Arc<dyn ListOrderAuditHistoryPort> = store.clone();
        let readiness: Arc<dyn OrderReadiness> = store.clone();
        let writer: Arc<dyn AuditLedgerWriter> = store.clone();
        let reader: Arc<dyn AuditReader> = store.clone();
        let inspector: Arc<dyn AuditStorageInspector> = store;
        Self {
            place_orders,
            get_orders,
            list_orders,
            update_orders,
            delete_orders,
            audit_history,
            readiness,
            audit_services: AuditLedgerServices::from_parts(writer, reader, inspector),
        }
    }

    fn with_audit<S>(
        store: Arc<S>,
        audit_history: Arc<dyn ListOrderAuditHistoryPort>,
        audit_services: AuditLedgerServices,
    ) -> Self
    where
        S: PlaceOrderPort
            + GetOrderPort
            + ListOrdersPort
            + UpdateOrderPort
            + DeleteOrderPort
            + OrderReadiness
            + 'static,
    {
        let place_orders: Arc<dyn PlaceOrderPort> = store.clone();
        let get_orders: Arc<dyn GetOrderPort> = store.clone();
        let list_orders: Arc<dyn ListOrdersPort> = store.clone();
        let update_orders: Arc<dyn UpdateOrderPort> = store.clone();
        let delete_orders: Arc<dyn DeleteOrderPort> = store.clone();
        let readiness: Arc<dyn OrderReadiness> = store;
        Self {
            place_orders,
            get_orders,
            list_orders,
            update_orders,
            delete_orders,
            audit_history,
            readiness,
            audit_services,
        }
    }
}

async fn build_store(config: &AppConfig) -> Result<StoreBundle> {
    match config.database_kind {
        DatabaseKind::Memory => Ok(StoreBundle::new(Arc::new(MemoryOrderStore::new()))),
        DatabaseKind::Sqlite => build_sqlite_store(config).await,
        DatabaseKind::Postgres => build_postgres_store(config).await,
        DatabaseKind::DynamoDb => build_dynamodb_store(config).await,
    }
}

#[cfg(feature = "sqlite")]
async fn build_sqlite_store(config: &AppConfig) -> Result<StoreBundle> {
    use minco_sqlx_sqlite::{
        SqlitePoolConfig,
        audit_v2::{
            SqliteAuditLedger, SqliteAuditStorageInspector, migrate_audit_ledger,
            validate_separate_audit_pools,
        },
    };
    use orders_adapters::SqliteOrderStore;
    if let Some(parent) = config.sqlite_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if let Some(parent) = config.audit_sqlite_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut database = SqlitePoolConfig::file(&config.sqlite_path);
    database.max_connections = config.database_max_connections;
    let store = SqliteOrderStore::connect(&database).await?;
    store.migrate("examples/orders/migrations/sqlite").await?;
    minco_sqlx_sqlite::migrate_with_history_table(
        store.pool(),
        "extensions/minco-sqlx-sqlite/migrations/plugins",
        "_minco_plugin_migrations",
    )
    .await?;
    let mut audit_database = SqlitePoolConfig::file(&config.audit_sqlite_path);
    audit_database.max_connections = config.database_max_connections;
    let audit_pool = minco_sqlx_sqlite::connect(&audit_database).await?;
    validate_separate_audit_pools(store.pool(), &audit_pool).await?;
    migrate_audit_ledger(&audit_pool).await?;
    let store = Arc::new(store);
    let ledger = Arc::new(SqliteAuditLedger::new(audit_pool.clone()));
    let inspector = Arc::new(SqliteAuditStorageInspector::new(
        store.pool().clone(),
        audit_pool,
        AuditLifecyclePolicy::sqlite_100_mib(512 * 1024 * 1024),
    )?);
    let writer: Arc<dyn AuditLedgerWriter> = ledger.clone();
    let reader: Arc<dyn AuditReader> = ledger;
    let inspector: Arc<dyn AuditStorageInspector> = inspector;
    let audit_history: Arc<dyn ListOrderAuditHistoryPort> =
        Arc::new(OrderAuditReader::new(Arc::clone(&reader)));
    Ok(StoreBundle::with_audit(
        store,
        audit_history,
        AuditLedgerServices::from_parts(writer, reader, inspector),
    ))
}

#[cfg(not(feature = "sqlite"))]
async fn build_sqlite_store(_config: &AppConfig) -> Result<StoreBundle> {
    bail!("the orders-service sqlite feature is disabled")
}

#[cfg(feature = "postgres")]
async fn build_postgres_store(config: &AppConfig) -> Result<StoreBundle> {
    use minco_sqlx_postgres::{
        PostgresPoolConfig,
        audit_v2::{
            PostgresAuditLedger, PostgresAuditStorageInspector, validate_separate_audit_pools,
        },
    };
    use orders_adapters::PostgresOrderStore;
    let mut database = PostgresPoolConfig::serverless(
        config
            .database_url
            .clone()
            .context("DATABASE_URL is required")?,
    );
    database.max_connections = config.database_max_connections;
    let store = PostgresOrderStore::connect(&database).await?;
    let mut audit_database = PostgresPoolConfig::serverless(
        config
            .audit_database_url
            .clone()
            .context("AUDIT_DATABASE_URL is required")?,
    );
    audit_database.max_connections = config.database_max_connections;
    let audit_pool = minco_sqlx_postgres::connect(&audit_database).await?;
    validate_separate_audit_pools(store.pool(), &audit_pool).await?;
    let mut lifecycle = AuditLifecyclePolicy::cloud_online();
    lifecycle.maximum_pending_records = 100_000;
    lifecycle.maximum_pending_bytes = 64 * 1024 * 1024;
    lifecycle.maximum_oldest_pending_seconds = 3_600;
    let store = Arc::new(store);
    let ledger = Arc::new(PostgresAuditLedger::new(audit_pool.clone()));
    let inspector = Arc::new(PostgresAuditStorageInspector::new(
        store.pool().clone(),
        audit_pool,
        lifecycle,
    )?);
    let writer: Arc<dyn AuditLedgerWriter> = ledger.clone();
    let reader: Arc<dyn AuditReader> = ledger;
    let inspector: Arc<dyn AuditStorageInspector> = inspector;
    let audit_history: Arc<dyn ListOrderAuditHistoryPort> =
        Arc::new(OrderAuditReader::new(Arc::clone(&reader)));
    Ok(StoreBundle::with_audit(
        store,
        audit_history,
        AuditLedgerServices::from_parts(writer, reader, inspector),
    ))
}

#[cfg(not(feature = "postgres"))]
async fn build_postgres_store(_config: &AppConfig) -> Result<StoreBundle> {
    bail!("the orders-service postgres feature is disabled")
}

#[cfg(feature = "dynamodb")]
async fn build_dynamodb_store(config: &AppConfig) -> Result<StoreBundle> {
    use minco_aws_dynamodb::DynamoDbConfig;
    use orders_adapters::DynamoDbOrderStore;
    let provider = DynamoDbConfig::new(
        config
            .dynamodb_table_name
            .clone()
            .context("DYNAMODB_TABLE_NAME is required")?,
        config.aws_region.clone(),
        config.dynamodb_endpoint_url.clone(),
    )
    .build()
    .await?;
    let store = Arc::new(DynamoDbOrderStore::with_audit(
        provider,
        config
            .audit_dynamodb_table_name
            .clone()
            .context("AUDIT_DYNAMODB_TABLE_NAME is required")?,
    )?);
    let ledger = Arc::new(
        store
            .audit_reader()
            .context("DynamoDB audit ledger is required")?,
    );
    let writer: Arc<dyn AuditLedgerWriter> = ledger.clone();
    let reader: Arc<dyn AuditReader> = ledger.clone();
    let inspector: Arc<dyn AuditStorageInspector> = ledger;
    let audit_history: Arc<dyn ListOrderAuditHistoryPort> =
        Arc::new(OrderAuditReader::new(Arc::clone(&reader)));
    Ok(StoreBundle::with_audit(
        store,
        audit_history,
        AuditLedgerServices::from_parts(writer, reader, inspector),
    ))
}

#[cfg(not(feature = "dynamodb"))]
async fn build_dynamodb_store(_config: &AppConfig) -> Result<StoreBundle> {
    bail!("the orders-service dynamodb feature is disabled")
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
    store: Arc<dyn OrderReadiness>,
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

struct AuditHealthCheck {
    inspector: Arc<dyn AuditStorageInspector>,
}

impl std::fmt::Debug for AuditHealthCheck {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuditHealthCheck")
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl HealthCheck for AuditHealthCheck {
    fn id(&self) -> &'static str {
        "audit-ledger"
    }

    async fn check(&self) -> HealthResult {
        match self.inspector.storage_health().await {
            Ok(health) => {
                let ready = health.severity != AuditHealthSeverity::Critical;
                let detail = match health.severity {
                    AuditHealthSeverity::Healthy => None,
                    AuditHealthSeverity::Warning => {
                        Some("audit storage is approaching a lifecycle threshold".into())
                    }
                    AuditHealthSeverity::RotationRequired => {
                        Some("audit storage reached its explicit rotation threshold".into())
                    }
                    AuditHealthSeverity::Critical => {
                        Some("audit storage reached a hard lifecycle limit".into())
                    }
                };
                HealthResult {
                    id: self.id().into(),
                    ready,
                    critical: true,
                    detail,
                }
            }
            Err(_) => HealthResult {
                id: self.id().into(),
                ready: false,
                critical: true,
                detail: Some("audit storage health is unavailable".into()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct WarningAuditInspector;

    #[async_trait]
    impl AuditStorageInspector for WarningAuditInspector {
        async fn storage_health(
            &self,
        ) -> Result<minco_plugin_audit::AuditStorageHealth, minco_plugin_audit::AuditLedgerError>
        {
            minco_plugin_audit::evaluate_storage_health(
                AuditLifecyclePolicy::sqlite_100_mib(512 * 1024 * 1024),
                minco_plugin_audit::AuditStorageSnapshot {
                    provider: "sqlite".into(),
                    hot_bytes: 90 * 1024 * 1024,
                    free_bytes: Some(1024 * 1024 * 1024),
                    pending_records: 0,
                    pending_bytes: 0,
                    oldest_pending_seconds: None,
                    quarantined_records: 0,
                    archive_watermark: None,
                    segments: vec![minco_plugin_audit::AuditSegmentStatus {
                        segment_id: 1,
                        state: minco_plugin_audit::AuditSegmentState::Active,
                        record_count: 1,
                        encoded_bytes: 90 * 1024 * 1024,
                        first: None,
                        last: None,
                        archive_receipt: None,
                    }],
                },
            )
        }
    }

    #[tokio::test]
    async fn audit_warning_is_visible_without_failing_readiness() {
        let check = AuditHealthCheck {
            inspector: Arc::new(WarningAuditInspector),
        };
        let result = check.check().await;
        assert!(result.ready);
        assert_eq!(
            result.detail.as_deref(),
            Some("audit storage is approaching a lifecycle threshold")
        );
    }

    #[test]
    fn production_rejects_development_identity_headers() {
        let config = AppConfig {
            environment: "production".into(),
            host: "127.0.0.1".parse().expect("IP"),
            port: 3000,
            database_kind: DatabaseKind::Memory,
            database_url: None,
            sqlite_path: "orders.db".into(),
            audit_sqlite_path: "orders-audit.db".into(),
            database_max_connections: 1,
            audit_database_url: None,
            dynamodb_table_name: None,
            audit_dynamodb_table_name: None,
            dynamodb_endpoint_url: None,
            aws_region: "ap-southeast-2".into(),
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
            audit_sqlite_path: "orders-audit.db".into(),
            database_max_connections: 1,
            audit_database_url: None,
            dynamodb_table_name: None,
            audit_dynamodb_table_name: None,
            dynamodb_endpoint_url: None,
            aws_region: "ap-southeast-2".into(),
            allowed_origins: vec!["*".into()],
            allow_development_headers: true,
            disabled_plugins: Vec::new(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn dynamodb_profile_requires_an_explicit_table_and_accepts_no_database_url() {
        assert_eq!(
            "dynamodb".parse::<DatabaseKind>().unwrap(),
            DatabaseKind::DynamoDb
        );
        let config = AppConfig {
            environment: "production".into(),
            host: "127.0.0.1".parse().expect("IP"),
            port: 3000,
            database_kind: DatabaseKind::DynamoDb,
            database_url: None,
            sqlite_path: "orders.db".into(),
            audit_sqlite_path: "orders-audit.db".into(),
            database_max_connections: 1,
            audit_database_url: None,
            dynamodb_table_name: Some("orders-production".into()),
            audit_dynamodb_table_name: Some("orders-audit-production".into()),
            dynamodb_endpoint_url: None,
            aws_region: "ap-southeast-2".into(),
            allowed_origins: vec!["https://app.example.invalid".into()],
            allow_development_headers: false,
            disabled_plugins: Vec::new(),
        };
        assert!(config.validate().is_ok());
        let missing_table = AppConfig {
            dynamodb_table_name: None,
            ..config
        };
        assert!(missing_table.validate().is_err());
    }
}
