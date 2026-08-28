//! The standalone Minco Desk example (ADR-0072).
//!
//! Composes every service a private helpdesk needs on one `SQLite`
//! database behind one native Axum process: identity, sessions/CSRF,
//! idempotency, object storage, notifications, audit, events/outbox,
//! jobs, health, observability and ticketing. The composition root is
//! the only place concrete adapters are selected (ADR-0011):
//! `SQLite` for every durable surface — tickets, jobs (same-transaction
//! enqueue), requester sessions, idempotency and audit — and memory
//! adapters only where no provider is contacted by design (raw MIME
//! objects, the in-process event bus, the notification sink). The
//! trust boundary is explicit: requester routes authenticate with
//! durable session cookies; every other ticketing route requires the
//! loopback service bearer token (`DESK_AGENT_TOKEN`) and the
//! development identity headers are not trusted.
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
    /// Loopback service token for the agent/integration surface. Loaded
    /// from `DESK_AGENT_TOKEN`; when unset a high-entropy token is
    /// generated for this process (printed at startup by the local
    /// binary) so the desk never trusts unauthenticated callers.
    pub agent_token: String,
    /// CSRF signing secret. Loaded from `DESK_CSRF_SECRET`; generated
    /// per process when unset (sessions then do not survive restarts,
    /// which the durability proofs make explicit).
    pub csrf_secret: String,
    /// Handoff return-location policy: exact origins and their allowed
    /// path prefixes, loaded from `DESK_ALLOWED_RETURN_PATHS` as
    /// `origin=path|path,origin=path`. Defaults to the portal origin
    /// with the ticketing prefix.
    pub allowed_return_paths: BTreeMap<String, Vec<String>>,
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
            agent_token: std::env::var("DESK_AGENT_TOKEN")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| {
                    format!(
                        "{}{}",
                        uuid::Uuid::new_v4().simple(),
                        uuid::Uuid::new_v4().simple()
                    )
                }),
            csrf_secret: std::env::var("DESK_CSRF_SECRET")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| {
                    format!(
                        "{}{}",
                        uuid::Uuid::new_v4().simple(),
                        uuid::Uuid::new_v4().simple()
                    )
                }),
            allowed_return_paths: parse_return_paths(
                &std::env::var("DESK_ALLOWED_RETURN_PATHS")
                    .unwrap_or_else(|_| "http://127.0.0.1:8090=/_minco/ticketing".into()),
            ),
        })
    }
}

/// Parses `origin=path|path,origin=path` into the handoff location
/// policy; malformed entries fail closed at startup.
fn parse_return_paths(value: &str) -> BTreeMap<String, Vec<String>> {
    let mut policy = BTreeMap::new();
    for entry in value.split(',') {
        let Some((origin, paths)) = entry.split_once('=') else {
            continue;
        };
        let paths = paths
            .split('|')
            .filter(|path| !path.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if !origin.is_empty() && !paths.is_empty() {
            policy.insert(origin.to_owned(), paths);
        }
    }
    policy
}

/// The composed application: the router, the explicit jobs worker and
/// the service graph the health registry reports on.
pub struct BuiltDesk {
    pub router: axum::Router,
    pub worker: DeskWorker,
    pub health_report: serde_json::Value,
}

/// The desk's explicit jobs worker: one bounded dispatch pass per call.
/// Nothing schedules it implicitly — the local binary drives it on an
/// interval and proofs drive it by hand (review finding 3).
#[derive(Clone)]
pub struct DeskWorker {
    jobs: Arc<minco_plugin_jobs::JobsServices>,
    audit: Arc<minco_plugin_ticketing::TicketingService>,
    project_id: String,
}

impl std::fmt::Debug for DeskWorker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("DeskWorker").finish()
    }
}

