//! The standalone Minco Desk private-beta example (ADR-0072).
//!
//! Composes every service a private helpdesk needs on one `SQLite`
//! database behind one native Axum process: identity, sessions/CSRF,
//! idempotency, object storage, notifications, audit, events/outbox,
//! jobs, health, observability and ticketing. The composition root is
//! the only place concrete adapters are selected (ADR-0011): memory
//! adapters for mail/objects (no provider contact), `SQLite` for all
//! durable state.
#![forbid(unsafe_code)]

use anyhow::{Context as _, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

/// Runtime configuration from environment variables; every default is
/// safe for a purely local, providerless run.
#[derive(Debug, Clone)]
pub struct DeskConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub project_id: String,
    pub portal_origin: String,
    pub allowed_origins: Vec<String>,
    pub mailbox_scope: String,
    pub environment: String,
}

impl DeskConfig {
    /// Loads the configuration; `DESK_DATABASE_URL` defaults to a local
    /// `SQLite` file so a clean clone runs without external setup.
    pub fn from_env() -> Result<Self> {
        let database_url = std::env::var("DESK_DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://minco-desk.sqlite?mode=rwc".into());
        Ok(Self {
            host: std::env::var("DESK_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            port: std::env::var("DESK_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8090),
            database_url,
            project_id: std::env::var("DESK_PROJECT_ID").unwrap_or_else(|_| "desk".into()),
            portal_origin: std::env::var("DESK_PORTAL_ORIGIN")
                .unwrap_or_else(|_| "http://127.0.0.1:8090".into()),
            allowed_origins: match std::env::var("DESK_ALLOWED_ORIGINS") {
                Ok(value) => value.split(',').map(str::to_owned).collect(),
                Err(_) => vec!["http://127.0.0.1:8090".into()],
            },
            mailbox_scope: std::env::var("DESK_MAILBOX_SCOPE")
                .unwrap_or_else(|_| "support@desk.example.test".into()),
            environment: std::env::var("DESK_ENVIRONMENT").unwrap_or_else(|_| "local".into()),
        })
    }
}

/// The composed application: the router plus the service graph the
/// health registry reports on.
pub struct BuiltDesk {
    pub router: axum::Router,
    pub health_report: serde_json::Value,
}

impl std::fmt::Debug for BuiltDesk {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BuiltDesk")
            .field("health_report", &self.health_report)
            .finish_non_exhaustive()
    }
}

/// Applies every plugin migration to the configured database and
/// returns the pool. Clean install and migration are the same command:
/// a fresh file gets every table; an existing file advances.
#[cfg(feature = "sqlite")]
pub async fn migrate(config: &DeskConfig) -> Result<sqlx::SqlitePool> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(4)
        .connect(&config.database_url)
        .await
        .context("open the desk database")?;
    // Plugin-storage migrations (jobs, sessions, idempotency...).
    minco_sqlx_sqlite::plugin_adapters::migrate_plugin_storage(&pool)
        .await
        .context("apply plugin-storage migrations")?;
    // Ticketing's own migrations.
    let ticketing = minco_plugin_ticketing::SqliteTicketingStore::new(pool.clone());
    ticketing
        .migrate()
        .await
        .context("apply ticketing migrations")?;
    Ok(pool)
}

