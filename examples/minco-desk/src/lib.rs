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
//!
//! The desk is an isolated deployment (ADR-0076): startup provisions
//! exactly one workspace identity into this database (under the
//! workspace plugin's own migration ledger), registers the configured
//! project verbatim, and resolves the service-principal scope once —
//! the bearer middleware then injects that checked, scope-carrying
//! principal instead of minting one.
//!
//! # Route inventory (ADR-0076)
//!
//! Every route the desk serves falls into exactly one class:
//!
//! - **Business routes** (the ticketing agent, requester, ingest,
//!   integrate, ai-context, console, search, views, macros,
//!   clarifications, automation and management operations under
//!   `/_minco/ticketing`) require a resolved scoped caller: the bearer
//!   path injects the provisioned desk-agent principal, and requester
//!   paths resolve a session whose bound project was validated against
//!   the registry. Scopeless or foreign-scoped principals are denied
//!   with `ticketing_scope_denied` before any business effect.
//! - **Session/handoff exchange routes** (`POST /handoffs/exchange`,
//!   `POST /tickets/from-handoff`, `POST /requester/sessions`,
//!   `POST /integrations/handoffs`, `POST /requester/logout`) validate
//!   their own bound grant or credential — a one-time handoff token, a
//!   handoff-minted session, or the integration permission — and never
//!   require the session they are creating.
//! - **Public assets, preflight and operational probes**
//!   (`/support-entry.js`, `/bootstrap`, `/agent` console assets,
//!   CORS preflights, `/live`, `/ready`) carry explicit public or
//!   deployment-operational policies; no fabricated authenticated
//!   identity is ever injected for them.
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
    /// Inbound sender-authentication posture (exact-head review
    /// R30/P0-4), loaded from `DESK_INBOUND_AUTH_POLICY`
    /// (`local_trusted` | `require_aligned_spf` | `require_aligned_dkim`
    /// | `require_dmarc` | `reject_any_auth_failure`). Defaults to the
    /// channel-trusting local profile.
    pub inbound_auth_policy: minco_plugin_ticketing::InboundAuthPolicy,
    /// Scan-verdict enforcement (exact-head review R30/P0-4), loaded
    /// from `DESK_INBOUND_SCAN_VERDICTS` (`local` | `require_clean`).
    /// The SES production profile must set `require_clean`: a missing
    /// spam/virus verdict is never a silent pass.
    pub inbound_scan_verdicts: minco_plugin_ticketing::ScanVerdictPolicy,
    /// The authserv-id the receiving provider stamps, loaded from
    /// `DESK_INBOUND_AUTHSERV_ID` (default `amazonses.com`).
    pub inbound_authserv_id: String,
    /// Optional pinned workspace identity (ADR-0076), loaded from
    /// `DESK_WORKSPACE_ID`. When set it must equal the identity already
    /// bound to this database on every later startup; a different pin
    /// fails closed instead of rebinding. When unset, first provisioning
    /// mints an identity and every restart reuses the persisted binding.
    pub workspace_id: Option<String>,
    /// Workspace display name (a label, never an authority identifier),
    /// loaded from `DESK_WORKSPACE_DISPLAY_NAME`.
    pub workspace_display_name: String,
}

