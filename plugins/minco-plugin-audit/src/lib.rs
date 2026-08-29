//! Append-only audit events and a deterministic memory reference sink.
#![forbid(unsafe_code)]

mod journal;
mod v2;

pub use journal::*;
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

/// Durable audit sink contract (exact-head review R17).
///
/// `append` MUST be idempotent by (event id, semantic fingerprint):
/// re-appending the SAME event succeeds without duplication, while the
/// same id with DIFFERENT content (action, resource, actor,
/// correlation, timestamp or metadata) returns
/// [`AuditError::Conflict`] — an integrity violation, never a silent
/// success. The ticketing dispatcher redelivers at-least-once with the
/// intent id as the audit event id, so this contract is what makes the
/// pipeline exactly-once observable. Adapters MUST reject events with
/// empty action or resource id.
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

/// Canonical semantic fingerprint of one audit event (exact-head
/// review R24/P1-1): everything except the event id itself.
pub fn event_fingerprint(event: &AuditEvent) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}",
        event.action,
        event.resource_type,
        event.resource_id,
        event.actor_subject.as_deref().unwrap_or(""),
        event.correlation_id,
        event.occurred_at.to_rfc3339(),
        serde_json::to_string(&event.metadata).unwrap_or_default()
    )
}

#[async_trait]
impl AuditSink for MemoryAuditSink {
    async fn append(&self, event: AuditEvent) -> Result<(), AuditError> {
        if event.action.trim().is_empty() || event.resource_id.trim().is_empty() {
            return Err(AuditError::InvalidEvent);
        }
        let mut events = self.events.write().await;
        // Idempotent by (event id, semantic fingerprint) (exact-head
        // reviews R17 and R24): a redelivery of the identical record is
        // a silent success; the same id with DIFFERENT content is an
        // integrity conflict.
        if let Some(existing) = events.iter().find(|existing| existing.id == event.id) {
            let fingerprint_matches = event_fingerprint(existing) == event_fingerprint(&event);
            drop(events);
            return if fingerprint_matches {
                Ok(())
            } else {
                Err(AuditError::Conflict)
            };
        }
        events.push(event);
        drop(events);
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

    #[must_use]
    pub fn with_ledger_services(mut self, services: AuditLedgerServices) -> Self {
        self.ledger = Some(services);
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
    /// Same event id with a DIFFERENT semantic fingerprint (exact-head
    /// review R24/P1-1): an integrity conflict, never an idempotent
    /// success.
    #[error("audit event id conflict: same id with different content")]
    Conflict,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn same_id_different_content_is_a_conflict() {
        // Exact-head review R24/P1-1: the same event id with a
        // different semantic fingerprint is an integrity conflict,
        // never an idempotent success.
        let sink = MemoryAuditSink::default();
        let mut first = AuditEvent::new(
            "ticketing.created",
            "ticketing.ticket",
            "one",
            uuid::Uuid::new_v4(),
        );
        first.id = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, b"stable");
        sink.append(first.clone()).await.unwrap();
        let mut impostor = first.clone();
        impostor.action = "ticketing.deleted".into();
        let error = sink.append(impostor).await.unwrap_err();
        assert!(matches!(error, AuditError::Conflict));
        let all = sink.all().await;
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].action, "ticketing.created");
    }

    #[tokio::test]
    async fn append_is_idempotent_by_event_id() {
        // Exact-head review R17: the sink contract guarantees appending
        // the same event id twice succeeds without duplication.
        let sink = MemoryAuditSink::default();
        let mut event = AuditEvent::new(
            "ticketing.created",
            "ticketing.ticket",
            "one",
            uuid::Uuid::new_v4(),
        );
        event.id = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, b"stable-intent");
        sink.append(event.clone()).await.unwrap();
        sink.append(event.clone()).await.unwrap();
        sink.append(event).await.unwrap();
        let all = sink.all().await;
        assert_eq!(all.len(), 1, "duplicate ids never duplicate the ledger");
    }

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