/// Builds the standalone desk: all services composed on one pool, one
/// native process, zero provider contact.
#[cfg(feature = "sqlite")]
pub async fn build_desk(config: &DeskConfig) -> Result<BuiltDesk> {
    use minco_plugin_audit::MemoryAuditSink;
    use minco_plugin_events::EventsPlugin;
    use minco_plugin_health::{HealthPlugin, HealthRegistry};
    use minco_plugin_idempotency::IdempotencyPlugin;
    use minco_plugin_identity::IdentityPlugin;
    use minco_plugin_jobs::JobsServices;
    use minco_plugin_notifications::NotificationsPlugin;
    use minco_plugin_observability::{ObservabilityConfig, ObservabilityPlugin};
    use minco_plugin_sessions::SessionsPlugin;
    use minco_plugin_ticketing::{
        SqliteTicketingStore, TicketingConfig, TicketingJobsDeps, TicketingPortalServices,
        TicketingService, TicketingStoreService, register_ticketing_jobs, ticketing_router,
    };

    let pool = migrate(config).await?;

    // Concrete adapter selection lives here and nowhere else.
    let ticketing_store: Arc<dyn minco_plugin_ticketing::TicketingStore> =
        Arc::new(SqliteTicketingStore::new(pool.clone()));
    let jobs_store = Arc::new(minco_sqlx_sqlite::jobs::SqliteJobStore::new(pool.clone()));
    let registry = Arc::new(minco_plugin_jobs::JobHandlerRegistry::new());
    let jobs = JobsServices::new(
        jobs_store.clone(),
        jobs_store.clone(),
        Arc::new(minco_plugin_jobs::FailClosedDispatcher),
        jobs_store,
        Arc::new(minco_plugin_jobs::SystemJobClock),
        Arc::new(minco_plugin_jobs::JobExecutor::new(Arc::clone(&registry))),
    );
    let objects = Arc::new(minco_plugin_object_storage::ObjectStoreService::new(
        Arc::new(minco_plugin_object_storage::MemoryObjectStore::default()),
    ));
    let notifications = Arc::new(minco_plugin_notifications::NotificationService::new(
        Arc::new(minco_plugin_notifications::MemoryNotificationSink::default()),
    ));

    let service = Arc::new(
        TicketingService::new(
            TicketingStoreService::new(Arc::clone(&ticketing_store)),
            TicketingConfig {
                project_id: config.project_id.clone(),
                portal_origin: config.portal_origin.clone(),
                notify_requester_on_public_reply: true,
                ..TicketingConfig::default()
            },
        )?
        .with_portal_services(TicketingPortalServices {
            jobs: Some(Arc::new(jobs)),
            objects: Some(objects.clone()),
            ..TicketingPortalServices::default()
        }),
    );
    // The durable worker principal holds only ingest authority.
    let worker = minco_plugin_identity::Identity {
        subject: "desk-mail-worker".into(),
        permissions: BTreeSet::from(["ticketing.ingest".into()]),
        scopes: BTreeSet::new(),
        claims: BTreeMap::default(),
    };
    register_ticketing_jobs(
        &registry,
        &TicketingStoreService::new(Arc::clone(&ticketing_store)),
        TicketingJobsDeps {
            service: minco_plugin_ticketing::TicketingService::clone(&service),
            notifications: Arc::clone(&notifications),
            mail: None,
            objects: Arc::clone(&objects),
            worker,
        },
    )?;

    // The plugin graph proves the composition (ADR-0072): every
    // dependency is registered and the selection is explicit.
    let mut manager = minco_core::PluginManager::default();
    manager.register(HealthPlugin)?;
    manager.register(ObservabilityPlugin::new(ObservabilityConfig {
        service_name: "minco-desk".into(),
        json: config.environment != "local",
        default_filter: "info,tower_http=info,sqlx=warn".into(),
    }))?;
    manager.register(IdentityPlugin::default())?;
    manager.register(SessionsPlugin::memory())?;
    manager.register(IdempotencyPlugin::memory())?;
    manager.register(NotificationsPlugin::memory().0)?;
    manager.register(EventsPlugin::memory().0)?;
    manager.register(minco_plugin_audit::AuditPlugin::new(Arc::new(
        MemoryAuditSink::default(),
    )))?;
    let mut selection = minco_core::PluginSelection::default();
    selection
        .enabled
        .insert(minco_core::PluginId::new("health")?);
    selection
        .enabled
        .insert(minco_core::PluginId::new("observability")?);
    selection
        .enabled
        .insert(minco_core::PluginId::new("identity")?);
    selection
        .enabled
        .insert(minco_core::PluginId::new("sessions")?);
    selection
        .enabled
        .insert(minco_core::PluginId::new("idempotency")?);
    selection
        .enabled
        .insert(minco_core::PluginId::new("notifications")?);
    selection
        .enabled
        .insert(minco_core::PluginId::new("events")?);
    selection
        .enabled
        .insert(minco_core::PluginId::new("audit")?);
    let composed = manager.compose(&selection)?;

    let health = composed.services.get::<HealthRegistry>()?;
    health.register(Arc::new(TicketingStoreHealth {
        store: TicketingStoreService::new(Arc::clone(&ticketing_store)),
    }));
    health.register(Arc::new(JobsStoreHealth { pool }));
    let health_report =
        serde_json::to_value(&composed.graph).unwrap_or_else(|_| serde_json::json!({}));

    let desk_router = ticketing_router(minco_plugin_ticketing::TicketingService::clone(&service));
    let mut header_policy = minco_http::HttpHeaderPolicy::default();
    for name in ["x-minco-subject", "x-minco-permissions"] {
        header_policy.allow_request_header_name(name)?;
        header_policy.mark_request_header_name_sensitive(name)?;
    }
    let router = minco_http::apply_standard_middleware(
        desk_router,
        &minco_http::HttpRuntimeConfig {
            allowed_origins: config.allowed_origins.clone(),
            allow_credentials: false,
            timeout: Duration::from_secs(15),
            max_request_body_bytes: 1024 * 1024,
            compression: true,
            header_policy,
        },
    )?;
    Ok(BuiltDesk {
        router,
        health_report,
    })
}

#[cfg(not(feature = "sqlite"))]
pub async fn build_desk(_config: &DeskConfig) -> Result<BuiltDesk> {
    bail!("the standalone desk requires the sqlite feature")
}

#[cfg(feature = "sqlite")]
struct TicketingStoreHealth {
    store: minco_plugin_ticketing::TicketingStoreService,
}

#[cfg(feature = "sqlite")]
impl std::fmt::Debug for TicketingStoreHealth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("TicketingStoreHealth").finish()
    }
}

#[cfg(feature = "sqlite")]
#[async_trait::async_trait]
impl minco_plugin_health::HealthCheck for TicketingStoreHealth {
    fn id(&self) -> &'static str {
        "ticketing-store"
    }

    async fn check(&self) -> minco_plugin_health::HealthResult {
        let ready = self.store.ready().await.is_ok();
        minco_plugin_health::HealthResult {
            id: "ticketing-store".into(),
            ready,
            critical: true,
            detail: (!ready).then(|| "ticketing store is not ready".into()),
        }
    }
}

#[cfg(feature = "sqlite")]
struct JobsStoreHealth {
    pool: sqlx::SqlitePool,
}

#[cfg(feature = "sqlite")]
impl std::fmt::Debug for JobsStoreHealth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("JobsStoreHealth").finish()
    }
}

#[cfg(feature = "sqlite")]
#[async_trait::async_trait]
impl minco_plugin_health::HealthCheck for JobsStoreHealth {
    fn id(&self) -> &'static str {
        "jobs-store"
    }

    async fn check(&self) -> minco_plugin_health::HealthResult {
        let ready = sqlx::query("SELECT 1 FROM minco_jobs LIMIT 1")
            .execute(&self.pool)
            .await
            .is_ok();
        minco_plugin_health::HealthResult {
            id: "jobs-store".into(),
            ready,
            critical: true,
            detail: (!ready).then(|| "jobs store is not ready".into()),
        }
    }
}
