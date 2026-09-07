//! Static plugin registration for the workspace isolation boundary
//! (ADR-0076).
//!
//! The plugin registers the [`WorkspaceService`] and contributes a critical
//! store health check. It deliberately contributes no HTTP module: scope
//! types are transport-neutral and the composition root owns credential
//! verification. Provisioning is an explicit phase the composition runs
//! after migration; `install` never provisions.

use crate::{
    MemoryWorkspaceStore, WorkspaceConfig, WorkspaceService, WorkspaceStore, WorkspaceStoreService,
};
use async_trait::async_trait;
#[cfg(feature = "sqlite")]
use minco_core::MigrationSet;
use minco_core::{
    CapabilityProvision, CapabilityRequirement, ConfigurationField, ConfigurationValueKind,
    DataClass, HealthCheckDescriptor, Plugin, PluginContext, PluginDescriptor, PluginError,
    PluginId, PluginStability,
};
use minco_plugin_health::{HealthCheck, HealthResult};
use semver::{Version, VersionReq};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StorageProfile {
    Memory,
    Custom,
    #[cfg(feature = "sqlite")]
    Sqlite,
}

/// The workspace isolation plugin.
#[derive(Clone)]
pub struct WorkspacePlugin {
    store: WorkspaceStoreService,
    storage_profile: StorageProfile,
}

impl std::fmt::Debug for WorkspacePlugin {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorkspacePlugin")
            .field("storage_profile", &self.storage_profile)
            .finish_non_exhaustive()
    }
}

impl WorkspacePlugin {
    /// Compose over an explicitly selected store.
    #[must_use]
    pub fn new(store: Arc<dyn WorkspaceStore>) -> Self {
        Self {
            store: WorkspaceStoreService::new(store),
            storage_profile: StorageProfile::Custom,
        }
    }

    /// Deterministic in-memory profile for tests and non-durable
    /// compositions.
    #[must_use]
    pub fn memory() -> Self {
        Self {
            store: WorkspaceStoreService::new(Arc::new(MemoryWorkspaceStore::new())),
            storage_profile: StorageProfile::Memory,
        }
    }

    /// Durable `SQLite` profile; the store's `migrate()` owns the
    /// `_minco_workspace_migrations` ledger.
    #[cfg(feature = "sqlite")]
    #[must_use]
    pub fn sqlite(pool: sqlx::SqlitePool) -> Self {
        Self {
            store: WorkspaceStoreService::new(Arc::new(crate::SqliteWorkspaceStore::new(pool))),
            storage_profile: StorageProfile::Sqlite,
        }
    }
}

// No `Default` (following ADR-0053): the store is selected explicitly so a
// non-durable registry can never be selected silently.

impl Plugin for WorkspacePlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("workspace").expect("static plugin ID"),
            Version::new(0, 1, 0),
            "Workspace, project and integration-profile isolation with deployment-bound provisioning",
        );
        descriptor.core_compatibility =
            VersionReq::parse(concat!("^", env!("CARGO_PKG_VERSION"))).expect("package version");
        descriptor.stability = PluginStability::Experimental;
        descriptor.default_enabled = false;
        descriptor.documentation = Some("https://docs.rs/minco-plugin-workspace".into());
        descriptor.data_classes.push(DataClass::Internal);
        descriptor
            .plugin_dependencies
            .push(PluginId::new("health").expect("static plugin ID"));
        descriptor.requires.push(requirement("health.registry"));
        descriptor.provides.extend(
            [
                "workspace.provisioning",
                "workspace.scope-resolution",
                "workspace.profiles",
                "workspace.grants",
            ]
            .into_iter()
            .map(provision),
        );
        // No HTTP module and no operations: the scope model is
        // transport-neutral by decision (ADR-0076) and the composition root
        // owns concrete credential verification.
        #[cfg(feature = "sqlite")]
        if self.storage_profile == StorageProfile::Sqlite {
            descriptor.migrations.push(MigrationSet {
                id: "workspace-sqlite-v1".into(),
                database: "sqlite".into(),
                path: "migrations/sqlite".into(),
            });
        }
        descriptor.health_checks.push(HealthCheckDescriptor {
            id: "workspace-store".into(),
            critical: true,
        });
        descriptor.configuration.extend(configuration_fields());
        descriptor
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        let config = context.configuration::<WorkspaceConfig>()?;
        let service = WorkspaceService::new(self.store.clone(), config)
            .map_err(|error| PluginError::Installation(error.to_string()))?;
        context.services().insert(Arc::new(service.clone()))?;
        context
            .contributions()
            .push_shared::<dyn HealthCheck>(Arc::new(WorkspaceHealthCheck(service)));
        Ok(())
    }
}

