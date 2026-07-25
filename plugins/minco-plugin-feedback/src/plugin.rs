use crate::{
    FeedbackConfig, FeedbackService, FeedbackStore, FeedbackStoreService, MemoryFeedbackStore,
    TranscriptionService, feedback_request_body_budget, feedback_router,
};
use async_trait::async_trait;
#[cfg(any(feature = "postgres", feature = "sqlite"))]
use minco_core::MigrationSet;
use minco_core::{
    CapabilityProvision, CapabilityRequirement, ConfigurationField, ConfigurationValueKind,
    DataClass, HealthCheckDescriptor, IdleCostClass, OperationDescriptor, Plugin, PluginContext,
    PluginDescriptor, PluginError, PluginId, PluginStability, ResourceIntent, ResourceKind,
};
use minco_http::HttpModule;
use minco_plugin_audit::AuditService;
use minco_plugin_events::EventServices;
use minco_plugin_health::{HealthCheck, HealthResult};
use minco_plugin_notifications::NotificationService;
use minco_plugin_object_storage::ObjectStoreService;
use semver::{Version, VersionReq};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FeedbackStorageProfile {
    Memory,
    Custom,
    #[cfg(feature = "postgres")]
    Postgres,
    #[cfg(feature = "sqlite")]
    Sqlite,
}

#[derive(Clone)]
pub struct FeedbackPlugin {
    store: FeedbackStoreService,
    storage_profile: FeedbackStorageProfile,
    transcription: Option<TranscriptionService>,
}

impl std::fmt::Debug for FeedbackPlugin {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FeedbackPlugin")
            .field("storage_profile", &self.storage_profile)
            .field("transcription_configured", &self.transcription.is_some())
            .finish_non_exhaustive()
    }
}

impl FeedbackPlugin {
    #[must_use]
    pub fn new(store: Arc<dyn FeedbackStore>) -> Self {
        Self {
            store: FeedbackStoreService::new(store),
            storage_profile: FeedbackStorageProfile::Custom,
            transcription: None,
        }
    }

    #[must_use]
    pub fn memory() -> Self {
        Self {
            store: FeedbackStoreService::new(Arc::new(MemoryFeedbackStore::default())),
            storage_profile: FeedbackStorageProfile::Memory,
            transcription: None,
        }
    }

    #[must_use]
    pub fn with_transcription(mut self, transcription: TranscriptionService) -> Self {
        self.transcription = Some(transcription);
        self
    }

    #[cfg(feature = "postgres")]
    #[must_use]
    pub fn postgres(pool: sqlx::PgPool) -> Self {
        Self {
            store: FeedbackStoreService::new(Arc::new(crate::PostgresFeedbackStore::new(pool))),
            storage_profile: FeedbackStorageProfile::Postgres,
            transcription: None,
        }
    }

    #[cfg(feature = "sqlite")]
    #[must_use]
    pub fn sqlite(pool: sqlx::SqlitePool) -> Self {
        Self {
            store: FeedbackStoreService::new(Arc::new(crate::SqliteFeedbackStore::new(pool))),
            storage_profile: FeedbackStorageProfile::Sqlite,
            transcription: None,
        }
    }
}

impl Default for FeedbackPlugin {
    fn default() -> Self {
        Self::memory()
    }
}

