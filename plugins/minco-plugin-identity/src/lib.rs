//! Verified-claims identity mapping and application permission checks.
#![forbid(unsafe_code)]

use async_trait::async_trait;
use minco_core::{
    CapabilityProvision, ConfigurationField, ConfigurationValueKind, DataClass, Plugin,
    PluginContext, PluginDescriptor, PluginError, PluginId, PluginStability,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Claims supplied only after signature, issuer, audience, expiry, and other
/// transport-level checks have succeeded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedClaims {
    pub subject: String,
    #[serde(default)]
    pub claims: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub subject: String,
    #[serde(default)]
    pub permissions: BTreeSet<String>,
    #[serde(default)]
    pub scopes: BTreeSet<String>,
    #[serde(default)]
    pub claims: BTreeMap<String, String>,
}

impl Identity {
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }

    pub fn require_permission(&self, permission: &str) -> Result<(), IdentityError> {
        if self.has_permission(permission) {
            Ok(())
        } else {
            Err(IdentityError::PermissionDenied(permission.to_owned()))
        }
    }
}

impl From<Identity> for minco_http::Principal {
    fn from(identity: Identity) -> Self {
        Self {
            subject: identity.subject,
            permissions: identity.permissions,
            claims: identity.claims,
        }
    }
}

#[async_trait]
pub trait IdentityProvider: Send + Sync + std::fmt::Debug {
    async fn resolve(&self, claims: VerifiedClaims) -> Result<Identity, IdentityError>;
}

#[derive(Clone)]
pub struct IdentityService(pub Arc<dyn IdentityProvider>);

impl std::fmt::Debug for IdentityService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("IdentityService").finish()
    }
}

impl IdentityService {
    pub fn new(provider: Arc<dyn IdentityProvider>) -> Self {
        Self(provider)
    }