impl DeskConfig {
    /// Loads the configuration; `DESK_DATABASE_URL` defaults to a local
    /// `SQLite` file so a clean clone runs without external setup.
    /// Loads the configuration. In non-local environments startup fails
    /// closed when the agent credential, CSRF key or database URL are
    /// absent or trivial (exact-head review R10): generated secrets are
    /// acceptable only for the local development profile.
    pub fn from_env() -> Result<Self> {
        let environment = std::env::var("DESK_ENVIRONMENT").unwrap_or_else(|_| "local".into());
        let database_url = std::env::var("DESK_DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://minco-desk.sqlite?mode=rwc".into());
        // Credential sources (exact-head review R33/P1-3): a `_FILE`
        // variable wins over the plain one and reads the secret from a
        // file — the secret-manager/reference pattern, and the rotation
        // story (update the file, restart; the value is never argv or
        // logged).
        let agent_token = read_credential("DESK_AGENT_TOKEN");
        let csrf_secret = read_credential("DESK_CSRF_SECRET");
        if environment != "local" {
            for (name, value) in [
                ("DESK_AGENT_TOKEN", &agent_token),
                ("DESK_CSRF_SECRET", &csrf_secret),
            ] {
                if value.1 {
                    anyhow::bail!(
                        "{name} must be set explicitly in non-local environments \
                         ({environment}); generated credentials are local-only"
                    );
                }
            }
            for (name, value) in [
                ("DESK_AGENT_TOKEN", agent_token.0.as_str()),
                ("DESK_CSRF_SECRET", csrf_secret.0.as_str()),
            ] {
                validate_non_local_secret(name, value)?;
            }
            let lowered = database_url.to_ascii_lowercase();
            if !lowered.starts_with("sqlite://")
                || !lowered.contains("mode=rwc")
                || lowered.contains(":memory:")
            {
                anyhow::bail!(
                    "DESK_DATABASE_URL must be an explicit persistent read-write \
                     SQLite URL (mode=rwc, no :memory:) in non-local environments"
                );
            }
            let portal_origin = std::env::var("DESK_PORTAL_ORIGIN").map_err(|_| {
                anyhow::anyhow!(
                    "DESK_PORTAL_ORIGIN must be set explicitly in non-local environments"
                )
            })?;
            if !portal_origin.starts_with("https://")
                || portal_origin.contains('*')
                || portal_origin.contains('#')
                || portal_origin.contains('@')
                || portal_origin.ends_with('/')
                || portal_origin.len() > 2_048
            {
                anyhow::bail!(
                    "DESK_PORTAL_ORIGIN must be an exact HTTPS origin \
                     (scheme + host + optional port, no wildcard, credentials, \
                     fragment or trailing slash) in non-local environments"
                );
            }
            if std::env::var("DESK_ALLOWED_ORIGINS").is_err() {
                anyhow::bail!(
                    "DESK_ALLOWED_ORIGINS must be set explicitly in non-local environments"
                );
            }
        }
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
            environment: environment.clone(),
            agent_token: agent_token.0,
            csrf_secret: csrf_secret.0,
            allowed_return_paths: {
                let raw = match std::env::var("DESK_ALLOWED_RETURN_PATHS") {
                    Ok(value) => value,
                    Err(_) if environment == "local" => {
                        "http://127.0.0.1:8090=/_minco/ticketing".into()
                    }
                    Err(_) => {
                        anyhow::bail!(
                            "DESK_ALLOWED_RETURN_PATHS must be set explicitly in \
                             non-local environments"
                        )
                    }
                };
                if environment != "local" && raw.starts_with("http://") {
                    anyhow::bail!(
                        "DESK_ALLOWED_RETURN_PATHS must use HTTPS origins in \
                         non-local environments"
                    );
                }
                parse_return_paths(&raw)
            },
            inbound_auth_policy: match std::env::var("DESK_INBOUND_AUTH_POLICY")
                .unwrap_or_else(|_| "local_trusted".into())
                .to_ascii_lowercase()
                .as_str()
            {
                "local_trusted" => minco_plugin_ticketing::InboundAuthPolicy::LocalTrusted,
                "require_aligned_spf" => {
                    minco_plugin_ticketing::InboundAuthPolicy::RequireAlignedSpf
                }
                "require_aligned_dkim" => {
                    minco_plugin_ticketing::InboundAuthPolicy::RequireAlignedDkim
                }
                "require_dmarc" => minco_plugin_ticketing::InboundAuthPolicy::RequireDmarc,
                "reject_any_auth_failure" => {
                    minco_plugin_ticketing::InboundAuthPolicy::RejectAnyAuthFailure
                }
                other => anyhow::bail!("unknown DESK_INBOUND_AUTH_POLICY: {other}"),
            },
            inbound_scan_verdicts: match std::env::var("DESK_INBOUND_SCAN_VERDICTS")
                .unwrap_or_else(|_| "local".into())
                .to_ascii_lowercase()
                .as_str()
            {
                "local" => minco_plugin_ticketing::ScanVerdictPolicy::Local,
                "require_clean" => minco_plugin_ticketing::ScanVerdictPolicy::RequireClean,
                other => anyhow::bail!("unknown DESK_INBOUND_SCAN_VERDICTS: {other}"),
            },
            inbound_authserv_id: std::env::var("DESK_INBOUND_AUTHSERV_ID")
                .unwrap_or_else(|_| "amazonses.com".into()),
            workspace_id: match std::env::var("DESK_WORKSPACE_ID") {
                Ok(value) if !value.trim().is_empty() => {
                    minco_plugin_workspace::WorkspaceId::try_from(value.clone())
                        .map_err(|error| anyhow::anyhow!("DESK_WORKSPACE_ID: {error}"))?;
                    Some(value)
                }
                _ => None,
            },
            workspace_display_name: std::env::var("DESK_WORKSPACE_DISPLAY_NAME")
                .unwrap_or_else(|_| "Default workspace".into()),
        })
    }
}