impl Plugin for FeedbackPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("feedback").expect("static plugin ID"),
            Version::new(0, 1, 0),
            "Fast client feedback loops with screenshots, voice, discussion, and AI-ready context",
        );
        descriptor.core_compatibility =
            VersionReq::parse(concat!("^", env!("CARGO_PKG_VERSION"))).expect("package version");
        descriptor.stability = PluginStability::Stable;
        descriptor.default_enabled = false;
        descriptor.documentation = Some("https://docs.rs/minco-plugin-feedback".into());
        descriptor.data_classes.extend([
            DataClass::CustomerProvided,
            DataClass::Personal,
            DataClass::Confidential,
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
        descriptor.provides.extend([
            provision("feedback.submit"),
            provision("feedback.conversation"),
            provision("feedback.manage"),
            provision("feedback.ai-context"),
            provision("feedback.widget"),
        ]);
        if self.transcription.is_some() {
            descriptor
                .provides
                .push(provision("feedback.transcription"));
        }
        descriptor.operations.extend(feedback_operations());
        match self.storage_profile {
            FeedbackStorageProfile::Memory => {}
            FeedbackStorageProfile::Custom => descriptor.resources.push(ResourceIntent {
                id: "feedback-custom-store".into(),
                kind: ResourceKind::Custom("feedback-store".into()),
                idle_cost: IdleCostClass::ProviderManaged,
                wake_sources: Vec::new(),
                dependencies: Vec::new(),
            }),
            #[cfg(feature = "postgres")]
            FeedbackStorageProfile::Postgres => descriptor.migrations.push(MigrationSet {
                id: "feedback-postgres-v1".into(),
                database: "postgres".into(),
                path: "migrations/postgres".into(),
            }),
            #[cfg(feature = "sqlite")]
            FeedbackStorageProfile::Sqlite => descriptor.migrations.push(MigrationSet {
                id: "feedback-sqlite-v1".into(),
                database: "sqlite".into(),
                path: "migrations/sqlite".into(),
            }),
        }
        descriptor.health_checks.push(HealthCheckDescriptor {
            id: "feedback-store".into(),
            critical: true,
        });
        descriptor.configuration.extend(configuration_fields());
        descriptor
    }

    fn configure_descriptor(
        &self,
        descriptor: &mut PluginDescriptor,
        configuration: Option<&serde_json::Value>,
    ) -> Result<(), PluginError> {
        let configuration = configuration
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let configuration =
            serde_json::from_value::<FeedbackConfig>(configuration).map_err(|source| {
                PluginError::InvalidConfiguration {
                    plugin: descriptor.id.clone(),
                    source,
                }
            })?;
        if !configuration.transcription_enabled {
            descriptor
                .provides
                .retain(|capability| capability.name != "feedback.transcription");
        }
        Ok(())
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        let config = context.configuration::<FeedbackConfig>()?;
        let (objects, notifications, audit, events) = {
            let services = context.services();
            (
                (*services.get::<ObjectStoreService>()?).clone(),
                (*services.get::<NotificationService>()?).clone(),
                (*services.get::<AuditService>()?).clone(),
                (*services.get::<EventServices>()?).clone(),
            )
        };
        let service = FeedbackService::new(
            self.store.clone(),
            objects,
            notifications,
            audit,
            events,
            self.transcription.clone(),
            config,
        )
        .map_err(|error| PluginError::Installation(error.to_string()))?;

        context.services().insert(Arc::new(self.store.clone()))?;
        context.services().insert(Arc::new(service.clone()))?;
        context
            .contributions()
            .push_shared::<dyn HealthCheck>(Arc::new(FeedbackHealthCheck(service.clone())));
        let request_body_budget = feedback_request_body_budget(service.config());
        HttpModule::new(context.plugin_id().clone(), feedback_router(service))
            .with_operations(
                feedback_operations()
                    .into_iter()
                    .map(|operation| operation.operation_id),
            )
            .with_max_request_body_bytes(request_body_budget)
            .contribute(context);
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct FeedbackHealthCheck(FeedbackService);

#[async_trait]
impl HealthCheck for FeedbackHealthCheck {
    fn id(&self) -> &'static str {
        "feedback-store"
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

fn feedback_operations() -> Vec<OperationDescriptor> {
    [
        ("feedbackWidget", "GET", "/_minco/feedback/widget.js", true),
        (
            "getFeedbackWidgetConfig",
            "GET",
            "/_minco/feedback/widget-config",
            true,
        ),
        ("createFeedback", "POST", "/_minco/feedback/threads", true),
        (
            "getClientFeedback",
            "GET",
            "/_minco/feedback/threads/{id}",
            true,
        ),
        (
            "replyToFeedback",
            "POST",
            "/_minco/feedback/threads/{id}/messages",
            true,
        ),
        (
            "getClientFeedbackAttachment",
            "GET",
            "/_minco/feedback/threads/{id}/attachments/{attachmentId}",
            true,
        ),
        (
            "transcribeFeedbackAudio",
            "POST",
            "/_minco/feedback/transcriptions",
            true,
        ),
        (
            "listDeveloperFeedback",
            "GET",
            "/_minco/feedback/developer/threads",
            false,
        ),
        (
            "getDeveloperFeedback",
            "GET",
            "/_minco/feedback/developer/threads/{id}",
            false,
        ),
        (
            "developerReplyToFeedback",
            "POST",
            "/_minco/feedback/developer/threads/{id}/messages",
            false,
        ),
        (
            "transitionFeedback",
            "PATCH",
            "/_minco/feedback/developer/threads/{id}/status",
            false,
        ),
        (
            "getFeedbackAiContext",
            "GET",
            "/_minco/feedback/developer/threads/{id}/ai-context",
            false,
        ),
        (
            "getDeveloperFeedbackAttachment",
            "GET",
            "/_minco/feedback/developer/threads/{id}/attachments/{attachmentId}",
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
            false,
            None,
            "Stable product or application identifier",
        ),
        field(
            "widget_label",
            ConfigurationValueKind::String,
            false,
            false,
            Some(serde_json::json!("Share feedback")),
            "Accessible label shown on the feedback action",
        ),
        field(
            "widget_position",
            ConfigurationValueKind::String,
            false,
            false,
            Some(serde_json::json!("bottom_right")),
            "FAB position: top_left, top_right, bottom_left, or bottom_right",
        ),
        field(
            "offset_x_px",
            ConfigurationValueKind::Integer,
            false,
            false,
            Some(serde_json::json!(24)),
            "Horizontal viewport offset in CSS pixels",
        ),
        field(
            "offset_y_px",
            ConfigurationValueKind::Integer,
            false,
            false,
            Some(serde_json::json!(24)),
            "Vertical viewport offset in CSS pixels",
        ),
        field(
            "theme",
            ConfigurationValueKind::String,
            false,
            false,
            Some(serde_json::json!("auto")),
            "Widget theme: light, dark, or auto",
        ),
        field(
            "token_storage",
            ConfigurationValueKind::String,
            false,
            false,
            Some(serde_json::json!("session")),
            "Opaque client-token storage: session (default) or local",
        ),
        field(
            "max_http_body_bytes",
            ConfigurationValueKind::Integer,
            false,
            false,
            Some(serde_json::json!(7 * 1024 * 1024)),
            "Maximum complete multipart request size for the default serverless HTTP path",
        ),
        field(
            "max_screenshot_bytes",
            ConfigurationValueKind::Integer,
            false,
            false,
            Some(serde_json::json!(4 * 1024 * 1024)),
            "Maximum screenshot upload size",
        ),
        field(
            "max_audio_bytes",
            ConfigurationValueKind::Integer,
            false,
            false,
            Some(serde_json::json!(5 * 1024 * 1024)),
            "Maximum voice recording upload size",
        ),
        field(
            "max_file_bytes",
            ConfigurationValueKind::Integer,
            false,
            false,
            Some(serde_json::json!(5 * 1024 * 1024)),
            "Maximum general attachment upload size",
        ),
        field(
            "max_attachments",
            ConfigurationValueKind::Integer,
            false,
            false,
            Some(serde_json::json!(3)),
            "Maximum number of screenshot, audio, and file attachments per submission",
        ),
        field(
            "allow_anonymous",
            ConfigurationValueKind::Boolean,
            false,
            false,
            Some(serde_json::json!(false)),
            "Explicitly allow unauthenticated feedback when neither identity nor a project key is available",
        ),
        field(
            "project_key",
            ConfigurationValueKind::String,
            false,
            false,
            None,
            "Optional browser-visible submission key used for basic abuse controls",
        ),
        field(
            "developer_token",
            ConfigurationValueKind::String,
            false,
            true,
            None,
            "Fallback bearer token for local/operator access; prefer an identity principal with feedback.manage",
        ),
        field(
            "developer_recipient",
            ConfigurationValueKind::String,
            false,
            false,
            Some(serde_json::json!("developers")),
            "Recipient understood by the configured notification sink",
        ),
        field(
            "developer_link_base",
            ConfigurationValueKind::String,
            false,
            false,
            None,
            "Optional base URL included in developer notifications",
        ),
        field(
            "notify_client_updates",
            ConfigurationValueKind::Boolean,
            false,
            false,
            Some(serde_json::json!(true)),
            "Send in-app notifications for developer replies and status changes",
        ),
        field(
            "publish_events_inline",
            ConfigurationValueKind::Boolean,
            false,
            false,
            Some(serde_json::json!(false)),
            "Publish outbox events on the request path instead of leaving them for a worker",
        ),
        field(
            "screenshot_enabled",
            ConfigurationValueKind::Boolean,
            false,
            false,
            Some(serde_json::json!(true)),
            "Allow browser screen capture and image attachments",
        ),
        field(
            "voice_enabled",
            ConfigurationValueKind::Boolean,
            false,
            false,
            Some(serde_json::json!(false)),
            "Allow microphone recording when the browser supports MediaRecorder",
        ),
        field(
            "max_recording_seconds",
            ConfigurationValueKind::Integer,
            false,
            false,
            Some(serde_json::json!(90)),
            "Maximum browser voice-note recording duration",
        ),
        field(
            "include_url_query",
            ConfigurationValueKind::Boolean,
            false,
            false,
            Some(serde_json::json!(false)),
            "Include URL query parameters in captured context after redaction",
        ),
        field(
            "redact_query_parameters",
            ConfigurationValueKind::StringList,
            false,
            false,
            Some(serde_json::json!([
                "access_token",
                "api_key",
                "code",
                "key",
                "password",
                "secret",
                "signature",
                "token"
            ])),
            "Case-insensitive query parameter names replaced with [REDACTED]",
        ),
        field(
            "transcription_enabled",
            ConfigurationValueKind::Boolean,
            false,
            false,
            Some(serde_json::json!(false)),
            "Expose voice transcription for authenticated feedback.create principals when a TranscriptionService is configured",
        ),
        field(
            "auto_transcribe_audio",
            ConfigurationValueKind::Boolean,
            false,
            false,
            Some(serde_json::json!(false)),
            "Transcribe uploaded voice recordings automatically",
        ),
        field(
            "poll_interval_ms",
            ConfigurationValueKind::Integer,
            false,
            false,
            Some(serde_json::json!(15_000)),
            "Client discussion refresh interval in milliseconds",
        ),
        field(
            "privacy_notice",
            ConfigurationValueKind::String,
            false,
            false,
            None,
            "Optional client-visible privacy and retention notice",
        ),
    ]
}

fn field(
    key: &str,
    kind: ConfigurationValueKind,
    required: bool,
    secret: bool,
    default: Option<serde_json::Value>,
    description: &str,
) -> ConfigurationField {
    ConfigurationField {
        key: key.into(),
        kind,
        required,
        secret,
        description: description.into(),
        default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DisabledTranscriber, TranscriptionService};
    use minco_core::{PluginManager, PluginSelection};
    use minco_plugin_audit::AuditPlugin;
    use minco_plugin_events::EventsPlugin;
    use minco_plugin_health::HealthPlugin;
    use minco_plugin_identity::IdentityPlugin;
    use minco_plugin_notifications::NotificationsPlugin;
    use minco_plugin_object_storage::ObjectStoragePlugin;

    #[test]
    fn feedback_declares_every_foundational_dependency() {
        let descriptor = FeedbackPlugin::default().descriptor();
        assert_eq!(descriptor.stability, PluginStability::Stable);
        let dependencies = descriptor
            .plugin_dependencies
            .iter()
            .map(PluginId::as_str)
            .collect::<Vec<_>>();
        assert_eq!(
            dependencies,
            [
                "health",
                "identity",
                "object-storage",
                "notifications",
                "audit",
                "events"
            ]
        );
        assert!(
            descriptor
                .operations
                .iter()
                .any(|operation| operation.operation_id == "createFeedback")
        );
        assert!(
            descriptor
                .data_classes
                .contains(&DataClass::CustomerProvided)
        );
    }

    #[test]
    fn feedback_plugin_composes_with_explicit_foundational_dependencies() {
        let mut manager = PluginManager::default();
        manager.register(HealthPlugin).unwrap();
        manager.register(IdentityPlugin::default()).unwrap();
        manager.register(ObjectStoragePlugin::memory()).unwrap();
        manager.register(NotificationsPlugin::memory().0).unwrap();
        manager.register(AuditPlugin::memory().0).unwrap();
        manager.register(EventsPlugin::memory().0).unwrap();
        manager.register(FeedbackPlugin::memory()).unwrap();

        let mut selection = PluginSelection::default();
        let feedback_id = PluginId::new("feedback").unwrap();
        selection.enabled.insert(feedback_id.clone());
        selection
            .set_configuration(
                feedback_id,
                &FeedbackConfig {
                    project_id: "example".into(),
                    developer_token: Some("developer-token-with-enough-entropy".into()),
                    ..FeedbackConfig::default()
                },
            )
            .unwrap();
        let application = manager.compose(&selection).unwrap();
        assert!(application.services.get::<FeedbackService>().is_ok());
        assert_eq!(application.contributions.get::<HttpModule>().len(), 1);
        assert_eq!(
            application
                .contributions
                .get_shared::<dyn HealthCheck>()
                .len(),
            1
        );
    }
    #[test]
    fn memory_feedback_does_not_claim_database_migrations_or_transcription() {
        let descriptor = FeedbackPlugin::memory().descriptor();
        assert!(descriptor.migrations.is_empty());
        assert!(descriptor.resources.is_empty());
        assert!(
            descriptor
                .provides
                .iter()
                .all(|capability| capability.name != "feedback.transcription")
        );
    }

    #[test]
    fn transcription_capability_requires_both_provider_and_enabled_configuration() {
        fn manager_with_feedback(plugin: FeedbackPlugin) -> PluginManager {
            let mut manager = PluginManager::default();
            manager.register(HealthPlugin).unwrap();
            manager.register(IdentityPlugin::default()).unwrap();
            manager.register(ObjectStoragePlugin::memory()).unwrap();
            manager.register(NotificationsPlugin::memory().0).unwrap();
            manager.register(AuditPlugin::memory().0).unwrap();
            manager.register(EventsPlugin::memory().0).unwrap();
            manager.register(plugin).unwrap();
            manager
        }

        let plugin = FeedbackPlugin::memory()
            .with_transcription(TranscriptionService::new(Arc::new(DisabledTranscriber)));
        let manager = manager_with_feedback(plugin);
        let feedback_id = PluginId::new("feedback").unwrap();

        let mut disabled = PluginSelection::default();
        disabled.enabled.insert(feedback_id.clone());
        disabled
            .set_configuration(
                feedback_id.clone(),
                &FeedbackConfig {
                    project_id: "example".into(),
                    transcription_enabled: false,
                    ..FeedbackConfig::default()
                },
            )
            .unwrap();
        let disabled_graph = manager.compose(&disabled).unwrap().graph;
        assert!(
            !disabled_graph
                .capabilities
                .contains_key("feedback.transcription")
        );

        let mut enabled = PluginSelection::default();
        enabled.enabled.insert(feedback_id.clone());
        enabled
            .set_configuration(
                feedback_id,
                &FeedbackConfig {
                    project_id: "example".into(),
                    transcription_enabled: true,
                    ..FeedbackConfig::default()
                },
            )
            .unwrap();
        let enabled_graph = manager.compose(&enabled).unwrap().graph;
        assert!(
            enabled_graph
                .capabilities
                .contains_key("feedback.transcription")
        );
    }

    #[test]
    fn openapi_contract_matches_the_plugin_operation_inventory() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("openapi/feedback.openapi.yaml");
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
        let mut descriptor = FeedbackPlugin::memory()
            .descriptor()
            .operations
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
}
