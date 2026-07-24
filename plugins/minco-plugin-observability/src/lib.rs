//! Default structured-observability plugin.
#![forbid(unsafe_code)]

use minco_core::{
    CapabilityProvision, ConfigurationField, ConfigurationValueKind, Plugin, PluginContext,
    PluginDescriptor, PluginError, PluginId, PluginStability,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ObservabilityConfig {
    pub service_name: String,
    pub json: bool,
    pub default_filter: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ObservabilityError {
    #[error("failed to initialize tracing subscriber: {0}")]
    Initialization(String),
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            service_name: "minco-app".into(),
            json: true,
            default_filter: "info,tower_http=info".into(),
        }
    }
}

impl ObservabilityConfig {
    pub fn init(&self) -> Result<(), ObservabilityError> {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(&self.default_filter));
        let result = if self.json {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .json()
                .try_init()
        } else {
            tracing_subscriber::fmt().with_env_filter(filter).try_init()
        };
        result.map_err(|error| ObservabilityError::Initialization(error.to_string()))
    }
}

#[derive(Debug, Clone)]
pub struct ObservabilityPlugin {
    config: ObservabilityConfig,
}

impl ObservabilityPlugin {
    pub const fn new(config: ObservabilityConfig) -> Self {
        Self { config }
    }
}

impl Default for ObservabilityPlugin {
    fn default() -> Self {
        Self::new(ObservabilityConfig::default())
    }
}

impl Plugin for ObservabilityPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("observability").expect("static id"),
            Version::new(1, 0, 0),
            "Structured tracing and CloudWatch-compatible JSON logging",
        );
        descriptor.core_compatibility =
            VersionReq::parse(concat!("^", env!("CARGO_PKG_VERSION"))).expect("package version");
        descriptor.stability = PluginStability::Stable;
        descriptor.documentation = Some("https://docs.rs/minco-plugin-observability".into());
        descriptor.default_enabled = true;
        descriptor.provides.push(CapabilityProvision {
            name: "observability.tracing".into(),
            version: Version::new(1, 0, 0),
        });
        descriptor.configuration.extend([
            ConfigurationField {
                key: "service_name".into(),
                kind: ConfigurationValueKind::String,
                required: false,
                secret: false,
                description: "Stable service name included in operational telemetry".into(),
                default: Some(serde_json::json!(self.config.service_name)),
            },
            ConfigurationField {
                key: "json".into(),
                kind: ConfigurationValueKind::Boolean,
                required: false,
                secret: false,
                description: "Emit structured JSON suitable for CloudWatch Logs".into(),
                default: Some(serde_json::json!(self.config.json)),
            },
            ConfigurationField {
                key: "default_filter".into(),
                kind: ConfigurationValueKind::String,
                required: false,
                secret: false,
                description: "Fallback tracing filter when RUST_LOG is unset".into(),
                default: Some(serde_json::json!(self.config.default_filter)),
            },
        ]);
        descriptor
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        let config = context.configuration::<ObservabilityConfig>()?;
        context.services().insert(Arc::new(config))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_core::{PluginManager, PluginSelection};

    #[test]
    fn runtime_selection_overrides_constructor_defaults() {
        let mut manager = PluginManager::default();
        manager
            .register(ObservabilityPlugin::new(ObservabilityConfig {
                service_name: "constructor".into(),
                json: true,
                default_filter: "info".into(),
            }))
            .unwrap();
        let id = PluginId::new("observability").unwrap();
        let mut selection = PluginSelection::default();
        selection
            .configuration
            .insert(id, serde_json::json!({ "service_name": "runtime" }));

        let application = manager.compose(&selection).unwrap();
        let config = application.services.get::<ObservabilityConfig>().unwrap();
        assert_eq!(config.service_name, "runtime");
        assert!(config.json);
        assert_eq!(config.default_filter, "info");
    }
}