/// Reads one credential: `(value, was_generated)` — the flag drives
/// non-local fail-closed startup (exact-head review R10). A `{name}_FILE`
/// variable wins over `{name}` and reads the secret from a file
/// (exact-head review R33/P1-3): the secret-manager/reference pattern —
/// rotation is "update the file, restart", and the value never appears
/// in argv or logs.
fn read_credential(name: &str) -> (String, bool) {
    if let Ok(path) = std::env::var(format!("{name}_FILE"))
        && let Ok(contents) = std::fs::read_to_string(&path)
        && !contents.trim().is_empty()
    {
        return (contents.trim().to_owned(), false);
    }
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => (value, false),
        _ => (
            format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            ),
            true,
        ),
    }
}

/// Non-local credential strength (exact-head review R33/P1-3): a length
/// plus distinct-character check alone accepts predictable repeated
/// patterns. Accept EITHER machine material that decodes (hex or
/// standard base64) to at least 32 random bytes, OR a 32+ character
/// passphrase with at least 16 distinct characters.
fn validate_non_local_secret(name: &str, value: &str) -> anyhow::Result<()> {
    let trimmed = value.trim();
    if let Some(decoded) = decode_secret_material(trimmed) {
        if decoded >= 32 {
            return Ok(());
        }
        anyhow::bail!(
            "{name} looks like encoded key material but decodes to only {decoded} bytes; \
             at least 32 random bytes (64 hex characters / 43 base64 characters) are required \
             in non-local environments"
        );
    }
    if trimmed.len() < 32 {
        anyhow::bail!(
            "{name} must carry at least 32 characters in non-local environments \
             (or hex/base64-encoded material decoding to at least 32 bytes)"
        );
    }
    if trimmed
        .chars()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        < 16
    {
        anyhow::bail!(
            "{name} passphrase must carry real entropy (at least 16 distinct characters) \
             or be hex/base64-encoded material decoding to at least 32 random bytes"
        );
    }
    Ok(())
}

