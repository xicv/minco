use crate::{
    MemoryTicketingStore, TicketingConfig, TicketingService, TicketingStore, TicketingStoreService,
    ticketing_router,
};
use async_trait::async_trait;
#[cfg(feature = "sqlite")]
use minco_core::MigrationSet;
use minco_core::{
    CapabilityProvision, CapabilityRequirement, ConfigurationField, ConfigurationValueKind,
    DataClass, HealthCheckDescriptor, IdleCostClass, OperationDescriptor, Plugin, PluginContext,
    PluginDescriptor, PluginError, PluginId, PluginStability, ResourceIntent, ResourceKind,
};
use minco_http::{HttpHeaderPolicy, HttpModule};
use minco_plugin_audit::AuditService;
use minco_plugin_events::EventServices;
use minco_plugin_health::{HealthCheck, HealthResult};
use minco_plugin_identity::IdentityService;
use minco_plugin_notifications::NotificationService;
use minco_plugin_object_storage::ObjectStoreService;
use semver::{Version, VersionReq};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StorageProfile {
    Memory,
    Custom,
    #[cfg(feature = "sqlite")]
    Sqlite,
}

#[derive(Clone)]
pub struct TicketingPlugin {
    store: TicketingStoreService,
    storage_profile: StorageProfile,
}

impl std::fmt::Debug for TicketingPlugin {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TicketingPlugin")
            .field("storage_profile", &self.storage_profile)
            .finish_non_exhaustive()
    }
}

impl TicketingPlugin {
    #[must_use]
    pub fn new(store: Arc<dyn TicketingStore>) -> Self {
        Self {
            store: TicketingStoreService::new(store),
            storage_profile: StorageProfile::Custom,
        }
    }

    #[must_use]
    pub fn memory() -> Self {
        Self {
            store: TicketingStoreService::new(Arc::new(MemoryTicketingStore::default())),
            storage_profile: StorageProfile::Memory,
        }
    }

    #[cfg(feature = "sqlite")]
    #[must_use]
    pub fn sqlite(pool: sqlx::SqlitePool) -> Self {
        Self {
            store: TicketingStoreService::new(Arc::new(crate::SqliteTicketingStore::new(pool))),
            storage_profile: StorageProfile::Sqlite,
        }
    }
}

// No `Default` (ADR-0053): store selection must be explicit — `memory()`
// for the deterministic test profile, `sqlite(pool)` or `new(store)` for
// durable profiles — so non-durable storage can never be selected
// silently.