impl DeskWorker {
    /// One bounded dispatch pass over due job publications; claimed jobs
    /// execute in-process through the registered handlers and their
    /// durable dispositions are committed.
    pub async fn run_once(&self) -> Result<minco_plugin_jobs::DispatchReport, anyhow::Error> {
        let report = self
            .jobs
            .dispatch_due_once(
                &format!("desk-worker-{}", uuid::Uuid::new_v4().simple()),
                50,
                chrono::TimeDelta::seconds(60),
            )
            .await
            .map_err(|error| anyhow::anyhow!("desk worker dispatch pass failed: {error}"))?;
        // The explicit audit pass rides every worker run (exact-head
        // review R5): committed intents reach the durable audit sink.
        self.audit
            .dispatch_pending_audit(&self.project_id, 100)
            .await
            .map_err(|error| anyhow::anyhow!("desk worker audit pass failed: {error}"))?;
        Ok(report)
    }
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
    use minco_plugin_health::{HealthPlugin, HealthRegistry};
    use minco_plugin_identity::IdentityPlugin;
    use minco_plugin_jobs::JobsServices;
    use minco_plugin_observability::{ObservabilityConfig, ObservabilityPlugin};
    use minco_plugin_ticketing::{
        SqliteTicketingStore, TicketingConfig, TicketingJobsDeps, TicketingPortalServices,
        TicketingService, TicketingStoreService, register_ticketing_jobs, ticketing_router,
    };

    let pool = migrate(config).await?;

    // Concrete adapter selection lives here and nowhere else.
    let jobs_store = Arc::new(minco_sqlx_sqlite::jobs::SqliteJobStore::new(pool.clone()));
    // The same-transaction enqueue adapter (review finding 3): ticket
    // mutations commit their job records inside the caller's SQLite
    // transaction, so a public reply can never strand its notification.
    let ticketing_store: Arc<dyn minco_plugin_ticketing::TicketingStore> = Arc::new(
        SqliteTicketingStore::new(pool.clone())
            .with_job_enqueue(Arc::new(JobStoreEnqueue(jobs_store.clone()))),
    );
    let registry = Arc::new(minco_plugin_jobs::JobHandlerRegistry::new());
    let clock: Arc<dyn minco_plugin_jobs::JobClock> = Arc::new(minco_plugin_jobs::SystemJobClock);
    let executor = Arc::new(minco_plugin_jobs::JobExecutor::new(Arc::clone(&registry)));
    // The operated dispatch path (review finding 3): claimed due
    // publications execute in-process and commit durable dispositions —
    // never the fail-closed placeholder.
    let jobs = JobsServices::new(
        jobs_store.clone(),
        jobs_store.clone(),
        Arc::new(DurableJobDispatcher {
            executor: Arc::clone(&executor),
            clock: Arc::clone(&clock),
            store: Arc::clone(&jobs_store) as Arc<dyn minco_plugin_jobs::JobStore>,
            publications: Arc::clone(&jobs_store)
                as Arc<dyn minco_plugin_jobs::JobPublicationStore>,
            locks: Arc::clone(&jobs_store) as Arc<dyn minco_plugin_jobs::OverlapLockStore>,
        }),
        jobs_store,
        Arc::clone(&clock),
        Arc::clone(&executor),
    );
    let jobs_handle = Arc::new(jobs);
    let objects = Arc::new(minco_plugin_object_storage::ObjectStoreService::new(
        Arc::new(minco_plugin_object_storage::MemoryObjectStore::default()),
    ));
    let notification_sink = Arc::new(minco_plugin_notifications::MemoryNotificationSink::default());
    let notifications = Arc::new(minco_plugin_notifications::NotificationService::new(
        Arc::clone(&notification_sink) as Arc<dyn minco_plugin_notifications::NotificationSink>,
    ));
    // Durable portal services (review finding 2): requester sessions,
    // CSRF and idempotency survive restarts on the same database.
    let session_store = Arc::new(minco_sqlx_sqlite::plugin_adapters::SqliteSessionStore::new(
        pool.clone(),
    ));
    let sessions = Arc::new(minco_plugin_sessions::SessionService::new(
        Arc::clone(&session_store) as Arc<dyn minco_plugin_sessions::SessionStore>,
    ));
    let csrf = Arc::new(
        minco_plugin_sessions::CsrfService::new(config.csrf_secret.clone())
            .context("the desk CSRF secret must carry sufficient entropy")?,
    );
    let idempotency_store =
        Arc::new(minco_sqlx_sqlite::plugin_adapters::SqliteIdempotencyStore::new(pool.clone()));
    let idempotency = Arc::new(
        minco_plugin_idempotency::IdempotencyService::new(
            Arc::clone(&idempotency_store) as Arc<dyn minco_plugin_idempotency::IdempotencyStore>,
            chrono::TimeDelta::seconds(300),
        )
        .context("compose the desk idempotency service")?,
    );
    // The in-process event bus: ticketing keeps durable intents and the
    // outbox claim mediates single publication; subscribers are local
    // by design in the desk profile (no external broker).
    let (_events_plugin, events_bus) = minco_plugin_events::EventsPlugin::memory();
    let events = Arc::new(minco_plugin_events::EventServices {
        publisher: Arc::clone(&events_bus) as Arc<dyn minco_plugin_events::EventPublisher>,
        outbox: Arc::clone(&events_bus) as Arc<dyn minco_plugin_events::OutboxStore>,
    });
    // Semantic audit rides the same durable sink the plugin graph
    // registers (exact-head review R5).
    let audit_sink: Arc<dyn minco_plugin_audit::AuditSink> = Arc::new(
        minco_sqlx_sqlite::plugin_adapters::SqliteAuditSink::new(pool.clone()),
    );
    let audit = Arc::new(minco_plugin_audit::AuditService(Arc::clone(&audit_sink)));