/// Returns the decoded byte length when the value is recognizable hex
/// or standard base64 key material.
fn decode_secret_material(value: &str) -> Option<usize> {
    if value.len() >= 32
        && value.len().is_multiple_of(2)
        && value.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Some(value.len() / 2);
    }
    if value.len() >= 43 {
        use base64::Engine as _;
        return base64::engine::general_purpose::STANDARD
            .decode(value)
            .ok()
            .map(|decoded: Vec<u8>| decoded.len());
    }
    None
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
    /// The workspace provisioning outcome (ADR-0076): the bound identity
    /// and whether this build created the deployment binding.
    pub workspace_report: minco_plugin_workspace::ProvisionReport,
    /// The checked, scope-carrying principal the bearer middleware
    /// injects — resolved once from the provisioned desk-agent profile,
    /// never minted per request.
    pub agent_principal: minco_http::Principal,
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
    /// One bounded worker cycle (exact-head review R16): Jobs dispatch,
    /// Domain-Events delivery and Audit dispatch each run INDEPENDENTLY —
    /// a failing job store can never starve event delivery or audit — and
    /// the cycle fails only after every pass was attempted, with each
    /// failure preserved in the error chain.
    pub async fn run_once(&self) -> Result<minco_plugin_jobs::DispatchReport, anyhow::Error> {
        let mut failures: Vec<String> = Vec::new();
        let report = match self
            .jobs
            .dispatch_due_once(
                &format!("desk-worker-{}", uuid::Uuid::new_v4().simple()),
                50,
                chrono::TimeDelta::seconds(60),
            )
            .await
        {
            Ok(report) => report,
            Err(error) => {
                failures.push(format!("jobs dispatch pass failed: {error}"));
                minco_plugin_jobs::DispatchReport::default()
            }
        };
        // The durable activity intents reach the Events outbox (the
        // required dependency becomes a runtime truth).
        if let Err(error) = self
            .audit
            .dispatch_pending_activity(&self.project_id, 100)
            .await
        {
            failures.push(format!("domain events dispatch pass failed: {error}"));
        }
        // The audit pass rides every cycle regardless of the others.
        if let Err(error) = self
            .audit
            .dispatch_pending_audit(&self.project_id, 100)
            .await
        {
            failures.push(format!("audit dispatch pass failed: {error}"));
        }
        if failures.is_empty() {
            Ok(report)
        } else {
            Err(anyhow::anyhow!(failures.join("; ")))
        }
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
    // The workspace isolation registry (ADR-0076) under its own ledger —
    // bootstrap order: plugin storage, ticketing, workspace. It never
    // runs under the sqlx default ledger or another plugin's ledger.
    minco_plugin_workspace::SqliteWorkspaceStore::new(pool.clone())
        .migrate()
        .await
        .context("apply workspace-isolation migrations")?;
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

    // Workspace isolation (ADR-0076): provisioning is an explicit startup
    // phase that binds one workspace identity to this database, registers
    // the configured project verbatim, and seeds the desk-agent profile.
    // Conflicting pins, drifted policy, and second workspaces fail closed
    // here — before any service serves a request.
    let desk_project = minco_plugin_workspace::ProjectId::try_from(config.project_id.clone())
        .context("DESK_PROJECT_ID is not a registrable project identifier")?;
    // Upgrade inventory (round 1 finding 5): every project identity that
    // exists in this database — live tickets AND durable security
    // artifacts (handoffs, exchange grants, external messages, activity
    // intents, operation receipts) — registers verbatim alongside the
    // configured project, so a populated database never ends up with a
    // registry that omits its own history.
    let mut inventoried: BTreeSet<String> = BTreeSet::from_iter([desk_project.as_str().to_owned()]);
    for table in [
        "ticketing_tickets",
        "ticketing_handoffs",
        "ticketing_session_exchange_grants",
        "ticketing_external_messages",
        "ticketing_activity_intents",
        "ticketing_operation_receipts",
    ] {
        let projects: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(
            format!("SELECT DISTINCT project_id FROM {table}").as_str(),
        ))
        .fetch_all(&pool)
        .await
        .with_context(|| format!("inventory {table} for the workspace registry"))?;
        inventoried.extend(projects);
    }
    let registered_projects: Vec<minco_plugin_workspace::ProjectId> = inventoried
        .into_iter()
        .map(|project| {
            minco_plugin_workspace::ProjectId::try_from(project)
                .context("an inventoried project identifier is not registrable")
        })
        .collect::<Result<Vec<_>>>()?;
    let pinned_workspace = match &config.workspace_id {
        Some(pin) => Some(
            minco_plugin_workspace::WorkspaceId::try_from(pin.clone())
                .context("the pinned DESK_WORKSPACE_ID is invalid")?,
        ),
        None => None,
    };
    let workspace_store = Arc::new(minco_plugin_workspace::SqliteWorkspaceStore::new(
        pool.clone(),
    ));
    let workspace_service = Arc::new(
        minco_plugin_workspace::WorkspaceService::new(
            minco_plugin_workspace::WorkspaceStoreService::new(workspace_store.clone()),
            minco_plugin_workspace::WorkspaceConfig {
                display_name: config.workspace_display_name.clone(),
                workspace_id: pinned_workspace,
                projects: registered_projects,
                profiles: [
                    (
                        desk_agent_profile_id(),
                        minco_plugin_workspace::ProfileSpec {
                            kind: minco_plugin_workspace::ProfileKind::ServiceApi,
                            auth_mode: minco_plugin_workspace::ProfileAuthMode::ServiceBearerToken,
                            bound_project: desk_project.clone(),
                            service_subject: "desk-agent".into(),
                            permission_ceiling: DESK_AGENT_PERMISSIONS
                                .iter()
                                .map(|permission| (*permission).to_owned())
                                .collect(),
                            allowed_origins: std::iter::once(config.portal_origin.clone())
                                .collect(),
                            resource_types: BTreeSet::new(),
                            secret_reference: Some("DESK_AGENT_TOKEN".into()),
                        },
                    ),
                    // The requester portal's persisted profile (round 1
                    // finding 3): cookie sessions are minted only through
                    // the handoff exchange, whose origin policy this
                    // profile records. The startup check below proves the
                    // composition's portal configuration still matches it.
                    (
                        desk_portal_profile_id(),
                        minco_plugin_workspace::ProfileSpec {
                            kind: minco_plugin_workspace::ProfileKind::Portal,
                            auth_mode: minco_plugin_workspace::ProfileAuthMode::PortalSession,
                            bound_project: desk_project.clone(),
                            service_subject: "desk-portal".into(),
                            permission_ceiling: [
                                "ticketing.requester.read",
                                "ticketing.requester.write",
                            ]
                            .map(str::to_owned)
                            .into_iter()
                            .collect(),
                            allowed_origins: std::iter::once(config.portal_origin.clone())
                                .collect(),
                            resource_types: BTreeSet::new(),
                            secret_reference: None,
                        },
                    ),
                    // The inbound-mail worker's persisted profile (round 1
                    // finding 3): the worker principal below resolves
                    // through this profile, never an anonymous
                    // configuration default.
                    (
                        desk_mail_profile_id(),
                        minco_plugin_workspace::ProfileSpec {
                            kind: minco_plugin_workspace::ProfileKind::Email,
                            auth_mode: minco_plugin_workspace::ProfileAuthMode::InboundEmail,
                            bound_project: desk_project.clone(),
                            service_subject: "desk-mail-worker".into(),
                            permission_ceiling: ["ticketing.ingest"]
                                .map(str::to_owned)
                                .into_iter()
                                .collect(),
                            allowed_origins: BTreeSet::new(),
                            resource_types: BTreeSet::new(),
                            secret_reference: None,
                        },
                    ),
                ]
                .into_iter()
                .collect(),
                grants: BTreeMap::new(),
            },
        )
        .context("compose the workspace isolation service")?,
    );
    let workspace_report = workspace_service
        .provision()
        .await
        .context("provision the workspace isolation registry")?;
    // Ownership binding (round 1 finding 5), in the same explicit
    // startup phase with writers not yet serving: every inventoried
    // project is now registered under the provisioned workspace, so the
    // ticket table can take the composite ownership constraint; legacy
    // exchange grants (issued before isolation) are deterministically
    // bound to the provisioned workspace. Interruption before this
    // point leaves the un-bound (default '') schema in place and the
    // next startup redoes the whole phase idempotently.
    SqliteTicketingStore::new(pool.clone())
        .bind_workspace_ownership(workspace_report.workspace.as_str())
        .await
        .context("bind ticket ownership to the provisioned workspace")?;
    sqlx::query(
        "UPDATE ticketing_session_exchange_grants SET workspace_id = ? WHERE workspace_id IS NULL",
    )
    .bind(workspace_report.workspace.as_str())
    .execute(&pool)
    .await
    .context("bind legacy session-exchange grants to the provisioned workspace")?;
    // The service-principal scope is resolved once and served through this
    // explicit local context (ADR-0076: no per-request discovery); the
    // bearer middleware injects the resulting checked principal.
    let agent_scope = workspace_service
        .resolve_service_principal_scope(&desk_agent_profile_id())
        .await
        .context("resolve the desk-agent service principal scope")?;
    let agent_principal = minco_http::Principal {
        subject: agent_scope.service_subject.clone(),
        permissions: agent_scope.permission_ceiling.iter().cloned().collect(),
        claims: BTreeMap::default(),
    }
    .with_scopes(agent_scope.scope.scope_tokens());
    // The principal scope claim is whitespace-tokenized (`minco-http`), so
    // identifiers containing whitespace cannot ride it. Verify the round
    // trip once at startup and fail closed with a precise error instead of
    // silently dropping a scope token.
    minco_plugin_workspace::ProjectScope::from_scope_tokens(
        &agent_principal
            .claims
            .get(minco_http::PRINCIPAL_SCOPES_CLAIM)
            .map(|value| {
                value
                    .split_ascii_whitespace()
                    .map(str::to_owned)
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default(),
    )
    .context(
        "the resolved workspace/project scope cannot be carried through the \
         principal scope claim: identifiers containing whitespace are not \
         representable on this boundary",
    )?;

    // The portal profile's persisted policy must still match this
    // composition's portal configuration (round 1 finding 3): sessions
    // are minted only through the handoff exchange, whose origin this
    // profile records — startup fails closed on drift.
    let portal_scope = workspace_service
        .resolve_service_principal_scope(&desk_portal_profile_id())
        .await
        .context("resolve the desk-portal profile")?;
    if !portal_scope.allowed_origins.contains(&config.portal_origin) {
        anyhow::bail!(
            "the persisted desk-portal profile no longer allows the configured \
             portal origin; explicit reconciliation is required"
        );
    }

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
                inbound_auth_policy: config.inbound_auth_policy,
                inbound_scan_verdicts: config.inbound_scan_verdicts,
                inbound_authserv_id: config.inbound_authserv_id.clone(),
                // Isolation (ADR-0076): the desk binds ticketing to the
                // provisioned workspace — every exposed operation then
                // requires the caller's resolved scope to match.
                workspace_isolation: true,
                workspace_id: Some(workspace_report.workspace.as_str().to_owned()),
                // Resource-type policy propagated from the resolved
                // agent profile (round 1 finding 3): consumed where
                // ticket creation accepts resource references.
                allowed_resource_types: Some(agent_scope.resource_types.clone()),
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
    // The durable worker principal holds only ingest authority, bound to
    // the provisioned workspace/project scope (ADR-0076): worker execution
    // cannot adopt a scope broader than the deployment's registered
    // project.
    // The durable worker principal resolves through the persisted email
    // profile (round 1 finding 3): subject, ingest ceiling, and scope
    // all come from the profile — never an anonymous configuration
    // default — so misrouted execution cannot adopt a broader identity.
    let worker_scope = workspace_service
        .resolve_service_principal_scope(&desk_mail_profile_id())
        .await
        .context("resolve the desk-mail worker profile")?;
    let worker = minco_plugin_identity::Identity {
        subject: worker_scope.service_subject.clone(),
        permissions: worker_scope.permission_ceiling.iter().cloned().collect(),
        scopes: worker_scope.scope.scope_tokens().into_iter().collect(),
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
    health.register(Arc::new(JobsStoreHealth { pool: pool.clone() }));
    // Readiness coverage (exact-head review R33/P1-3): the subsystems a
    // request actually traverses — sessions, idempotency receipts, the
    // audit dispatch backlog and object storage — each report their own
    // readiness instead of one pool check standing in for all of them.
    // They are non-critical: liveness stays the critical subset.
    health.register(Arc::new(SessionsTableHealth { pool: pool.clone() }));
    health.register(Arc::new(IdempotencyTableHealth { pool }));
    health.register(Arc::new(AuditBacklogHealth {
        store: TicketingStoreService::new(Arc::clone(&ticketing_store)),
        project_id: config.project_id.clone(),
        threshold: AUDIT_BACKLOG_READINESS_THRESHOLD,
    }));
    health.register(Arc::new(ObjectStorageHealth {
        objects: Arc::clone(&objects),
    }));
    // The isolation registry is critical: a desk whose workspace binding
    // is unreachable cannot safely resolve scope for any caller.
    health.register(Arc::new(WorkspaceStoreHealth {
        service: Arc::clone(&workspace_service),
    }));
    let health_report =
        serde_json::to_value(&composed.graph).unwrap_or_else(|_| serde_json::json!({}));

    // Real liveness and readiness routes (exact-head review R10): the
    // registered checks execute on demand instead of shipping a static
    // composition graph.
    // Liveness answers "is the process up" from the critical subset;
    // readiness answers "can it serve traffic" from every registered
    // check including non-critical ones (exact-head review R26/P1-3).
    let liveness_registry = health.clone();
    let liveness = move || async move {
        let results = liveness_registry.run().await;
        let alive = results
            .iter()
            .filter(|result| result.critical)
            .all(|result| result.ready);
        if alive {
            axum::http::StatusCode::OK
        } else {
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        }
    };
    let readiness_registry = health;
    let readiness = move || async move {
        let results = readiness_registry.run().await;
        let ready = results.iter().all(|result| result.ready);
        if ready {
            axum::http::StatusCode::OK
        } else {
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        }
    };
    let desk_router = ticketing_router(minco_plugin_ticketing::TicketingService::clone(&service))
        .route("/live", axum::routing::get(liveness))
        .route("/ready", axum::routing::get(readiness))
        .layer(axum::middleware::from_fn_with_state(
            DeskAgentAuth {
                token: config.agent_token.clone(),
                principal: agent_principal.clone(),
                allowed_origins: agent_scope.allowed_origins.clone(),
            },
            desk_agent_identity,
        ));
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
        workspace_report,
        agent_principal,
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

/// The bearer-checked desk-agent state: the shared secret, the
/// scope-carrying principal resolved once at startup from the provisioned
/// desk-agent profile, and the profile's exact-origin restriction
/// enforced at ingress (ADR-0076, round 1 finding 3). Origins are a
/// restriction on the authenticated service call — never an
/// authentication factor.
#[cfg(feature = "sqlite")]
#[derive(Clone)]
struct DeskAgentAuth {
    token: String,
    principal: minco_http::Principal,
    allowed_origins: BTreeSet<String>,
}

/// The desk trust boundary: `Authorization: Bearer <agent token>` maps to
/// the loopback service principal (the profile's bounded capability
/// ceiling plus its resolved workspace/project scope) and every other
/// request stays anonymous until a requester session cookie resolves.
/// Forged development headers are never trusted.
#[cfg(feature = "sqlite")]
async fn desk_agent_identity(
    axum::extract::State(auth): axum::extract::State<DeskAgentAuth>,
    mut request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let authorized = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| constant_time_eq(token.as_bytes(), auth.token.as_bytes()));
    if !authorized {
        return next.run(request).await;
    }
    // Profile origin restriction (round 1 finding 3): an authenticated
    // service call carrying an Origin header must carry one of the
    // profile's exact origins. Origin never authenticates — a missing
    // Origin (non-browser service call) is unrestricted, a present
    // Origin outside the profile is denied before any business handler.
    if let Some(origin) = request
        .headers()
        .get(axum::http::header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        && !auth.allowed_origins.contains(origin)
    {
        return origin_denied(origin);
    }
    if request
        .extensions()
        .get::<minco_http::Principal>()
        .is_none()
    {
        request.extensions_mut().insert(auth.principal.clone());
    }
    next.run(request).await
}

/// The origin-restriction denial: a stable problem response, never a
/// hint about which origins are allowed.
#[cfg(feature = "sqlite")]
fn origin_denied(origin: &str) -> axum::response::Response {
    if origin.is_ascii() && origin.len() <= 256 {
        tracing::warn!(
            origin,
            "desk-agent profile origin restriction denied a request"
        );
    } else {
        tracing::warn!("desk-agent profile origin restriction denied a malformed origin");
    }
    axum::http::Response::builder()
        .status(axum::http::StatusCode::FORBIDDEN)
        .header(axum::http::header::CONTENT_TYPE, "application/problem+json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "type": "about:blank",
                "title": "Origin not permitted for this integration profile",
                "status": 403,
                "detail": "The request's origin is outside this profile's policy.",
            })
            .to_string(),
        ))
        .expect("static problem response")
}