impl Plugin for TicketingPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("ticketing").expect("static plugin ID"),
            Version::new(0, 1, 0),
            "Project-scoped support ticketing with atomic browser handoffs and conversation",
        );
        descriptor.core_compatibility =
            VersionReq::parse(concat!("^", env!("CARGO_PKG_VERSION"))).expect("package version");
        descriptor.stability = PluginStability::Beta;
        descriptor.default_enabled = false;
        descriptor.documentation = Some("https://docs.rs/minco-plugin-ticketing".into());
        descriptor.data_classes.extend([
            DataClass::CustomerProvided,
            DataClass::Personal,
            DataClass::Confidential,
            DataClass::Internal,
        ]);
        descriptor.plugin_dependencies.extend(
            [
                "health",
                "identity",
                "object-storage",
                "notifications",
                "audit",
                "events",
            ]
            .into_iter()
            .map(|id| PluginId::new(id).expect("static plugin ID")),
        );
        descriptor.requires.extend([
            requirement("health.registry"),
            requirement("identity.resolve"),
            requirement("authorization.permissions"),
            requirement("storage.object"),
            requirement("notifications.send"),
            requirement("audit.append"),
            requirement("events.publish"),
            requirement("events.outbox"),
        ]);
        descriptor.provides.extend(
            [
                "ticketing.create",
                "ticketing.read",
                "ticketing.conversation",
                "ticketing.manage",
                "ticketing.integrate",
                "ticketing.ingest",
                "ticketing.attachments",
                "ticketing.ai-context",
                "ticketing.support-entry",
                "ticketing.agent-console",
                "ticketing.agent.read",
                "ticketing.agent.manage",
            ]
            .into_iter()
            .map(provision),
        );
        // No `ticketing.jobs` capability is declared: the descriptor and
        // the distribution manifest must match exactly, and neither can
        // express feature-conditional capabilities (ADR-0054). The Cargo
        // feature and the notify configuration are the opt-in truth.
        descriptor.operations.extend(ticketing_operations());
        match self.storage_profile {
            StorageProfile::Memory => {}
            StorageProfile::Custom => descriptor.resources.push(ResourceIntent {
                id: "ticketing-custom-store".into(),
                kind: ResourceKind::Custom("ticketing-store".into()),
                idle_cost: IdleCostClass::ProviderManaged,
                wake_sources: Vec::new(),
                dependencies: Vec::new(),
            }),
            #[cfg(feature = "sqlite")]
            StorageProfile::Sqlite => descriptor.migrations.push(MigrationSet {
                id: "ticketing-sqlite-v1".into(),
                database: "sqlite".into(),
                path: "migrations/sqlite".into(),
            }),
        }
        descriptor.health_checks.push(HealthCheckDescriptor {
            id: "ticketing-store".into(),
            critical: true,
        });
        descriptor.configuration.extend(configuration_fields());
        descriptor
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        let config = context.configuration::<TicketingConfig>()?;
        {
            let services = context.services();
            let _identity = services.get::<IdentityService>()?;
            let _objects = services.get::<ObjectStoreService>()?;
            let _notifications = services.get::<NotificationService>()?;
            let _audit = services.get::<AuditService>()?;
            let _events = services.get::<EventServices>()?;
        }
        // Sessions, CSRF and idempotency are optional portal services
        // (ADR-0051): the base plugin works without all of them. Events
        // are a required capability (ADR-0056) and become a used
        // dependency through activity-intent dispatch.
        let portal = {
            let services = context.services();
            crate::TicketingPortalServices {
                sessions: services
                    .get_optional::<minco_plugin_sessions::SessionService>()
                    .map_err(|error| PluginError::Installation(error.to_string()))?,
                csrf: services
                    .get_optional::<minco_plugin_sessions::CsrfService>()
                    .map_err(|error| PluginError::Installation(error.to_string()))?,
                idempotency: services
                    .get_optional::<minco_plugin_idempotency::IdempotencyService>()
                    .map_err(|error| PluginError::Installation(error.to_string()))?,
                events: Some(
                    services
                        .get::<EventServices>()
                        .map_err(|error| PluginError::Installation(error.to_string()))?,
                ),
                #[cfg(feature = "jobs")]
                jobs: services
                    .get_optional::<minco_plugin_jobs::JobsServices>()
                    .map_err(|error| PluginError::Installation(error.to_string()))?,
                objects: Some(
                    services
                        .get::<ObjectStoreService>()
                        .map_err(|error| PluginError::Installation(error.to_string()))?,
                ),
            }
        };
        let service = TicketingService::new(self.store.clone(), config)
            .map_err(|error| PluginError::Installation(error.to_string()))?
            .with_portal_services(portal);
        context.services().insert(Arc::new(self.store.clone()))?;
        context.services().insert(Arc::new(service.clone()))?;
        context
            .contributions()
            .push_shared::<dyn HealthCheck>(Arc::new(TicketingHealthCheck(service.clone())));
        let mut header_policy = HttpHeaderPolicy::empty();
        header_policy
            .allow_request_header_name(crate::HANDOFF_HEADER)
            .and_then(|()| header_policy.mark_request_header_name_sensitive(crate::HANDOFF_HEADER))
            .and_then(|()| header_policy.allow_request_header_name("cookie"))
            .and_then(|()| header_policy.mark_request_header_name_sensitive("cookie"))
            .and_then(|()| header_policy.enable_cookie_csrf())
            .map_err(|error| PluginError::Installation(error.to_string()))?;
        HttpModule::new(context.plugin_id().clone(), ticketing_router(service))
            .with_operations(
                ticketing_operations()
                    .into_iter()
                    .map(|operation| operation.operation_id),
            )
            .with_max_request_body_bytes(256 * 1024)
            .with_header_policy(header_policy)
            .contribute(context);
        Ok(())
    }
}

#[derive(Clone)]
struct TicketingHealthCheck(TicketingService);

impl std::fmt::Debug for TicketingHealthCheck {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("TicketingHealthCheck").finish()
    }
}

#[async_trait]
impl HealthCheck for TicketingHealthCheck {
    fn id(&self) -> &'static str {
        "ticketing-store"
    }

    async fn check(&self) -> HealthResult {
        match self.0.ready().await {
            Ok(()) => HealthResult {
                id: self.id().into(),
                ready: true,
                critical: true,
                detail: None,
            },
            Err(error) => HealthResult {
                id: self.id().into(),
                ready: false,
                critical: true,
                detail: Some(error.to_string()),
            },
        }
    }
}

fn provision(name: &str) -> CapabilityProvision {
    CapabilityProvision {
        name: name.into(),
        version: Version::new(1, 0, 0),
    }
}

fn requirement(name: &str) -> CapabilityRequirement {
    CapabilityRequirement {
        name: name.into(),
        version: VersionReq::parse("^1").expect("static requirement"),
    }
}

