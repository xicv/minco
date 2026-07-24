//! Default health plugin and asynchronous health registry.
#![forbid(unsafe_code)]

use async_trait::async_trait;
use minco_core::{
    CapabilityProvision, HealthCheckDescriptor, Plugin, PluginContext, PluginDescriptor,
    PluginError, PluginFinalizeContext, PluginId, PluginStability,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResult {
    pub id: String,
    pub ready: bool,
    pub critical: bool,
    pub detail: Option<String>,
}

#[async_trait]
pub trait HealthCheck: Send + Sync + std::fmt::Debug {
    fn id(&self) -> &str;

    fn critical(&self) -> bool {
        true
    }

    async fn check(&self) -> HealthResult;
}

#[derive(Default)]
pub struct HealthRegistry {
    checks: RwLock<Vec<Arc<dyn HealthCheck>>>,
}

impl std::fmt::Debug for HealthRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HealthRegistry")
            .finish_non_exhaustive()
    }
}

impl HealthRegistry {
    pub fn register_now(&self, check: Arc<dyn HealthCheck>) {
        self.checks
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(check);
    }

    pub fn register(&self, check: Arc<dyn HealthCheck>) {
        self.register_now(check);
    }

    pub async fn run(&self) -> Vec<HealthResult> {
        let checks = self
            .checks
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let mut results = Vec::with_capacity(checks.len());
        for check in checks {
            results.push(check.check().await);
        }
        results
    }

    pub async fn ready(&self) -> bool {
        self.run()
            .await
            .into_iter()
            .all(|result| result.ready || !result.critical)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.checks
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone)]
pub struct StaticHealthCheck {
    id: String,
    ready: bool,
    critical: bool,
}

impl StaticHealthCheck {
    #[must_use]
    pub fn new(id: impl Into<String>, ready: bool, critical: bool) -> Self {
        Self {
            id: id.into(),
            ready,
            critical,
        }
    }
}

#[async_trait]
impl HealthCheck for StaticHealthCheck {
    fn id(&self) -> &str {
        &self.id
    }

    fn critical(&self) -> bool {
        self.critical
    }

    async fn check(&self) -> HealthResult {
        HealthResult {
            id: self.id.clone(),
            ready: self.ready,
            critical: self.critical,
            detail: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct HealthPlugin;

impl Plugin for HealthPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("health").expect("static plugin ID"),
            Version::new(1, 0, 0),
            "Liveness, readiness, and dependency health registry",
        );
        descriptor.default_enabled = true;
        descriptor.core_compatibility =
            VersionReq::parse(concat!("^", env!("CARGO_PKG_VERSION"))).expect("package version");
        descriptor.stability = PluginStability::Stable;
        descriptor.documentation = Some("https://docs.rs/minco-plugin-health".into());
        descriptor.provides.push(CapabilityProvision {
            name: "health.registry".into(),
            version: Version::new(1, 0, 0),
        });
        descriptor.health_checks.push(HealthCheckDescriptor {
            id: "minco-core".into(),
            critical: true,
        });
        descriptor
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        let registry = Arc::new(HealthRegistry::default());
        registry.register_now(Arc::new(StaticHealthCheck::new("minco-core", true, true)));
        context.services().insert(registry)?;
        Ok(())
    }

    fn finalize(&self, context: &mut PluginFinalizeContext<'_>) -> Result<(), PluginError> {
        let registry = context.services().get::<HealthRegistry>()?;
        for check in context.contributions().get_shared::<dyn HealthCheck>() {
            registry.register_now(check);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_core::{PluginManager, PluginSelection};

    #[tokio::test]
    async fn readiness_fails_only_for_critical_failures() {
        let registry = HealthRegistry::default();
        registry.register(Arc::new(StaticHealthCheck::new("optional", false, false)));
        assert!(registry.ready().await);
        registry.register(Arc::new(StaticHealthCheck::new("database", false, true)));
        assert!(!registry.ready().await);
    }

    #[test]
    fn health_plugin_aggregates_shared_health_contributions_during_finalize() {
        #[derive(Debug)]
        struct ContributingPlugin;

        impl Plugin for ContributingPlugin {
            fn descriptor(&self) -> PluginDescriptor {
                let mut descriptor = PluginDescriptor::new(
                    PluginId::new("contributor").unwrap(),
                    Version::new(1, 0, 0),
                    "health contributor",
                );
                descriptor
                    .plugin_dependencies
                    .push(PluginId::new("health").unwrap());
                descriptor
            }

            fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
                context
                    .contributions()
                    .push_shared::<dyn HealthCheck>(Arc::new(StaticHealthCheck::new(
                        "contributed",
                        true,
                        true,
                    )));
                Ok(())
            }
        }

        let mut manager = PluginManager::default();
        manager.register(HealthPlugin).unwrap();
        manager.register(ContributingPlugin).unwrap();
        let mut selection = PluginSelection::default();
        selection
            .enabled
            .insert(PluginId::new("contributor").unwrap());
        let application = manager.compose(&selection).unwrap();
        let registry = application.services.get::<HealthRegistry>().unwrap();
        assert_eq!(registry.len(), 2);
    }
}