/// The desk-agent integration profile identifier provisioned by this
/// composition.
#[cfg(feature = "sqlite")]
fn desk_agent_profile_id() -> minco_plugin_workspace::ProfileId {
    minco_plugin_workspace::ProfileId::try_from("desk-agent".to_owned())
        .expect("static profile identifier")
}

/// The requester-portal profile identifier provisioned by this
/// composition (round 1 finding 3).
#[cfg(feature = "sqlite")]
fn desk_portal_profile_id() -> minco_plugin_workspace::ProfileId {
    minco_plugin_workspace::ProfileId::try_from("desk-portal".to_owned())
        .expect("static profile identifier")
}

/// The inbound-mail worker profile identifier provisioned by this
/// composition (round 1 finding 3).
#[cfg(feature = "sqlite")]
fn desk_mail_profile_id() -> minco_plugin_workspace::ProfileId {
    minco_plugin_workspace::ProfileId::try_from("desk-mail".to_owned())
        .expect("static profile identifier")
}

/// The desk-agent permission ceiling: the full agent capability set the
/// provisioned profile may ever contribute (ADR-0076 — a ceiling that is
/// intersected, never unioned into a human identity).
#[cfg(feature = "sqlite")]
const DESK_AGENT_PERMISSIONS: [&str; 9] = [
    "ticketing.create",
    "ticketing.reply",
    "ticketing.manage",
    "ticketing.ingest",
    "ticketing.integrate",
    "ticketing.ai-context",
    "ticketing.agent-console",
    "ticketing.agent.read",
    "ticketing.agent.manage",
];

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

