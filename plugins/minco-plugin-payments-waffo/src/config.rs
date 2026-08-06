// This private helper module exposes selected items to sibling modules through
// the crate root; `pub(super)` is intentional and narrower than public API.
#![allow(clippy::redundant_pub_crate)]

use crate::WaffoError;
use minco_config::{ConfigurationGraph, EnvironmentClass, SecretProvider, SecretReference};
use serde::Deserialize;
use std::{collections::BTreeSet, fmt, time::Duration};
use url::Url;

pub const CONFIGURATION_NAMESPACE: &str = "plugins.payments-waffo";
pub const DEFAULT_API_BASE_URL: &str = "https://api.waffo.ai";
pub const DEFAULT_REQUEST_MAX_BYTES: usize = 1024 * 1024;
pub(super) const DEFAULT_RESPONSE_MAX_BYTES: usize = 2 * 1024 * 1024;
pub(super) const DEFAULT_WEBHOOK_MAX_BYTES: usize = 1024 * 1024;

/// Local environment guard for a Waffo API key and webhook stream.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaffoEnvironment {
    #[default]
    Test,
    Production,
}

impl WaffoEnvironment {
    pub const fn webhook_test_mode(self) -> bool {
        matches!(self, Self::Test)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Test => "test",
            Self::Production => "production",
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
enum SecretReferenceInput {
    Text(String),
    Reference(SecretReference),
}

impl Default for SecretReferenceInput {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

impl fmt::Debug for SecretReferenceInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretReferenceInput([REDACTED])")
    }
}

impl SecretReferenceInput {
    fn into_reference(self) -> Result<SecretReference, WaffoError> {
        match self {
            Self::Text(value) => SecretReference::parse(&value)
                .map_err(|_| WaffoError::InvalidConfiguration("invalid private-key reference")),
            Self::Reference(reference) => Ok(reference),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawWaffoConfiguration {
    #[serde(default)]
    environment: WaffoEnvironment,
    merchant_id: String,
    private_key: SecretReferenceInput,
    #[serde(default)]
    webhook_public_key: Option<SecretReferenceInput>,
    #[serde(default)]
    store_id: Option<String>,
    #[serde(default = "default_api_base_url")]
    api_base_url: String,
    #[serde(default)]
    allow_custom_api_base_url: bool,
    #[serde(default = "default_request_timeout_seconds")]
    request_timeout_seconds: u64,
    #[serde(default = "default_request_max_bytes")]
    request_max_bytes: u64,
    #[serde(default = "default_response_max_bytes")]
    response_max_bytes: u64,
    #[serde(default = "default_webhook_tolerance_seconds")]
    webhook_tolerance_seconds: u64,
    #[serde(default = "default_webhook_max_bytes")]
    webhook_max_bytes: u64,
    #[serde(default)]
    allow_production_writes: bool,
    #[serde(default)]
    webhook_url: Option<String>,
    #[serde(default)]
    webhook_events: Vec<String>,
}

impl Default for RawWaffoConfiguration {
    fn default() -> Self {
        Self {
            environment: WaffoEnvironment::default(),
            merchant_id: String::new(),
            private_key: SecretReferenceInput::default(),
            webhook_public_key: None,
            store_id: None,
            api_base_url: default_api_base_url(),
            allow_custom_api_base_url: false,
            request_timeout_seconds: default_request_timeout_seconds(),
            request_max_bytes: default_request_max_bytes(),
            response_max_bytes: default_response_max_bytes(),
            webhook_tolerance_seconds: default_webhook_tolerance_seconds(),
            webhook_max_bytes: default_webhook_max_bytes(),
            allow_production_writes: false,
            webhook_url: None,
            webhook_events: Vec::new(),
        }
    }
}

/// Validated, unresolved Waffo configuration. Secret values are never stored here.
#[derive(Clone)]
pub struct WaffoConfiguration {
    environment: WaffoEnvironment,
    merchant_id: String,
    private_key: SecretReference,
    webhook_public_key: Option<SecretReference>,
    store_id: Option<String>,
    api_base_url: Url,
    allow_custom_api_base_url: bool,
    request_timeout: Duration,
    request_max_bytes: usize,
    response_max_bytes: usize,
    webhook_tolerance: Duration,
    webhook_max_bytes: usize,
    allow_production_writes: bool,
    webhook_url: Option<Url>,
    webhook_events: Vec<String>,
}

impl fmt::Debug for WaffoConfiguration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WaffoConfiguration")
            .field("environment", &self.environment)
            .field("merchant_id_configured", &!self.merchant_id.is_empty())
            .field("private_key", &self.private_key)
            .field(
                "webhook_public_key_configured",
                &self.webhook_public_key.is_some(),
            )
            .field("store_id_configured", &self.store_id.is_some())
            .field("api_base_url", &self.api_base_url)
            .field("allow_custom_api_base_url", &self.allow_custom_api_base_url)
            .field("request_timeout", &self.request_timeout)
            .field("request_max_bytes", &self.request_max_bytes)
            .field("response_max_bytes", &self.response_max_bytes)
            .field("webhook_tolerance", &self.webhook_tolerance)
            .field("webhook_max_bytes", &self.webhook_max_bytes)
            .field("allow_production_writes", &self.allow_production_writes)
            .field("webhook_url_configured", &self.webhook_url.is_some())
            .field("webhook_event_count", &self.webhook_events.len())
            .finish()
    }
}

impl TryFrom<RawWaffoConfiguration> for WaffoConfiguration {
    type Error = WaffoError;

