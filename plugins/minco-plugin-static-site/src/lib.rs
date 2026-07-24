//! Provider-neutral static-site deployment intent for Minco applications.
#![forbid(unsafe_code)]

use minco_core::{
    CapabilityProvision, ConfigurationField, ConfigurationValueKind, IdleCostClass, Plugin,
    PluginContext, PluginDescriptor, PluginError, PluginId, PluginStability, ResourceIntent,
    ResourceKind, WakeSource,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudFrontPriceClass {
    #[default]
    PriceClass100,
    PriceClass200,
    PriceClassAll,
}

impl CloudFrontPriceClass {
    #[must_use]
    pub const fn as_aws_value(self) -> &'static str {
        match self {
            Self::PriceClass100 => "PriceClass_100",
            Self::PriceClass200 => "PriceClass_200",
            Self::PriceClassAll => "PriceClass_All",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticSiteConfig {
    #[serde(default = "default_source_directory")]
    pub source_directory: String,
    #[serde(default = "default_index_document")]
    pub index_document: String,
    #[serde(default = "default_true")]
    pub spa_fallback: bool,
    #[serde(default = "default_immutable_cache_seconds")]
    pub immutable_cache_seconds: u32,
    #[serde(default)]
    pub html_cache_seconds: u32,
    #[serde(default)]
    pub price_class: CloudFrontPriceClass,
    #[serde(default = "default_true")]
    pub ipv6_enabled: bool,
    #[serde(default)]
    pub custom_domain: Option<String>,
    #[serde(default)]
    pub manage_dns_alias: bool,
}

impl Default for StaticSiteConfig {
    fn default() -> Self {
        Self {
            source_directory: default_source_directory(),
            index_document: default_index_document(),
            spa_fallback: true,
            immutable_cache_seconds: default_immutable_cache_seconds(),
            html_cache_seconds: 0,
            price_class: CloudFrontPriceClass::PriceClass100,
            ipv6_enabled: true,
            custom_domain: None,
            manage_dns_alias: false,
        }
    }
}

impl StaticSiteConfig {
    pub fn validate(&self) -> Result<(), StaticSiteError> {
        validate_relative_path("source_directory", &self.source_directory)?;
        validate_relative_path("index_document", &self.index_document)?;
        if self.immutable_cache_seconds > 31_536_000 || self.html_cache_seconds > 86_400 {
            return Err(StaticSiteError::InvalidConfiguration(
                "immutable cache must not exceed one year and HTML cache must not exceed one day"
                    .into(),
            ));
        }
        if self.manage_dns_alias && self.custom_domain.is_none() {
            return Err(StaticSiteError::InvalidConfiguration(
                "manage_dns_alias requires custom_domain".into(),
            ));
        }
        if let Some(domain) = self.custom_domain.as_deref() {
            validate_domain(domain)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticSitePlan {
    pub source_directory: String,
    pub index_document: String,
    pub spa_fallback: bool,
    pub immutable_cache_seconds: u32,
    pub html_cache_seconds: u32,
    pub price_class: String,
    pub ipv6_enabled: bool,
    pub custom_domain: Option<String>,
    pub manage_dns_alias: bool,
}

impl From<StaticSiteConfig> for StaticSitePlan {
    fn from(value: StaticSiteConfig) -> Self {
        Self {
            source_directory: value.source_directory,
            index_document: value.index_document,
            spa_fallback: value.spa_fallback,
            immutable_cache_seconds: value.immutable_cache_seconds,
            html_cache_seconds: value.html_cache_seconds,
            price_class: value.price_class.as_aws_value().into(),
            ipv6_enabled: value.ipv6_enabled,
            custom_domain: value.custom_domain,
            manage_dns_alias: value.manage_dns_alias,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StaticSitePlugin;

impl Plugin for StaticSitePlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("static-site").expect("static plugin ID"),
            Version::new(1, 0, 0),
            "Private static assets distributed through a CDN without assuming a frontend framework",
        );
        descriptor.documentation = Some("https://docs.rs/minco-plugin-static-site".into());
        descriptor.core_compatibility =
            VersionReq::parse(concat!("^", env!("CARGO_PKG_VERSION"))).expect("package version");
        descriptor.stability = PluginStability::Beta;
        descriptor.provides.push(CapabilityProvision {
            name: "static-site.publish".into(),
            version: Version::new(1, 0, 0),
        });
        descriptor.configuration = configuration_fields();
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
            serde_json::from_value::<StaticSiteConfig>(configuration).map_err(|source| {
                PluginError::InvalidConfiguration {
                    plugin: descriptor.id.clone(),
                    source,
                }
            })?;
        configuration
            .validate()
            .map_err(|error| PluginError::Installation(error.to_string()))?;
        descriptor
            .resources
            .extend(static_site_resources(&configuration));
        Ok(())
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        let configuration = context.configuration::<StaticSiteConfig>()?;
        configuration
            .validate()
            .map_err(|error| PluginError::Installation(error.to_string()))?;
        context
            .services()
            .insert(Arc::new(StaticSitePlan::from(configuration)))?;
        Ok(())
    }
}

fn static_site_resources(configuration: &StaticSiteConfig) -> Vec<ResourceIntent> {
    let mut resources = vec![
        ResourceIntent {
            id: "static-site-bucket".into(),
            kind: ResourceKind::S3Bucket,
            idle_cost: IdleCostClass::StorageOnly,
            wake_sources: Vec::new(),
            dependencies: Vec::new(),
        },
        ResourceIntent {
            id: "static-site-cdn".into(),
            kind: ResourceKind::CloudFrontDistribution,
            idle_cost: IdleCostClass::ProviderManaged,
            wake_sources: vec![WakeSource::HttpRequest],
            dependencies: vec!["static-site-bucket".into()],
        },
    ];
    if configuration.custom_domain.is_some() {
        resources.push(ResourceIntent {
            id: "static-site-certificate".into(),
            kind: ResourceKind::AcmCertificate,
            idle_cost: IdleCostClass::ProviderManaged,
            wake_sources: Vec::new(),
            dependencies: Vec::new(),
        });
        if let Some(cdn) = resources
            .iter_mut()
            .find(|resource| resource.id == "static-site-cdn")
        {
            cdn.dependencies.push("static-site-certificate".into());
        }
    }
    if configuration.manage_dns_alias {
        resources.push(ResourceIntent {
            id: "static-site-dns".into(),
            kind: ResourceKind::Route53Alias,
            idle_cost: IdleCostClass::ProviderManaged,
            wake_sources: Vec::new(),
            dependencies: vec!["static-site-cdn".into()],
        });
    }
    resources
}

fn configuration_fields() -> Vec<ConfigurationField> {
    vec![
        string_field(
            "source_directory",
            "dist",
            "Directory containing the built static artifact",
        ),
        string_field(
            "index_document",
            "index.html",
            "Default document served at the site root",
        ),
        boolean_field(
            "spa_fallback",
            true,
            "Rewrite missing browser routes to the index document",
        ),
        integer_field(
            "immutable_cache_seconds",
            31_536_000,
            "Cache lifetime for fingerprinted immutable assets",
        ),
        integer_field(
            "html_cache_seconds",
            0,
            "Cache lifetime for HTML entrypoints",
        ),
        string_field(
            "price_class",
            "price_class100",
            "CloudFront price class: price_class100, price_class200, or price_class_all",
        ),
        boolean_field("ipv6_enabled", true, "Enable IPv6 on the CDN distribution"),
        ConfigurationField {
            key: "custom_domain".into(),
            kind: ConfigurationValueKind::String,
            required: false,
            secret: false,
            description: "Optional application hostname".into(),
            default: None,
        },
        boolean_field(
            "manage_dns_alias",
            false,
            "Create a DNS alias for custom_domain in the selected deployment renderer",
        ),
    ]
}

fn string_field(key: &str, default: &str, description: &str) -> ConfigurationField {
    ConfigurationField {
        key: key.into(),
        kind: ConfigurationValueKind::String,
        required: false,
        secret: false,
        description: description.into(),
        default: Some(serde_json::Value::String(default.into())),
    }
}

fn boolean_field(key: &str, default: bool, description: &str) -> ConfigurationField {
    ConfigurationField {
        key: key.into(),
        kind: ConfigurationValueKind::Boolean,
        required: false,
        secret: false,
        description: description.into(),
        default: Some(serde_json::Value::Bool(default)),
    }
}

fn integer_field(key: &str, default: u32, description: &str) -> ConfigurationField {
    ConfigurationField {
        key: key.into(),
        kind: ConfigurationValueKind::Integer,
        required: false,
        secret: false,
        description: description.into(),
        default: Some(serde_json::json!(default)),
    }
}

fn validate_relative_path(field: &str, value: &str) -> Result<(), StaticSiteError> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || value.chars().any(char::is_control)
        || path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(StaticSiteError::InvalidConfiguration(format!(
            "{field} must be a non-empty relative path without parent traversal"
        )));
    }
    Ok(())
}

fn validate_domain(value: &str) -> Result<(), StaticSiteError> {
    if value.len() > 253
        || value.starts_with('.')
        || value.ends_with('.')
        || value.split('.').count() < 2
        || value.bytes().any(|byte| byte.is_ascii_uppercase())
        || value.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err(StaticSiteError::InvalidConfiguration(
            "custom_domain must be a valid lower-ASCII DNS name".into(),
        ));
    }
    Ok(())
}

fn default_source_directory() -> String {
    "dist".into()
}

fn default_index_document() -> String {
    "index.html".into()
}

const fn default_true() -> bool {
    true
}

const fn default_immutable_cache_seconds() -> u32 {
    31_536_000
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum StaticSiteError {
    #[error("invalid static-site configuration: {0}")]
    InvalidConfiguration(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_core::{PluginManager, PluginSelection};

    #[test]
    fn default_profile_declares_private_storage_and_cdn() {
        let mut manager = PluginManager::default();
        manager.register(StaticSitePlugin).unwrap();
        let mut selection = PluginSelection::default();
        selection
            .enabled
            .insert(PluginId::new("static-site").unwrap());
        let application = manager.compose(&selection).unwrap();
        assert!(
            application
                .graph
                .resources
                .contains_key("static-site-bucket")
        );
        assert!(application.graph.resources.contains_key("static-site-cdn"));
        assert!(!application.graph.resources.contains_key("static-site-dns"));
    }

    #[test]
    fn custom_domain_configuration_changes_the_validated_resource_graph() {
        let mut manager = PluginManager::default();
        manager.register(StaticSitePlugin).unwrap();
        let id = PluginId::new("static-site").unwrap();
        let mut selection = PluginSelection::default();
        selection.enabled.insert(id.clone());
        selection.configuration.insert(
            id,
            serde_json::json!({
                "custom_domain": "app.example.test",
                "manage_dns_alias": true
            }),
        );
        let application = manager.compose(&selection).unwrap();
        assert!(
            application
                .graph
                .resources
                .contains_key("static-site-certificate")
        );
        assert!(application.graph.resources.contains_key("static-site-dns"));
    }

    #[test]
    fn dns_management_requires_a_custom_domain() {
        let configuration = StaticSiteConfig {
            manage_dns_alias: true,
            ..StaticSiteConfig::default()
        };
        assert!(configuration.validate().is_err());
    }
}
