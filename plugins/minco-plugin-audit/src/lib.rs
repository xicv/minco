//! Append-only audit events and a deterministic memory reference sink.
#![forbid(unsafe_code)]

mod v2;

pub use v2::*;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use minco_core::{
    CapabilityProvision, DataClass, Plugin, PluginContext, PluginDescriptor, PluginError, PluginId,
    PluginStability,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: Uuid,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_subject: Option<String>,
    pub correlation_id: Uuid,
    pub occurred_at: DateTime<Utc>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

impl AuditEvent {
    pub fn new(
        action: impl Into<String>,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
        correlation_id: Uuid,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            action: action.into(),
            resource_type: resource_type.into(),
            resource_id: resource_id.into(),
            actor_subject: None,
            correlation_id,
            occurred_at: Utc::now(),
            metadata: BTreeMap::new(),
        }
    }
}

#[async_trait]
pub trait AuditSink: Send + Sync + std::fmt::Debug {
    async fn append(&self, event: AuditEvent) -> Result<(), AuditError>;
}

#[derive(Clone)]
pub struct AuditService(pub Arc<dyn AuditSink>);

impl std::fmt::Debug for AuditService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("AuditService").finish()
    }
}

impl AuditService {
    pub fn new(sink: Arc<dyn AuditSink>) -> Self {
        Self(sink)
    }

    pub async fn append(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.0.append(event).await
    }
}

#[derive(Debug, Default)]
pub struct MemoryAuditSink {
    events: RwLock<Vec<AuditEvent>>,
}

impl MemoryAuditSink {
    pub async fn all(&self) -> Vec<AuditEvent> {
        self.events.read().await.clone()
    }
}

#[async_trait]
impl AuditSink for MemoryAuditSink {
    async fn append(&self, event: AuditEvent) -> Result<(), AuditError> {
        if event.action.trim().is_empty() || event.resource_id.trim().is_empty() {
            return Err(AuditError::InvalidEvent);
        }
        self.events.write().await.push(event);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AuditPlugin {
    service: AuditService,
    ledger: Option<AuditLedgerServices>,
}

impl AuditPlugin {
    pub fn new(sink: Arc<dyn AuditSink>) -> Self {
        Self {
            service: AuditService::new(sink),
            ledger: None,
        }
    }

    #[must_use]
    pub fn with_ledger<L>(mut self, ledger: Arc<L>) -> Self
    where
        L: AuditLedgerWriter + AuditReader + AuditStorageInspector + 'static,
    {
        self.ledger = Some(AuditLedgerServices::new(ledger));
        self
    }

    pub fn memory() -> (Self, Arc<MemoryAuditSink>) {
        let sink = Arc::new(MemoryAuditSink::default());
        let ledger = Arc::new(MemoryAuditLedger::default());
        (Self::new(sink.clone()).with_ledger(ledger), sink)
    }

    pub fn memory_v2() -> (Self, Arc<MemoryAuditSink>, Arc<MemoryAuditLedger>) {
        let sink = Arc::new(MemoryAuditSink::default());
        let ledger = Arc::new(MemoryAuditLedger::default());
        (
            Self::new(sink.clone()).with_ledger(ledger.clone()),
            sink,
            ledger,
        )
    }
}

impl Plugin for AuditPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("audit").expect("static plugin ID"),
            Version::new(1, 0, 0),
            "Durable append-only audit history independent of operational logs",
        );
        descriptor.documentation = Some("https://docs.rs/minco-plugin-audit".into());
        descriptor.core_compatibility =
            VersionReq::parse(concat!("^", env!("CARGO_PKG_VERSION"))).expect("package version");
        descriptor.stability = PluginStability::Beta;
        descriptor.data_classes.extend([
            DataClass::Internal,
            DataClass::Personal,
            DataClass::Confidential,
        ]);
        descriptor.provides.push(CapabilityProvision {
            name: "audit.append".into(),
            version: Version::new(1, 0, 0),
        });
        if self.ledger.is_some() {
            descriptor.provides.extend([
                CapabilityProvision {
                    name: "audit.ledger".into(),
                    version: Version::new(2, 0, 0),
                },
                CapabilityProvision {
                    name: "audit.query".into(),
                    version: Version::new(2, 0, 0),
                },
                CapabilityProvision {
                    name: "audit.health".into(),
                    version: Version::new(1, 0, 0),
                },
            ]);
        }
        descriptor
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        context.services().insert(Arc::new(self.service.clone()))?;
        if let Some(ledger) = &self.ledger {
            context.services().insert(Arc::new(ledger.clone()))?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("audit events require a non-empty action and resource ID")]
    InvalidEvent,
    #[error("audit append failed: {0}")]
    Append(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_sink_is_append_only_and_ordered() {
        let sink = MemoryAuditSink::default();
        let first = AuditEvent::new("feedback.created", "feedback", "one", Uuid::now_v7());
        let second = AuditEvent::new("feedback.replied", "feedback", "one", Uuid::now_v7());
        sink.append(first).await.unwrap();
        sink.append(second).await.unwrap();
        assert_eq!(
            sink.all()
                .await
                .iter()
                .map(|event| event.action.as_str())
                .collect::<Vec<_>>(),
            ["feedback.created", "feedback.replied"]
        );
    }

    #[test]
    fn memory_plugin_advertises_additive_v2_capabilities() {
        let descriptor = AuditPlugin::memory().0.descriptor();
        assert_eq!(descriptor.version, Version::new(1, 0, 0));
        assert!(
            descriptor
                .provides
                .iter()
                .any(|capability| capability.name == "audit.ledger")
        );
    }
}