/// Above this many pending audit dispatch intents, readiness reports
/// degraded: the worker is falling behind the audit trail.
#[cfg(feature = "sqlite")]
const AUDIT_BACKLOG_READINESS_THRESHOLD: usize = 10_000;

/// Sessions subsystem readiness (exact-head review R33/P1-3): the
/// portal's session table answers queries.
#[cfg(feature = "sqlite")]
struct SessionsTableHealth {
    pool: sqlx::SqlitePool,
}

#[cfg(feature = "sqlite")]
impl std::fmt::Debug for SessionsTableHealth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("SessionsTableHealth").finish()
    }
}

#[cfg(feature = "sqlite")]
#[async_trait::async_trait]
impl minco_plugin_health::HealthCheck for SessionsTableHealth {
    fn id(&self) -> &'static str {
        "sessions-store"
    }

    async fn check(&self) -> minco_plugin_health::HealthResult {
        let ready = sqlx::query("SELECT 1 FROM minco_sessions LIMIT 1")
            .execute(&self.pool)
            .await
            .is_ok();
        minco_plugin_health::HealthResult {
            id: "sessions-store".into(),
            ready,
            critical: false,
            detail: (!ready).then(|| "sessions store is not ready".into()),
        }
    }
}

/// Idempotency subsystem readiness (exact-head review R33/P1-3): the
/// receipt table answers queries.
#[cfg(feature = "sqlite")]
struct IdempotencyTableHealth {
    pool: sqlx::SqlitePool,
}

