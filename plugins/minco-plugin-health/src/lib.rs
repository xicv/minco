//! Default health plugin and asynchronous health registry.
#![forbid(unsafe_code)]

use async_trait::async_trait;
use minco_core::{
    CapabilityProvision, HealthCheckDescriptor, Plugin, PluginContext, PluginDescriptor,
    PluginError, PluginId,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

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
    pub async fn register(&self, check: Arc<dyn HealthCheck>) {
        self.checks.write().await.push(check);
    }

    pub async fn run(&self) -> Vec<HealthResult> {
        let checks = self.checks.read().await.clone();
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
}

#[derive(Debug, Clone)]
pub struct StaticHealthCheck {
    id: String,
    ready: bool,
    critical: bool,
}

impl StaticHealthCheck {
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
            PluginId::new("health").expect("static id"),
            Version::new(1, 0, 0),
            "Liveness, readiness and dependency health registry",
        );
        descriptor.default_enabled = true;
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
        context
            .services()
            .insert(Arc::new(HealthRegistry::default()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn readiness_fails_only_for_critical_failures() {
        let registry = HealthRegistry::default();
        registry
            .register(Arc::new(StaticHealthCheck::new("optional", false, false)))
            .await;
        assert!(registry.ready().await);
        registry
            .register(Arc::new(StaticHealthCheck::new("database", false, true)))
            .await;
        assert!(!registry.ready().await);
    }
}
