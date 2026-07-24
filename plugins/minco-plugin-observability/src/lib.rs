//! Default structured-observability plugin.
#![forbid(unsafe_code)]

use minco_core::{CapabilityProvision, Plugin, PluginContext, PluginDescriptor, PluginError, PluginId};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    fn default() -> Self { Self { service_name: "minco-app".into(), json: true, default_filter: "info,tower_http=info".into() } }
}

impl ObservabilityConfig {
    pub fn init(&self) -> Result<(), ObservabilityError> {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&self.default_filter));
        let result = if self.json {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .json()
                .try_init()
        } else {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .try_init()
        };
        result.map_err(|error| ObservabilityError::Initialization(error.to_string()))
    }
}

#[derive(Debug, Clone)]
pub struct ObservabilityPlugin { config: ObservabilityConfig }
impl ObservabilityPlugin { pub fn new(config: ObservabilityConfig) -> Self { Self { config } } }
impl Default for ObservabilityPlugin { fn default() -> Self { Self::new(ObservabilityConfig::default()) } }

impl Plugin for ObservabilityPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(PluginId::new("observability").expect("static id"), Version::new(1, 0, 0), "Structured tracing and CloudWatch-compatible JSON logging");
        descriptor.default_enabled = true;
        descriptor.provides.push(CapabilityProvision { name: "observability.tracing".into(), version: Version::new(1, 0, 0) });
        descriptor
    }
    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        context.services().insert(Arc::new(self.config.clone()))?;
        Ok(())
    }
}