#[cfg(feature = "sqlite")]
impl std::fmt::Debug for IdempotencyTableHealth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("IdempotencyTableHealth").finish()
    }
}

#[cfg(feature = "sqlite")]
#[async_trait::async_trait]
impl minco_plugin_health::HealthCheck for IdempotencyTableHealth {
    fn id(&self) -> &'static str {
        "idempotency-store"
    }

    async fn check(&self) -> minco_plugin_health::HealthResult {
        let ready = sqlx::query("SELECT 1 FROM minco_idempotency LIMIT 1")
            .execute(&self.pool)
            .await
            .is_ok();
        minco_plugin_health::HealthResult {
            id: "idempotency-store".into(),
            ready,
            critical: false,
            detail: (!ready).then(|| "idempotency store is not ready".into()),
        }
    }
}

/// Audit dispatch backlog readiness (exact-head review R33/P1-3): the
/// audit trail degrades when undelivered dispatch intents pile up past
/// the operator threshold.
#[cfg(feature = "sqlite")]
struct AuditBacklogHealth {
    store: minco_plugin_ticketing::TicketingStoreService,
    project_id: String,
    threshold: usize,
}

#[cfg(feature = "sqlite")]
impl std::fmt::Debug for AuditBacklogHealth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("AuditBacklogHealth").finish()
    }
}