    fn try_from(raw: RawWaffoConfiguration) -> Result<Self, Self::Error> {
        validate_short_id(&raw.merchant_id, "MER_").map_err(|()| {
            WaffoError::InvalidConfiguration("merchant_id must be a MER_ short ID")
        })?;
        if let Some(store_id) = &raw.store_id {
            validate_short_id(store_id, "STO_").map_err(|()| {
                WaffoError::InvalidConfiguration("store_id must be a STO_ short ID")
            })?;
        }

        let api_base_url = parse_https_url(&raw.api_base_url, true)?;
        let official_api_base_url =
            Url::parse(DEFAULT_API_BASE_URL).expect("the compiled Waffo API base URL is valid");
        let custom_api_base_url = api_base_url != official_api_base_url;
        if custom_api_base_url && !raw.allow_custom_api_base_url {
            return Err(WaffoError::InvalidConfiguration(
                "custom api_base_url requires allow_custom_api_base_url = true",
            ));
        }
        if custom_api_base_url && raw.environment == WaffoEnvironment::Production {
            return Err(WaffoError::InvalidConfiguration(
                "production Waffo credentials may only target the official API origin",
            ));
        }
        let webhook_url = raw
            .webhook_url
            .as_deref()
            .map(|value| parse_https_url(value, false))
            .transpose()?;
        let private_key = raw.private_key.into_reference()?;
        let webhook_public_key = raw
            .webhook_public_key
            .map(SecretReferenceInput::into_reference)
            .transpose()?;

        if !(1..=120).contains(&raw.request_timeout_seconds) {
            return Err(WaffoError::InvalidConfiguration(
                "request_timeout_seconds must be between 1 and 120",
            ));
        }
        if !(1_024..=16 * 1024 * 1024).contains(&raw.request_max_bytes) {
            return Err(WaffoError::InvalidConfiguration(
                "request_max_bytes must be between 1024 and 16777216",
            ));
        }
        if !(1_024..=16 * 1024 * 1024).contains(&raw.response_max_bytes) {
            return Err(WaffoError::InvalidConfiguration(
                "response_max_bytes must be between 1024 and 16777216",
            ));
        }
        if !(1..=300).contains(&raw.webhook_tolerance_seconds) {
            return Err(WaffoError::InvalidConfiguration(
                "webhook_tolerance_seconds must be between 1 and 300",
            ));
        }
        if !(1_024..=16 * 1024 * 1024).contains(&raw.webhook_max_bytes) {
            return Err(WaffoError::InvalidConfiguration(
                "webhook_max_bytes must be between 1024 and 16777216",
            ));
        }

        let mut unique_events = BTreeSet::new();
        for event in &raw.webhook_events {
            if event.is_empty()
                || event.len() > 128
                || !event
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
                || !unique_events.insert(event)
            {
                return Err(WaffoError::InvalidConfiguration(
                    "webhook_events must be unique bounded event identifiers",
                ));
            }
        }
        let webhook_configured = webhook_url.is_some()
            || !raw.webhook_events.is_empty()
            || raw.store_id.is_some()
            || webhook_public_key.is_some();
        if webhook_configured
            && (webhook_url.is_none()
                || raw.webhook_events.is_empty()
                || raw.store_id.is_none()
                || webhook_public_key.is_none())
        {
            return Err(WaffoError::MissingWebhookConfiguration);
        }

        Ok(Self {
            environment: raw.environment,
            merchant_id: raw.merchant_id,
            private_key,
            webhook_public_key,
            store_id: raw.store_id,
            api_base_url,
            allow_custom_api_base_url: raw.allow_custom_api_base_url,
            request_timeout: Duration::from_secs(raw.request_timeout_seconds),
            request_max_bytes: usize::try_from(raw.request_max_bytes)
                .map_err(|_| WaffoError::InvalidConfiguration("request_max_bytes is too large"))?,
            response_max_bytes: usize::try_from(raw.response_max_bytes)
                .map_err(|_| WaffoError::InvalidConfiguration("response_max_bytes is too large"))?,
            webhook_tolerance: Duration::from_secs(raw.webhook_tolerance_seconds),
            webhook_max_bytes: usize::try_from(raw.webhook_max_bytes)
                .map_err(|_| WaffoError::InvalidConfiguration("webhook_max_bytes is too large"))?,
            allow_production_writes: raw.allow_production_writes,
            webhook_url,
            webhook_events: raw.webhook_events,
        })
    }
}

impl WaffoConfiguration {
    /// Deserialize and validate the plugin namespace from Minco's typed configuration graph.
    pub fn from_graph(graph: &ConfigurationGraph) -> Result<Self, WaffoError> {
        let raw = graph
            .deserialize_namespace::<RawWaffoConfiguration>(CONFIGURATION_NAMESPACE)
            .map_err(|_| WaffoError::ConfigurationGraph)?;
        Self::try_from(raw)
    }