    pub async fn resolve(&self, claims: VerifiedClaims) -> Result<Identity, IdentityError> {
        self.0.resolve(claims).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimsMapping {
    pub permission_claim: String,
    pub scope_claim: String,
    pub groups_claim: String,
    pub separator: char,
    #[serde(default)]
    pub group_permissions: BTreeMap<String, BTreeSet<String>>,
}

impl Default for ClaimsMapping {
    fn default() -> Self {
        Self {
            permission_claim: "permissions".into(),
            scope_claim: "scope".into(),
            groups_claim: "groups".into(),
            separator: ' ',
            group_permissions: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ClaimsIdentityProvider {
    mapping: ClaimsMapping,
}

impl ClaimsIdentityProvider {
    pub const fn new(mapping: ClaimsMapping) -> Self {
        Self { mapping }
    }
}

#[async_trait]
impl IdentityProvider for ClaimsIdentityProvider {
    async fn resolve(&self, claims: VerifiedClaims) -> Result<Identity, IdentityError> {
        if claims.subject.trim().is_empty() {
            return Err(IdentityError::InvalidSubject);
        }
        let split = |value: Option<&String>| {
            value
                .into_iter()
                .flat_map(|value| value.split(self.mapping.separator))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect::<BTreeSet<_>>()
        };
        let mut permissions = split(claims.claims.get(&self.mapping.permission_claim));
        let scopes = split(claims.claims.get(&self.mapping.scope_claim));
        if let Some(groups) = claims.claims.get(&self.mapping.groups_claim) {
            for group in groups
                .split(self.mapping.separator)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                if let Some(group_permissions) = self.mapping.group_permissions.get(group) {
                    permissions.extend(group_permissions.iter().cloned());
                }
            }
        }
        Ok(Identity {
            subject: claims.subject,
            permissions,
            scopes,
            claims: claims.claims,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ClaimsPluginConfiguration {
    #[serde(default)]
    permission_claim: Option<String>,
    #[serde(default)]
    scope_claim: Option<String>,
    #[serde(default)]
    groups_claim: Option<String>,
    #[serde(default)]
    separator: Option<String>,
    #[serde(default)]
    group_permissions: Option<BTreeMap<String, BTreeSet<String>>>,
}

#[derive(Debug, Clone)]
enum IdentityPluginSource {
    Claims(ClaimsMapping),
    Custom(IdentityService),
}

#[derive(Debug, Clone)]
pub struct IdentityPlugin {
    source: IdentityPluginSource,
}

impl IdentityPlugin {
    /// Uses an application-provided provider. Runtime claim-mapping fields are not exposed for
    /// custom providers because their configuration contract belongs to the provider itself.
    pub fn new(provider: Arc<dyn IdentityProvider>) -> Self {
        Self {
            source: IdentityPluginSource::Custom(IdentityService::new(provider)),
        }
    }

    pub const fn claims(mapping: ClaimsMapping) -> Self {
        Self {
            source: IdentityPluginSource::Claims(mapping),
        }
    }
}

impl Default for IdentityPlugin {
    fn default() -> Self {
        Self::claims(ClaimsMapping::default())
    }
}

impl Plugin for IdentityPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("identity").expect("static plugin ID"),
            Version::new(1, 0, 0),
            "Maps verified transport claims to provider-neutral identities and permissions",
        );
        descriptor.documentation = Some("https://docs.rs/minco-plugin-identity".into());
        descriptor.core_compatibility =
            VersionReq::parse(concat!("^", env!("CARGO_PKG_VERSION"))).expect("package version");
        descriptor.stability = PluginStability::Beta;
        descriptor
            .data_classes
            .extend([DataClass::Personal, DataClass::Confidential]);
        descriptor.provides.extend([
            CapabilityProvision {
                name: "identity.resolve".into(),
                version: Version::new(1, 0, 0),
            },
            CapabilityProvision {
                name: "authorization.permissions".into(),
                version: Version::new(1, 0, 0),
            },
        ]);

        if let IdentityPluginSource::Claims(mapping) = &self.source {
            descriptor
                .configuration
                .extend(claims_configuration(mapping));
        }
        descriptor
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        let service = match &self.source {
            IdentityPluginSource::Custom(service) => service.clone(),
            IdentityPluginSource::Claims(defaults) => {
                let configuration = context.configuration::<ClaimsPluginConfiguration>()?;
                let separator = configuration
                    .separator
                    .as_deref()
                    .map(parse_separator)
                    .transpose()
                    .map_err(PluginError::Installation)?
                    .unwrap_or(defaults.separator);
                let mapping = ClaimsMapping {
                    permission_claim: non_empty_or(
                        configuration.permission_claim,
                        &defaults.permission_claim,
                        "permission_claim",
                    )?,
                    scope_claim: non_empty_or(
                        configuration.scope_claim,
                        &defaults.scope_claim,
                        "scope_claim",
                    )?,
                    groups_claim: non_empty_or(
                        configuration.groups_claim,
                        &defaults.groups_claim,
                        "groups_claim",
                    )?,
                    separator,
                    group_permissions: configuration
                        .group_permissions
                        .unwrap_or_else(|| defaults.group_permissions.clone()),
                };
                IdentityService::new(Arc::new(ClaimsIdentityProvider::new(mapping)))
            }
        };
        context.services().insert(Arc::new(service))?;
        Ok(())
    }
}

fn claims_configuration(mapping: &ClaimsMapping) -> Vec<ConfigurationField> {
    vec![
        string_field(
            "permission_claim",
            "Verified claim containing permissions",
            &mapping.permission_claim,
        ),
        string_field(
            "scope_claim",
            "Verified claim containing OAuth/OIDC scopes",
            &mapping.scope_claim,
        ),
        string_field(
            "groups_claim",
            "Verified claim containing provider group names",
            &mapping.groups_claim,
        ),
        string_field(
            "separator",
            "Single character separating claim values",
            &mapping.separator.to_string(),
        ),
        ConfigurationField {
            key: "group_permissions".into(),
            kind: ConfigurationValueKind::Object,
            required: false,
            secret: false,
            description: "Map of provider groups to application permissions".into(),
            default: Some(
                serde_json::to_value(&mapping.group_permissions)
                    .expect("claims group mapping must serialize"),
            ),
        },
    ]
}

fn string_field(key: &str, description: &str, default: &str) -> ConfigurationField {
    ConfigurationField {
        key: key.into(),
        kind: ConfigurationValueKind::String,
        required: false,
        secret: false,
        description: description.into(),
        default: Some(serde_json::Value::String(default.into())),
    }
}

fn parse_separator(value: &str) -> Result<char, String> {
    let mut characters = value.chars();
    let separator = characters
        .next()
        .ok_or_else(|| "identity separator must contain one character".to_owned())?;
    if characters.next().is_some() || separator.is_control() {
        return Err("identity separator must contain one non-control character".into());
    }
    Ok(separator)
}

fn non_empty_or(value: Option<String>, default: &str, field: &str) -> Result<String, PluginError> {
    let value = value.unwrap_or_else(|| default.to_owned());
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(PluginError::Installation(format!(
            "identity {field} must not be empty or contain control characters"
        )));
    }
    Ok(value)
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum IdentityError {
    #[error("identity subject must not be empty")]
    InvalidSubject,
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("identity provider failed: {0}")]
    Provider(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_core::{PluginManager, PluginSelection};

    #[tokio::test]
    async fn verified_claims_map_to_permissions_and_scopes() {
        let provider = ClaimsIdentityProvider::default();
        let identity = provider
            .resolve(VerifiedClaims {
                subject: "client-1".into(),
                claims: BTreeMap::from([
                    ("permissions".into(), "feedback.create feedback.read".into()),
                    ("scope".into(), "openid profile".into()),
                ]),
            })
            .await
            .unwrap();
        assert!(identity.has_permission("feedback.create"));
        assert!(identity.scopes.contains("openid"));
        assert!(identity.require_permission("feedback.manage").is_err());
    }

    #[tokio::test]
    async fn runtime_mapping_configuration_is_applied_to_claims_provider() {
        let mut manager = PluginManager::default();
        manager.register(IdentityPlugin::default()).unwrap();
        let id = PluginId::new("identity").unwrap();
        let mut selection = PluginSelection::default();
        selection.enabled.insert(id.clone());
        selection.configuration.insert(
            id,
            serde_json::json!({
                "permission_claim": "roles",
                "scope_claim": "scp",
                "groups_claim": "teams",
                "separator": ",",
                "group_permissions": {
                    "operators": ["feedback.manage"]
                }
            }),
        );
        let application = manager.compose(&selection).unwrap();
        let service = application.services.get::<IdentityService>().unwrap();
        let identity = service
            .resolve(VerifiedClaims {
                subject: "developer-1".into(),
                claims: BTreeMap::from([
                    ("roles".into(), "feedback.read".into()),
                    ("scp".into(), "openid,profile".into()),
                    ("teams".into(), "operators".into()),
                ]),
            })
            .await
            .unwrap();
        assert!(identity.has_permission("feedback.manage"));
        assert!(identity.scopes.contains("profile"));
    }
}