#[cfg(feature = "sqlite")]
#[async_trait::async_trait]
impl minco_plugin_health::HealthCheck for AuditBacklogHealth {
    fn id(&self) -> &'static str {
        "audit-backlog"
    }

    async fn check(&self) -> minco_plugin_health::HealthResult {
        let pending = self
            .store
            .pending_audit_intents(&self.project_id, self.threshold + 1)
            .await
            .map(|intents| intents.len());
        let (ready, detail) = match pending {
            Ok(count) if count <= self.threshold => (true, None),
            Ok(count) => (
                false,
                Some(format!("{count} audit dispatch intents are pending")),
            ),
            Err(_) => (false, Some("audit backlog is not observable".into())),
        };
        minco_plugin_health::HealthResult {
            id: "audit-backlog".into(),
            ready,
            critical: false,
            detail,
        }
    }
}

/// Object storage readiness (exact-head review R33/P1-3): a real
/// probe write and delete through the same store inbound mail and
/// quarantine records use.
#[cfg(feature = "sqlite")]
struct ObjectStorageHealth {
    objects: Arc<minco_plugin_object_storage::ObjectStoreService>,
}

#[cfg(feature = "sqlite")]
impl std::fmt::Debug for ObjectStorageHealth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("ObjectStorageHealth").finish()
    }
}

#[cfg(feature = "sqlite")]
#[async_trait::async_trait]
impl minco_plugin_health::HealthCheck for ObjectStorageHealth {
    fn id(&self) -> &'static str {
        "object-storage"
    }

    async fn check(&self) -> minco_plugin_health::HealthResult {
        use minco_plugin_object_storage::{ObjectKey, PutObject};
        let Ok(key) = ObjectKey::parse("health/readiness-probe") else {
            return minco_plugin_health::HealthResult {
                id: "object-storage".into(),
                ready: false,
                critical: false,
                detail: Some("probe key is invalid".into()),
            };
        };
        let put_ok = self
            .objects
            .put(PutObject {
                key: key.clone(),
                bytes: b"readiness".to_vec(),
                content_type: "text/plain".into(),
                attributes: std::collections::BTreeMap::new(),
            })
            .await
            .is_ok();
        let ready = put_ok && self.objects.delete(&key).await.is_ok();
        minco_plugin_health::HealthResult {
            id: "object-storage".into(),
            ready,
            critical: false,
            detail: (!ready).then(|| "object storage is not writable".into()),
        }
    }
}

/// Workspace isolation registry readiness (ADR-0076): the provisioned
/// registry must be reachable for the deployment to resolve scope.
#[cfg(feature = "sqlite")]
struct WorkspaceStoreHealth {
    service: Arc<minco_plugin_workspace::WorkspaceService>,
}

#[cfg(feature = "sqlite")]
impl std::fmt::Debug for WorkspaceStoreHealth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("WorkspaceStoreHealth").finish()
    }
}

#[cfg(feature = "sqlite")]
#[async_trait::async_trait]
impl minco_plugin_health::HealthCheck for WorkspaceStoreHealth {
    fn id(&self) -> &'static str {
        "workspace-store"
    }

    async fn check(&self) -> minco_plugin_health::HealthResult {
        let ready = self.service.store_ready().await.is_ok();
        minco_plugin_health::HealthResult {
            id: "workspace-store".into(),
            ready,
            critical: true,
            detail: (!ready).then(|| "workspace isolation registry is not ready".into()),
        }
    }
}