    /// Reject a key/environment mismatch before any network call is made.
    pub const fn validate_environment_class(
        &self,
        environment_class: EnvironmentClass,
    ) -> Result<(), WaffoError> {
        let valid = matches!(
            (environment_class, self.environment),
            (EnvironmentClass::Production, WaffoEnvironment::Production)
                | (
                    EnvironmentClass::Local
                        | EnvironmentClass::Test
                        | EnvironmentClass::Development
                        | EnvironmentClass::Staging,
                    WaffoEnvironment::Test
                )
        );
        if valid {
            Ok(())
        } else {
            Err(WaffoError::InvalidConfiguration(
                "Waffo environment must be test outside production and production in production",
            ))
        }
    }

    pub const fn environment(&self) -> WaffoEnvironment {
        self.environment
    }

    pub fn merchant_id(&self) -> &str {
        &self.merchant_id
    }

    pub const fn api_base_url(&self) -> &Url {
        &self.api_base_url
    }

    pub const fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    pub const fn request_max_bytes(&self) -> usize {
        self.request_max_bytes
    }

    pub const fn response_max_bytes(&self) -> usize {
        self.response_max_bytes
    }

    pub const fn webhook_tolerance(&self) -> Duration {
        self.webhook_tolerance
    }

    pub const fn webhook_max_bytes(&self) -> usize {
        self.webhook_max_bytes
    }

    pub const fn production_writes_allowed(&self) -> bool {
        self.allow_production_writes
    }

    pub fn store_id(&self) -> Option<&str> {
        self.store_id.as_deref()
    }

    pub const fn webhook_url(&self) -> Option<&Url> {
        self.webhook_url.as_ref()
    }

    pub fn webhook_events(&self) -> &[String] {
        &self.webhook_events
    }

    pub const fn private_key_provider(&self) -> SecretProvider {
        self.private_key.provider()
    }