#[derive(Clone)]
struct WorkspaceHealthCheck(WorkspaceService);

impl std::fmt::Debug for WorkspaceHealthCheck {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("WorkspaceHealthCheck").finish()
    }
}

#[async_trait]
impl HealthCheck for WorkspaceHealthCheck {
    fn id(&self) -> &'static str {
        "workspace-store"
    }

    async fn check(&self) -> HealthResult {
        match self.0.store_ready().await {
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
                detail: Some(format!("workspace store failed: {error}")),
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

fn configuration_fields() -> Vec<ConfigurationField> {
    vec![
        field(
            "display_name",
            ConfigurationValueKind::String,
            false,
            Some(serde_json::json!("Default workspace")),
            "Display name for the provisioned workspace; a label, never an authority identifier",
        ),
        field(
            "workspace_id",
            ConfigurationValueKind::String,
            false,
            None,
            "Optional pinned workspace identity; a bound database refuses a different identity",
        ),
        field(
            "projects",
            ConfigurationValueKind::StringList,
            true,
            None,
            "Projects registered under the workspace, with historical identifiers preserved verbatim",
        ),
        field(
            "profiles",
            ConfigurationValueKind::Object,
            false,
            None,
            "Integration profiles seeded by configuration, keyed by profile id: kind, auth_mode, bound_project, service_subject, permission_ceiling, allowed_origins, resource_types, secret_reference (a name, never a value)",
        ),
        field(
            "grants",
            ConfigurationValueKind::Object,
            false,
            None,
            "Explicit principal-to-project grants seeded by configuration, keyed by subject with project lists",
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
    use minco_plugin_health::HealthPlugin;
    use minco_test::PluginConformance;

    #[test]
    fn descriptor_has_the_reviewed_identity() {
        let descriptor = WorkspacePlugin::memory().descriptor();
        assert_eq!(descriptor.id.as_str(), "workspace");
        assert_eq!(descriptor.version, Version::new(0, 1, 0));
        assert_eq!(descriptor.stability, PluginStability::Experimental);
        assert!(!descriptor.default_enabled);
        assert!(descriptor.operations.is_empty());
        assert!(
            descriptor
                .provides
                .iter()
                .any(|provision| provision.name == "workspace.scope-resolution")
        );
        assert!(
            descriptor
                .plugin_dependencies
                .iter()
                .all(|id| id.as_str() == "health")
        );
    }

    #[test]
    fn passes_public_plugin_conformance() {
        PluginConformance::for_package(env!("CARGO_MANIFEST_DIR"))
            .with_plugin(WorkspacePlugin::memory())
            .with_supporting_plugin(HealthPlugin)
            .with_configuration(serde_json::json!({
                "projects": ["desk"],
                "profiles": {
                    "desk-agent": {
                        "kind": "service_api",
                        "auth_mode": "service_bearer_token",
                        "bound_project": "desk",
                        "service_subject": "desk-agent",
                        "secret_reference": "DESK_AGENT_TOKEN"
                    }
                }
            }))
            .run()
            .assert_passed();
    }

    #[tokio::test]
    async fn install_registers_the_scope_service_and_health_check() {
        let plugin = WorkspacePlugin::memory();
        let descriptor = plugin.descriptor();
        assert!(
            descriptor
                .health_checks
                .iter()
                .any(|check| check.id == "workspace-store")
        );
        let service = WorkspaceService::new(plugin.store.clone(), WorkspaceConfig::default())
            .expect("valid configuration");
        assert!(service.store_ready().await.is_ok());
    }
}