fn ticketing_operations() -> Vec<OperationDescriptor> {
    [
        (
            "getTicketingSupportEntry",
            "GET",
            "/_minco/ticketing/support-entry.js",
            true,
        ),
        (
            "getTicketingBootstrap",
            "GET",
            "/_minco/ticketing/bootstrap",
            true,
        ),
        (
            "issueTicketingHandoff",
            "POST",
            "/_minco/ticketing/integrations/handoffs",
            false,
        ),
        (
            "consumeTicketingHandoff",
            "POST",
            "/_minco/ticketing/handoffs/exchange",
            true,
        ),
        (
            "createTicketFromHandoff",
            "POST",
            "/_minco/ticketing/tickets/from-handoff",
            true,
        ),
        ("createTicket", "POST", "/_minco/ticketing/tickets", false),
        ("listTickets", "GET", "/_minco/ticketing/tickets", false),
        (
            "getTicket",
            "GET",
            "/_minco/ticketing/tickets/{ticketId}",
            false,
        ),
        (
            "replyToTicketAsRequester",
            "POST",
            "/_minco/ticketing/tickets/{ticketId}/requester-replies",
            false,
        ),
        (
            "replyToTicketAsAgent",
            "POST",
            "/_minco/ticketing/tickets/{ticketId}/agent-replies",
            false,
        ),
        (
            "addTicketInternalNote",
            "POST",
            "/_minco/ticketing/tickets/{ticketId}/internal-notes",
            false,
        ),
        (
            "changeTicketAssignment",
            "PATCH",
            "/_minco/ticketing/tickets/{ticketId}/assignment",
            false,
        ),
        (
            "transferTicketQueue",
            "PATCH",
            "/_minco/ticketing/tickets/{ticketId}/queue",
            false,
        ),
        (
            "changeTicketPriority",
            "PATCH",
            "/_minco/ticketing/tickets/{ticketId}/priority",
            false,
        ),
        (
            "changeTicketStatus",
            "PATCH",
            "/_minco/ticketing/tickets/{ticketId}/status",
            false,
        ),
        (
            "ingestTicketExternalMessage",
            "POST",
            "/_minco/ticketing/ingress/messages",
            false,
        ),
        (
            "getTicketAiContext",
            "GET",
            "/_minco/ticketing/tickets/{ticketId}/ai-context",
            false,
        ),
        (
            "getTicketingAgentConsole",
            "GET",
            "/_minco/ticketing/agent",
            true,
        ),
        (
            "getTicketingAgentConsoleScript",
            "GET",
            "/_minco/ticketing/agent/console.js",
            true,
        ),
        (
            "getTicketingAgentConsoleStyles",
            "GET",
            "/_minco/ticketing/agent/console.css",
            true,
        ),
        (
            "getTicketingAgentBootstrap",
            "GET",
            "/_minco/ticketing/agent/bootstrap",
            false,
        ),
        (
            "listTicketingAgentTickets",
            "GET",
            "/_minco/ticketing/agent/tickets",
            false,
        ),
        (
            "getTicketingAgentTicket",
            "GET",
            "/_minco/ticketing/agent/tickets/{ticketId}",
            false,
        ),
        (
            "manageTicketingAgentTicket",
            "PATCH",
            "/_minco/ticketing/agent/tickets/{ticketId}/management",
            false,
        ),
        (
            "listTicketingRequesterTickets",
            "GET",
            "/_minco/ticketing/requester/tickets",
            false,
        ),
        (
            "getTicketingRequesterTicket",
            "GET",
            "/_minco/ticketing/requester/tickets/{ticketId}",
            false,
        ),
        (
            "replyToTicketingRequesterTicket",
            "POST",
            "/_minco/ticketing/requester/tickets/{ticketId}/replies",
            false,
        ),
        (
            "listTicketingRequesterMessages",
            "GET",
            "/_minco/ticketing/requester/tickets/{ticketId}/messages",
            false,
        ),
        (
            "createTicketingRequesterSession",
            "POST",
            "/_minco/ticketing/requester/sessions",
            true,
        ),
        (
            "endTicketingRequesterSession",
            "POST",
            "/_minco/ticketing/requester/logout",
            false,
        ),
    ]
    .into_iter()
    .map(|(operation_id, method, path, public)| OperationDescriptor {
        operation_id: operation_id.into(),
        method: method.into(),
        path: path.into(),
        public,
        idempotent: false,
    })
    .collect()
}