    pub fn webhook_public_key_provider(&self) -> Option<SecretProvider> {
        self.webhook_public_key
            .as_ref()
            .map(SecretReference::provider)
    }

    pub(super) const fn private_key_reference(&self) -> &SecretReference {
        &self.private_key
    }

    pub(super) const fn webhook_public_key_reference(&self) -> Option<&SecretReference> {
        self.webhook_public_key.as_ref()
    }
}

fn default_api_base_url() -> String {
    DEFAULT_API_BASE_URL.into()
}

const fn default_request_timeout_seconds() -> u64 {
    30
}

const fn default_request_max_bytes() -> u64 {
    1_048_576
}

const fn default_response_max_bytes() -> u64 {
    2_097_152
}

const fn default_webhook_tolerance_seconds() -> u64 {
    300
}

const fn default_webhook_max_bytes() -> u64 {
    1_048_576
}

fn validate_short_id(value: &str, prefix: &str) -> Result<(), ()> {
    let Some(suffix) = value.strip_prefix(prefix) else {
        return Err(());
    };
    if suffix.is_empty()
        || value.len() > 128
        || !suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(());
    }
    Ok(())
}

fn parse_https_url(value: &str, require_origin_only: bool) -> Result<Url, WaffoError> {
    let url = Url::parse(value)
        .map_err(|_| WaffoError::InvalidConfiguration("configured URL is invalid"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || (require_origin_only && !matches!(url.path(), "" | "/"))
    {
        return Err(WaffoError::InvalidConfiguration(
            "configured URL must be an HTTPS URL without credentials, query, or fragment",
        ));
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_config::{ConfigLayer, ConfigSourceKind, Environment};
    use minco_core::{Plugin, PluginDescriptor};

    fn graph(document: &str, descriptor: PluginDescriptor) -> ConfigurationGraph {
        let schema = minco_config::ConfigurationSchema::try_from_fields([])
            .unwrap()
            .with_plugin_descriptors([descriptor])
            .unwrap();
        let layer =
            ConfigLayer::from_toml(ConfigSourceKind::EnvironmentFile, "test", document).unwrap();
        ConfigurationGraph::compile(
            &schema,
            Environment::new("test", EnvironmentClass::Test),
            [layer],
        )
        .unwrap()
    }

    #[test]
    fn typed_graph_preserves_secret_references_without_values() {
        let document = r#"
schema = 1
environment_class = "test"

[values.plugins.payments-waffo]
merchant_id = "MER_ABC123"
private_key = "env:WAFFO_PRIVATE_KEY"
"#;
        let graph = graph(document, crate::WaffoPlugin.descriptor());
        let configuration = WaffoConfiguration::from_graph(&graph).unwrap();

        assert_eq!(configuration.environment(), WaffoEnvironment::Test);
        assert_eq!(
            configuration.private_key_provider(),
            SecretProvider::EnvironmentVariable
        );
        assert!(!format!("{configuration:?}").contains("WAFFO_PRIVATE_KEY"));
    }

    #[test]
    fn production_key_is_rejected_outside_production_class() {
        let raw = RawWaffoConfiguration {
            environment: WaffoEnvironment::Production,
            merchant_id: "MER_ABC123".into(),
            private_key: SecretReferenceInput::Text("env:WAFFO_PRIVATE_KEY".into()),
            ..RawWaffoConfiguration::default()
        };
        let configuration = WaffoConfiguration::try_from(raw).unwrap();

        assert!(
            configuration
                .validate_environment_class(EnvironmentClass::Development)
                .is_err()
        );
    }

    #[test]
    fn partial_webhook_configuration_fails_closed() {
        let raw = RawWaffoConfiguration {
            merchant_id: "MER_ABC123".into(),
            private_key: SecretReferenceInput::Text("env:WAFFO_PRIVATE_KEY".into()),
            webhook_url: Some("https://example.com/webhook".into()),
            ..RawWaffoConfiguration::default()
        };

        assert!(matches!(
            WaffoConfiguration::try_from(raw),
            Err(WaffoError::MissingWebhookConfiguration)
        ));
    }
}
