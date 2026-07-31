use crate::{
    CapabilityProvision, CapabilityRequirement, ConfigurationField, DataClass,
    HealthCheckDescriptor, IdleCostClass, MigrationSet, PluginId, PluginStability, ResourceKind,
    WakeSource,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

/// Archive-visible metadata used to evaluate a plugin before linking its code.
///
/// The plugin crate's `[package.metadata.minco]` table points to this record by
/// package-root filename. Runtime composition still requires an ordinary Cargo
/// dependency and an explicit typed constructor registration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginDistributionManifest {
    pub schema: u32,
    pub id: PluginId,
    pub kind: PluginDistributionKind,
    pub plugin_version: Version,
    pub core_compatibility: VersionReq,
    pub stability: PluginStability,
    pub default_enabled: bool,
    pub feature: String,
    #[serde(default)]
    pub runtimes: Vec<String>,
    #[serde(default)]
    pub databases: Vec<String>,
    /// Other statically linked Minco plugins that must be registered.
    #[serde(default)]
    pub plugin_dependencies: Vec<PluginId>,
    #[serde(default)]
    pub requires: Vec<CapabilityRequirement>,
    #[serde(default)]
    pub provides: Vec<CapabilityProvision>,
    #[serde(default)]
    pub configuration: Vec<ConfigurationField>,
    #[serde(default)]
    pub operations: Vec<DistributionOperation>,
    #[serde(default)]
    pub migrations: Vec<MigrationSet>,
    #[serde(default)]
    pub seeds: Vec<DistributionSeed>,
    #[serde(default)]
    pub resources: Vec<DistributionResource>,
    #[serde(default)]
    pub health_checks: Vec<HealthCheckDescriptor>,
    #[serde(default)]
    pub data_classes: Vec<DataClass>,
    pub retention: RetentionPolicy,
    pub failure_policy: FailurePolicy,
    pub documentation: DocumentationLinks,
    pub conformance: ConformanceEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginDistributionKind {
    Plugin,
    Adapter,
    Runtime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistributionOperation {
    pub operation_id: String,
    pub method: String,
    pub path: String,
    pub public: bool,
    pub idempotent: bool,
    #[serde(default)]
    pub headers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistributionSeed {
    pub id: String,
    pub database: String,
    pub class: SeedClass,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedClass {
    Reference,
    Demo,
    Test,
    Bootstrap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistributionResource {
    pub id: String,
    /// Cargo feature that activates this resource contract, when conditional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    pub kind: ResourceKind,
    pub idle_cost: IdleCostClass,
    #[serde(default)]
    pub wake_sources: Vec<WakeSource>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub iam_actions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionPolicy {
    None,
    Ephemeral,
    ApplicationDefined,
    Persistent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailurePolicy {
    pub mode: FailureMode,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureMode {
    FailClosed,
    Degrade,
    BestEffort,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentationLinks {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tutorial: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub how_to: Option<String>,
    pub reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConformanceEvidence {
    pub profile: String,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_manifest() -> serde_json::Value {
        serde_json::json!({
            "schema": 1,
            "id": "example",
            "kind": "plugin",
            "plugin_version": "0.1.0",
            "core_compatibility": "*",
            "stability": "experimental",
            "default_enabled": false,
            "feature": "plugin-example",
            "runtimes": ["native"],
            "retention": "none",
            "failure_policy": {
                "mode": "fail_closed",
                "description": "Example failures are explicit."
            },
            "documentation": {"reference": "https://docs.rs/example"},
            "conformance": {"profile": "minco-plugin-v1", "evidence": ["cargo test"]}
        })
    }

    #[test]
    fn nested_shared_contracts_reject_unknown_distribution_fields() {
        let cases = [
            (
                "requires",
                serde_json::json!({"name": "example", "version": "*", "typo": true}),
            ),
            (
                "provides",
                serde_json::json!({"name": "example", "version": "1.0.0", "typo": true}),
            ),
            (
                "configuration",
                serde_json::json!({
                    "key": "mode",
                    "kind": "string",
                    "required": false,
                    "secret": false,
                    "description": "Example mode.",
                    "typo": true
                }),
            ),
            (
                "migrations",
                serde_json::json!({
                    "id": "example",
                    "database": "postgres",
                    "path": "migrations/postgres",
                    "typo": true
                }),
            ),
            (
                "health_checks",
                serde_json::json!({"id": "example", "critical": true, "typo": true}),
            ),
        ];

        for (field, value) in cases {
            let mut manifest = minimal_manifest();
            manifest[field] = serde_json::json!([value]);
            assert!(
                serde_json::from_value::<PluginDistributionManifest>(manifest).is_err(),
                "unknown nested field was accepted in {field}"
            );
        }
    }
}
