use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PluginId(String);

impl PluginId {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 64
            || !value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            || value.ends_with('-')
            || value.contains("--")
        {
            return Err(IdentifierError::Invalid(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PluginId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for PluginId {
    type Err = IdentifierError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum IdentifierError {
    #[error("invalid lower-kebab identifier: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityProvision {
    pub name: String,
    pub version: Version,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRequirement {
    pub name: String,
    pub version: VersionReq,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationDescriptor {
    pub operation_id: String,
    pub method: String,
    pub path: String,
    pub public: bool,
    pub idempotent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationSet {
    pub id: String,
    pub database: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthCheckDescriptor {
    pub id: String,
    pub critical: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdleCostClass {
    ZeroCompute,
    StorageOnly,
    ProviderManaged,
    FixedCapacity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WakeSource {
    HttpRequest,
    QueueMessage,
    ObjectEvent,
    Schedule { expression: String },
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    ApiGatewayHttpApi,
    Lambda,
    ExternalPostgres,
    RdsPostgres,
    AuroraServerlessV2,
    SelfHostedPostgres,
    DynamoDb,
    Sqlite,
    S3Bucket,
    SqsQueue,
    SsmParameter,
    CloudWatchLogGroup,
    NatGateway,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceIntent {
    pub id: String,
    pub kind: ResourceKind,
    pub idle_cost: IdleCostClass,
    #[serde(default)]
    pub wake_sources: Vec<WakeSource>,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginDescriptor {
    pub id: PluginId,
    pub version: Version,
    pub description: String,
    pub default_enabled: bool,
    #[serde(default)]
    pub plugin_dependencies: Vec<PluginId>,
    #[serde(default)]
    pub requires: Vec<CapabilityRequirement>,
    #[serde(default)]
    pub provides: Vec<CapabilityProvision>,
    #[serde(default)]
    pub operations: Vec<OperationDescriptor>,
    #[serde(default)]
    pub migrations: Vec<MigrationSet>,
    #[serde(default)]
    pub health_checks: Vec<HealthCheckDescriptor>,
    #[serde(default)]
    pub resources: Vec<ResourceIntent>,
}

impl PluginDescriptor {
    pub fn new(id: PluginId, version: Version, description: impl Into<String>) -> Self {
        Self {
            id,
            version,
            description: description.into(),
            default_enabled: false,
            plugin_dependencies: Vec::new(),
            requires: Vec::new(),
            provides: Vec::new(),
            operations: Vec::new(),
            migrations: Vec::new(),
            health_checks: Vec::new(),
            resources: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_ids_are_strict_lower_kebab_case() {
        assert!(PluginId::new("health").is_ok());
        assert!(PluginId::new("sqlx-postgres").is_ok());
        for invalid in ["", "1plugin", "Plugin", "plugin_thing", "plugin--thing", "plugin-"] {
            assert!(PluginId::new(invalid).is_err(), "{invalid}");
        }
    }
}