    let service = Arc::new(
        TicketingService::new(
            TicketingStoreService::new(Arc::clone(&ticketing_store)),
            TicketingConfig {
                project_id: config.project_id.clone(),
                portal_origin: config.portal_origin.clone(),
                notify_requester_on_public_reply: true,
                allowed_return_paths: config.allowed_return_paths.clone(),
                ..TicketingConfig::default()
            },
        )?
        .with_portal_services(TicketingPortalServices {
            sessions: Some(sessions),
            csrf: Some(csrf),
            idempotency: Some(idempotency),
            events: Some(events),
            audit: Some(audit),
            jobs: Some(Arc::clone(&jobs_handle)),
            objects: Some(objects.clone()),
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
    // The graph registers the same concrete adapters the composition
    // uses — sqlite stores and the shared sinks — so the selection
    // describes the real desk, not decorative memory defaults.
    manager.register(minco_plugin_sessions::SessionsPlugin::new(
        Arc::clone(&session_store) as Arc<dyn minco_plugin_sessions::SessionStore>,
    ))?;
    manager.register(minco_plugin_idempotency::IdempotencyPlugin::new(
        Arc::clone(&idempotency_store) as Arc<dyn minco_plugin_idempotency::IdempotencyStore>,
    ))?;
    manager.register(minco_plugin_notifications::NotificationsPlugin::new(
        Arc::clone(&notification_sink) as Arc<dyn minco_plugin_notifications::NotificationSink>,
    ))?;
    manager.register(minco_plugin_events::EventsPlugin::new(
        Arc::clone(&events_bus) as Arc<dyn minco_plugin_events::EventPublisher>,
        Arc::clone(&events_bus) as Arc<dyn minco_plugin_events::OutboxStore>,
    ))?;
    manager.register(minco_plugin_audit::AuditPlugin::new(Arc::clone(
        &audit_sink,
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

    let desk_router =
        ticketing_router(minco_plugin_ticketing::TicketingService::clone(&service)).layer(
            axum::middleware::from_fn_with_state(config.agent_token.clone(), desk_agent_identity),
        );
    // Development identity headers are deliberately NOT allowed: the
    // desk's trust boundary is the session cookie plus the loopback
    // service bearer token (review finding 2).
    let header_policy = minco_http::HttpHeaderPolicy::default();
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
        worker: DeskWorker {
            jobs: jobs_handle,
            audit: Arc::clone(&service),
            project_id: config.project_id.clone(),
        },
        health_report,
    })
}

/// Retention erasure (ADR-0073): deletes resolved-or-closed tickets
/// last updated before the cutoff, cascading children; bounded. This is
/// an explicit operator operation — nothing schedules it implicitly.
#[cfg(feature = "sqlite")]
pub async fn erase_resolved_before(
    config: &DeskConfig,
    cutoff: chrono::DateTime<chrono::Utc>,
    limit: usize,
) -> anyhow::Result<usize> {
    let pool = migrate(config).await?;
    let store = minco_plugin_ticketing::TicketingStoreService::new(Arc::new(
        minco_plugin_ticketing::SqliteTicketingStore::new(pool),
    ));
    Ok(store
        .erase_tickets_resolved_before(&config.project_id, cutoff, limit)
        .await?)
}

#[cfg(not(feature = "sqlite"))]
pub async fn build_desk(_config: &DeskConfig) -> Result<BuiltDesk> {
    bail!("the standalone desk requires the sqlite feature")
}

/// The same-transaction job enqueue adapter: the composition root binds
/// the released `SqliteJobStore::enqueue_in` behind ticketing's port,
/// sharing one pool so job records commit with the ticket mutation
/// (ADR-0054, review finding 3).
#[cfg(feature = "sqlite")]
#[derive(Debug)]
struct JobStoreEnqueue(Arc<minco_sqlx_sqlite::jobs::SqliteJobStore>);

#[cfg(feature = "sqlite")]
#[async_trait::async_trait]
impl minco_plugin_ticketing::TicketingJobEnqueue for JobStoreEnqueue {
    async fn enqueue_in(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        record: minco_plugin_jobs::JobRecord,
    ) -> Result<(), minco_plugin_ticketing::TicketStoreError> {
        self.0
            .enqueue_in(transaction, record)
            .await
            .map(|_| ())
            .map_err(|error| {
                minco_plugin_ticketing::TicketStoreError::Infrastructure(error.to_string())
            })
    }
}

/// The desk's in-process dispatcher: a claimed publication executes
/// through the durable executor path — claim execution, run the handler,
/// commit the disposition — before the publication is acknowledged.
#[cfg(feature = "sqlite")]
struct DurableJobDispatcher {
    executor: Arc<minco_plugin_jobs::JobExecutor>,
    clock: Arc<dyn minco_plugin_jobs::JobClock>,
    store: Arc<dyn minco_plugin_jobs::JobStore>,
    publications: Arc<dyn minco_plugin_jobs::JobPublicationStore>,
    locks: Arc<dyn minco_plugin_jobs::OverlapLockStore>,
}

#[cfg(feature = "sqlite")]
impl std::fmt::Debug for DurableJobDispatcher {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("DurableJobDispatcher").finish()
    }
}

#[cfg(feature = "sqlite")]
#[async_trait::async_trait]
impl minco_plugin_jobs::JobDispatcher for DurableJobDispatcher {
    async fn dispatch(
        &self,
        delivery: &minco_plugin_jobs::JobDelivery,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), minco_plugin_jobs::JobError> {
        let _ = now;
        let worker = format!("desk-dispatch-{}", uuid::Uuid::new_v4().simple());
        let disposition = self
            .executor
            .run(
                &delivery.envelope,
                &worker,
                self.clock.as_ref(),
                self.store.as_ref(),
                self.publications.as_ref(),
                self.locks.as_ref(),
            )
            .await?;
        if let minco_plugin_jobs::JobRunDisposition::Executed(
            minco_plugin_jobs::JobExecutionDisposition::FailedPermanently { code, .. },
        ) = &disposition
        {
            return Err(minco_plugin_jobs::JobError::InvalidJob(format!(
                "job executed to permanent failure: {code}"
            )));
        }
        Ok(())
    }
}

/// The desk trust boundary: `Authorization: Bearer <agent token>` maps to
/// the loopback service principal (full agent capability set) and every
/// other request stays anonymous until a requester session cookie
/// resolves. Forged development headers are never trusted.
#[cfg(feature = "sqlite")]
async fn desk_agent_identity(
    axum::extract::State(agent_token): axum::extract::State<String>,
    mut request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let authorized = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| constant_time_eq(token.as_bytes(), agent_token.as_bytes()));
    if authorized
        && request
            .extensions()
            .get::<minco_http::Principal>()
            .is_none()
    {
        request.extensions_mut().insert(minco_http::Principal {
            subject: "desk-agent".into(),
            permissions: [
                "ticketing.create",
                "ticketing.reply",
                "ticketing.manage",
                "ticketing.ingest",
                "ticketing.integrate",
                "ticketing.ai-context",
                "ticketing.agent-console",
                "ticketing.agent.read",
                "ticketing.agent.manage",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            claims: BTreeMap::default(),
        });
    }
    next.run(request).await
}

/// Constant-time byte-slice comparison for bearer token checks.
#[cfg(feature = "sqlite")]
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
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
