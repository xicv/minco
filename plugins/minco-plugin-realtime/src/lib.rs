//! Application-owned Minco plugin `realtime`.
#![forbid(unsafe_code)]

use async_trait::async_trait;
use minco_core::{
    CapabilityProvision, ConfigurationField, ConfigurationValueKind, IdleCostClass, Plugin,
    PluginContext, PluginDescriptor, PluginError, PluginId, PluginStability, ResourceIntent,
    ResourceKind, WakeSource,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub const DEFAULT_MAX_EVENT_BYTES: usize = 5 * 1024;
/// Dependency-free browser subscriber distributed with the crate archive.
pub const REALTIME_CLIENT_MODULE: &str = include_str!("../assets/realtime-client.mjs");

/// A provider-neutral realtime channel without a leading slash.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RealtimeChannel(String);

impl RealtimeChannel {
    pub fn parse(value: impl Into<String>) -> Result<Self, RealtimeError> {
        let value = value.into();
        let segments = value.split('/').collect::<Vec<_>>();
        let valid = (1..=4).contains(&segments.len())
            && segments.iter().all(|segment| {
                (1..=50).contains(&segment.len())
                    && segment
                        .bytes()
                        .next()
                        .is_some_and(|byte| byte.is_ascii_alphanumeric())
                    && segment
                        .bytes()
                        .next_back()
                        .is_some_and(|byte| byte.is_ascii_alphanumeric())
                    && segment
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            });
        if !valid {
            return Err(RealtimeError::InvalidChannel);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RealtimeError {
    #[error(
        "realtime channel must contain one to four slash-separated AppSync-compatible segments of 1 to 50 bytes"
    )]
    InvalidChannel,
    #[error("invalid realtime configuration: {0}")]
    InvalidConfiguration(String),
    #[error("realtime envelope is invalid: {0}")]
    InvalidEnvelope(String),
    #[error("realtime publication was rejected: {0}")]
    Rejected(String),
    #[error("realtime publication is temporarily unavailable: {0}")]
    Unavailable(String),
    #[error("realtime publication failed: {0}")]
    Publish(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RealtimeEnvelope {
    pub id: String,
    pub event_type: String,
    pub occurred_at: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealtimePublication {
    pub channel: RealtimeChannel,
    pub envelope: RealtimeEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RealtimePlan {
    #[serde(default = "default_namespace")]
    pub namespace: String,
    #[serde(default = "default_max_event_bytes")]
    pub max_event_bytes: usize,
    #[serde(default = "default_subscriber_claim")]
    pub subscriber_claim: String,
}

impl Default for RealtimePlan {
    fn default() -> Self {
        Self {
            namespace: default_namespace(),
            max_event_bytes: default_max_event_bytes(),
            subscriber_claim: default_subscriber_claim(),
        }
    }
}

impl RealtimePlan {
    pub fn validate(&self) -> Result<(), RealtimeError> {
        let namespace = RealtimeChannel::parse(self.namespace.clone())?;
        if namespace.as_str().contains('/') {
            return Err(RealtimeError::InvalidConfiguration(
                "namespace must be one portable channel segment".into(),
            ));
        }
        if !namespace
            .as_str()
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
            || !namespace
                .as_str()
                .bytes()
                .next_back()
                .is_some_and(|byte| byte.is_ascii_alphanumeric())
        {
            return Err(RealtimeError::InvalidConfiguration(
                "namespace must start and end with an ASCII alphanumeric character".into(),
            ));
        }
        if !(256..=240 * 1024).contains(&self.max_event_bytes) {
            return Err(RealtimeError::InvalidConfiguration(
                "max_event_bytes must be between 256 and the AppSync Events 240 KiB limit".into(),
            ));
        }
        if !(1..=128).contains(&self.subscriber_claim.len())
            || !self.subscriber_claim.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
            })
        {
            return Err(RealtimeError::InvalidConfiguration(
                "subscriber_claim must be a bounded OIDC claim name".into(),
            ));
        }
        Ok(())
    }
}

fn default_namespace() -> String {
    "minco".into()
}

const fn default_max_event_bytes() -> usize {
    DEFAULT_MAX_EVENT_BYTES
}

fn default_subscriber_claim() -> String {
    "sub".into()
}

#[async_trait]
pub trait RealtimePublisher: Send + Sync + std::fmt::Debug {
    async fn publish(&self, publication: &RealtimePublication) -> Result<(), RealtimeError>;
}

#[derive(Clone)]
pub struct RealtimePublisherService {
    publisher: Arc<dyn RealtimePublisher>,
    plan: RealtimePlan,
}

impl std::fmt::Debug for RealtimePublisherService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RealtimePublisherService")
            .field("plan", &self.plan)
            .finish_non_exhaustive()
    }
}

impl RealtimePublisherService {
    #[must_use]
    pub fn new(publisher: Arc<dyn RealtimePublisher>, plan: RealtimePlan) -> Self {
        Self { publisher, plan }
    }

    pub async fn publish(&self, publication: &RealtimePublication) -> Result<(), RealtimeError> {
        self.plan.validate()?;
        validate_publication(publication, self.plan.max_event_bytes)?;
        self.publisher.publish(publication).await
    }
}

fn validate_publication(
    publication: &RealtimePublication,
    max_event_bytes: usize,
) -> Result<(), RealtimeError> {
    if publication.envelope.id.trim().is_empty()
        || publication.envelope.event_type.trim().is_empty()
        || publication.envelope.occurred_at.trim().is_empty()
    {
        return Err(RealtimeError::InvalidEnvelope(
            "id, event_type, and occurred_at are required".into(),
        ));
    }
    let encoded = serde_json::to_vec(&publication.envelope)
        .map_err(|error| RealtimeError::InvalidEnvelope(error.to_string()))?;
    if encoded.len() > max_event_bytes {
        return Err(RealtimeError::InvalidEnvelope(format!(
            "encoded envelope is {} bytes; limit is {max_event_bytes}",
            encoded.len()
        )));
    }
    Ok(())
}

#[derive(Debug, Default)]
pub struct MemoryRealtimePublisher {
    publications: Mutex<Vec<RealtimePublication>>,
}

impl MemoryRealtimePublisher {
    #[must_use]
    pub fn published(&self) -> Vec<RealtimePublication> {
        self.publications
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

#[async_trait]
impl RealtimePublisher for MemoryRealtimePublisher {
    async fn publish(&self, publication: &RealtimePublication) -> Result<(), RealtimeError> {
        self.publications
            .lock()
            .map_err(|_| RealtimeError::Publish("realtime memory lock was poisoned".into()))?
            .push(publication.clone());
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct RealtimePlugin {
    publisher: Option<Arc<dyn RealtimePublisher>>,
}

impl RealtimePlugin {
    #[must_use]
    pub fn with_publisher(mut self, publisher: Arc<dyn RealtimePublisher>) -> Self {
        self.publisher = Some(publisher);
        self
    }
}

impl Plugin for RealtimePlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("realtime").expect("static plugin ID"),
            Version::new(1, 0, 0),
            "Subscriber-only realtime invalidation with backend-owned publication",
        );
        descriptor.documentation = Some("https://docs.rs/minco-plugin-realtime".into());
        descriptor.core_compatibility =
            VersionReq::parse(concat!("^", env!("CARGO_PKG_VERSION"))).expect("package version");
        descriptor.stability = PluginStability::Beta;
        descriptor.provides.push(CapabilityProvision {
            name: "realtime.publish".into(),
            version: Version::new(1, 0, 0),
        });
        descriptor.configuration = realtime_configuration_fields();
        descriptor
    }

    fn configure_descriptor(
        &self,
        descriptor: &mut PluginDescriptor,
        configuration: Option<&serde_json::Value>,
    ) -> Result<(), PluginError> {
        let plan = serde_json::from_value::<RealtimePlan>(
            configuration
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
        )
        .map_err(|source| PluginError::InvalidConfiguration {
            plugin: descriptor.id.clone(),
            source,
        })?;
        plan.validate()
            .map_err(|error| PluginError::Installation(error.to_string()))?;
        descriptor.resources.push(realtime_resource());
        Ok(())
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        let plan = context.configuration::<RealtimePlan>()?;
        plan.validate()
            .map_err(|error| PluginError::Installation(error.to_string()))?;
        context.services().insert(Arc::new(plan.clone()))?;
        if let Some(publisher) = &self.publisher {
            context
                .services()
                .insert(Arc::new(RealtimePublisherService::new(
                    publisher.clone(),
                    plan,
                )))?;
        }
        Ok(())
    }
}

fn realtime_resource() -> ResourceIntent {
    ResourceIntent {
        id: "realtime-api".into(),
        kind: ResourceKind::Custom("realtime-event-api".into()),
        idle_cost: IdleCostClass::ProviderManaged,
        wake_sources: vec![WakeSource::HttpRequest],
        dependencies: Vec::new(),
    }
}

fn realtime_configuration_fields() -> Vec<ConfigurationField> {
    vec![
        ConfigurationField {
            key: "namespace".into(),
            kind: ConfigurationValueKind::String,
            required: false,
            secret: false,
            description: "Portable namespace prepended to subscriber channels".into(),
            default: Some(serde_json::json!(default_namespace())),
        },
        ConfigurationField {
            key: "max_event_bytes".into(),
            kind: ConfigurationValueKind::Integer,
            required: false,
            secret: false,
            description: "Maximum encoded envelope size; 5120 bytes keeps one billing unit".into(),
            default: Some(serde_json::json!(DEFAULT_MAX_EVENT_BYTES)),
        },
        ConfigurationField {
            key: "subscriber_claim".into(),
            kind: ConfigurationValueKind::String,
            required: false,
            secret: false,
            description: "OIDC claim that must equal the first channel segment after the namespace"
                .into(),
            default: Some(serde_json::json!(default_subscriber_claim())),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_core::{PluginManager, PluginSelection};
    use minco_test::PluginConformance;
    use std::sync::Arc;

    #[test]
    fn descriptor_has_the_reviewed_identity() {
        assert_eq!(
            RealtimePlugin::default().descriptor().id.as_str(),
            "realtime"
        );
    }

    #[test]
    fn package_exposes_the_receive_only_browser_module() {
        assert!(REALTIME_CLIENT_MODULE.contains("export function createRealtimeClient"));
        assert!(!REALTIME_CLIENT_MODULE.contains("publish("));
    }

    #[test]
    fn passes_the_public_plugin_conformance_kit() {
        PluginConformance::for_package(env!("CARGO_MANIFEST_DIR"))
            .with_plugin(RealtimePlugin::default())
            .run()
            .assert_passed();
    }

    #[test]
    fn channel_accepts_portable_segments_and_rejects_provider_specific_paths() {
        let channel = RealtimeChannel::parse("orders/tenant-42/order-7").expect("valid channel");
        assert_eq!(channel.as_str(), "orders/tenant-42/order-7");

        for invalid in [
            "",
            "/orders/tenant-42",
            "orders/tenant_42",
            "orders/-tenant-42",
            "orders/tenant-42-",
            "orders//tenant-42",
            "orders/tenant-42/one/two/three",
        ] {
            assert!(
                RealtimeChannel::parse(invalid).is_err(),
                "accepted {invalid}"
            );
        }
    }

    #[tokio::test]
    async fn memory_publisher_records_a_valid_envelope() {
        let publisher = Arc::new(MemoryRealtimePublisher::default());
        let service = RealtimePublisherService::new(publisher.clone(), RealtimePlan::default());
        let publication = RealtimePublication {
            channel: RealtimeChannel::parse("orders/tenant-42").unwrap(),
            envelope: RealtimeEnvelope {
                id: "evt-7".into(),
                event_type: "order.updated".into(),
                occurred_at: "2026-08-04T03:50:44Z".into(),
                payload: serde_json::json!({"order_id": "order-7"}),
            },
        };

        service.publish(&publication).await.unwrap();

        assert_eq!(publisher.published(), vec![publication]);
    }

    #[tokio::test]
    async fn oversized_envelope_fails_before_publication() {
        let publisher = Arc::new(MemoryRealtimePublisher::default());
        let service = RealtimePublisherService::new(publisher.clone(), RealtimePlan::default());
        let publication = RealtimePublication {
            channel: RealtimeChannel::parse("orders/tenant-42").unwrap(),
            envelope: RealtimeEnvelope {
                id: "evt-8".into(),
                event_type: "order.updated".into(),
                occurred_at: "2026-08-04T03:50:44Z".into(),
                payload: serde_json::json!({"content": "x".repeat(DEFAULT_MAX_EVENT_BYTES)}),
            },
        };

        let error = service.publish(&publication).await.unwrap_err();

        assert!(matches!(error, RealtimeError::InvalidEnvelope(_)));
        assert!(publisher.published().is_empty());
    }

    #[tokio::test]
    async fn invalid_standalone_plan_fails_before_publication() {
        let publisher = Arc::new(MemoryRealtimePublisher::default());
        let service = RealtimePublisherService::new(
            publisher.clone(),
            RealtimePlan {
                max_event_bytes: 0,
                ..RealtimePlan::default()
            },
        );
        let publication = RealtimePublication {
            channel: RealtimeChannel::parse("orders/tenant-42").unwrap(),
            envelope: RealtimeEnvelope {
                id: "evt-9".into(),
                event_type: "order.updated".into(),
                occurred_at: "2026-08-04T03:50:44Z".into(),
                payload: serde_json::json!({"order_id": "order-7"}),
            },
        };

        let error = service.publish(&publication).await.unwrap_err();

        assert!(matches!(error, RealtimeError::InvalidConfiguration(_)));
        assert!(publisher.published().is_empty());
    }

    #[test]
    fn composition_installs_validated_plan_and_explicit_publisher() {
        let publisher = Arc::new(MemoryRealtimePublisher::default());
        let mut manager = PluginManager::default();
        manager
            .register(RealtimePlugin::default().with_publisher(publisher))
            .unwrap();
        let id = PluginId::new("realtime").unwrap();
        let mut selection = PluginSelection::default();
        selection.enabled.insert(id.clone());
        selection.configuration.insert(
            id,
            serde_json::json!({
                "namespace": "orders",
                "max_event_bytes": 4096,
                "subscriber_claim": "tenant_id"
            }),
        );

        let application = manager.compose(&selection).unwrap();

        let plan = application.services.get::<RealtimePlan>().unwrap();
        assert_eq!(plan.namespace, "orders");
        assert_eq!(plan.max_event_bytes, 4096);
        assert_eq!(plan.subscriber_claim, "tenant_id");
        assert!(
            application
                .services
                .get::<RealtimePublisherService>()
                .is_ok()
        );
        assert!(application.graph.resources.contains_key("realtime-api"));
    }
}