fn configuration_fields() -> Vec<ConfigurationField> {
    vec![
        field(
            "project_id",
            ConfigurationValueKind::String,
            true,
            None,
            "Stable application project identifier",
        ),
        field(
            "portal_origin",
            ConfigurationValueKind::String,
            true,
            None,
            "Exact HTTPS portal origin",
        ),
        field(
            "allowed_return_paths",
            ConfigurationValueKind::Object,
            true,
            None,
            "Exact application origins mapped to allowed path prefixes",
        ),
        field(
            "handoff_ttl_seconds",
            ConfigurationValueKind::Integer,
            false,
            Some(serde_json::json!(120)),
            "One-time handoff lifetime, at most 900 seconds",
        ),
        field(
            "support_label",
            ConfigurationValueKind::String,
            false,
            Some(serde_json::json!("Get support")),
            "Accessible launcher label",
        ),
        field(
            "support_brand",
            ConfigurationValueKind::String,
            false,
            Some(serde_json::json!("Support")),
            "Browser-safe support brand",
        ),
        field(
            "privacy_notice",
            ConfigurationValueKind::String,
            false,
            Some(serde_json::json!(
                "Share only information needed to resolve this request."
            )),
            "Browser-safe privacy notice",
        ),
        field(
            "requester_session_ttl_seconds",
            ConfigurationValueKind::Integer,
            false,
            Some(serde_json::json!(3600)),
            "Requester portal session lifetime, at most 86400 seconds",
        ),
        field(
            "notify_requester_on_public_reply",
            ConfigurationValueKind::Boolean,
            false,
            Some(serde_json::json!(false)),
            "Requires the jobs feature and an enqueue adapter: enqueue a notification job with each public agent reply",
        ),
    ]
}

fn field(
    key: &str,
    kind: ConfigurationValueKind,
    required: bool,
    default: Option<serde_json::Value>,
    description: &str,
) -> ConfigurationField {
    ConfigurationField {
        key: key.into(),
        kind,
        required,
        secret: false,
        description: description.into(),
        default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_plugin_audit::AuditPlugin;
    use minco_plugin_events::EventsPlugin;
    use minco_plugin_health::HealthPlugin;
    use minco_plugin_identity::IdentityPlugin;
    use minco_plugin_notifications::NotificationsPlugin;
    use minco_plugin_object_storage::ObjectStoragePlugin;
    use minco_test::PluginConformance;

    #[test]
    fn descriptor_is_disabled_beta_with_exact_dependencies_and_capabilities() {
        let descriptor = TicketingPlugin::memory().descriptor();
        assert_eq!(descriptor.stability, PluginStability::Beta);
        assert!(!descriptor.default_enabled);
        assert_eq!(descriptor.plugin_dependencies.len(), 6);
        assert!(
            descriptor
                .provides
                .iter()
                .any(|value| value.name == "ticketing.support-entry")
        );
    }

    #[test]
    fn generated_request_boundary_is_current() {
        let contract_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("openapi/openapi.yaml");
        let report = minco_contract::load_contract(&contract_path).unwrap();
        assert!(report.is_valid(), "{:?}", report.findings);
        let generated_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/generated.rs");
        let expected = minco_contract::generate_rust(&report.document);
        if std::env::var_os("UPDATE_MINCO_GENERATED").is_some_and(|value| value == "1") {
            std::fs::write(&generated_path, &expected).unwrap();
        }
        let committed = std::fs::read_to_string(&generated_path).unwrap();
        assert_eq!(
            committed, expected,
            "src/generated.rs is stale; run UPDATE_MINCO_GENERATED=1 cargo test -p minco-plugin-ticketing generated_request_boundary_is_current"
        );
    }

    #[test]
    fn openapi_and_descriptor_operation_inventories_match() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("openapi/openapi.yaml");
        let report = minco_contract::load_contract(path).unwrap();
        assert!(report.is_valid(), "{:?}", report.findings);
        let mut contract = report
            .document
            .operations
            .into_iter()
            .map(|operation| {
                (
                    operation.operation_id,
                    operation.method.as_str().to_owned(),
                    operation.path,
                    !operation.authenticated,
                )
            })
            .collect::<Vec<_>>();
        let mut descriptor = ticketing_operations()
            .into_iter()
            .map(|operation| {
                (
                    operation.operation_id,
                    operation.method,
                    operation.path,
                    operation.public,
                )
            })
            .collect::<Vec<_>>();
        contract.sort();
        descriptor.sort();
        assert_eq!(contract, descriptor);
    }

    #[test]
    fn passes_public_plugin_conformance() {
        PluginConformance::for_package(env!("CARGO_MANIFEST_DIR"))
            .with_plugin(TicketingPlugin::memory())
            .with_supporting_plugin(HealthPlugin)
            .with_supporting_plugin(IdentityPlugin::default())
            .with_supporting_plugin(ObjectStoragePlugin::memory())
            .with_supporting_plugin(NotificationsPlugin::memory().0)
            .with_supporting_plugin(AuditPlugin::memory().0)
            .with_supporting_plugin(EventsPlugin::memory().0)
            .with_configuration(serde_json::json!({
                "project_id": "example",
                "portal_origin": "https://support.example.test",
                "allowed_return_paths": {"https://app.example.test": ["/orders"]}
            }))
            .run()
            .assert_passed();
    }
}
