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
/// correlation, timestamp or metadata) returns an integrity-conflict
/// error — the stable [`AUDIT_CONFLICT_CODE`] carried by
/// [`AuditError::Append`], detectable with [`is_audit_conflict`] —
/// never a silent success. The ticketing dispatcher redelivers
/// at-least-once with the intent id as the audit event id, so this
/// contract is what makes the pipeline exactly-once observable.
/// Adapters MUST reject events with empty action or resource id.
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
/// reviews R24 and R31): a SHA-256 digest over a length-framed
/// canonical encoding of everything except the event id itself.
///
/// Length framing (a fixed-width hex length prefix per field) makes the
/// encoding collision-free even when field values contain bytes that
/// look like delimiters, unlike plain concatenation.
pub fn event_fingerprint(event: &AuditEvent) -> String {
    fingerprint_from_parts(
        &event.action,
        &event.resource_type,
        &event.resource_id,
        event.actor_subject.as_deref(),
        &event.correlation_id.to_string(),
        &canonical_occurred_at(event.occurred_at),
        &canonical_metadata(&event.metadata),
    )
}

/// Fixed-format UTC timestamp used inside fingerprints.
///
/// Microsecond precision is what every persistence adapter round-trips,
/// so a fingerprint computed from a fresh event equals the one
/// recomputed from the same event read back out of storage.
pub fn canonical_occurred_at(occurred_at: DateTime<Utc>) -> String {
    occurred_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()
}

/// Canonicalize a timestamp exactly as stored by an adapter.
///
/// `SQLite` TEXT timestamps may be RFC 3339 (`T`-separated) or sqlx's
/// chrono encoding (space-separated, optional fraction/offset); all
/// accepted spellings of the same instant must canonicalize equally so
/// legacy backfill verification (R27) cannot produce false conflicts.
pub fn canonical_stored_occurred_at(stored: &str) -> String {
    let candidates = [
        stored.to_string(),
        stored.replacen(' ', "T", 1),
        format!("{}+00:00", stored.replacen(' ', "T", 1)),
    ];
    for candidate in &candidates {
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(candidate) {
            return canonical_occurred_at(parsed.with_timezone(&Utc));
        }
    }
    stored.to_string()
}

/// Canonical JSON encoding of audit metadata.
///
/// Object keys are recursively sorted and insignificant whitespace
/// removed. Metadata is a `BTreeMap`, but adapters recompute
/// fingerprints from stored JSON, so the canonical form is rebuilt
/// rather than trusted.
pub fn canonical_metadata(metadata: &BTreeMap<String, serde_json::Value>) -> String {
    let value = serde_json::Value::Object(
        metadata
            .iter()
            .map(|(key, value)| (key.clone(), sort_json(value)))
            .collect(),
    );
    serde_json::to_string(&value).unwrap_or_default()
}

fn sort_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(key, inner)| (key.clone(), sort_json(inner)))
                .collect(),
        ),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(sort_json).collect())
        }
        other => other.clone(),
    }
}

/// Fingerprint recomputation from stored parts. Adapters use this to
/// content-verify pre-fingerprint rows before adopting the digest
/// (safe backfill, exact-head review R27).
#[allow(clippy::too_many_arguments)]
pub fn fingerprint_from_parts(
    action: &str,
    resource_type: &str,
    resource_id: &str,
    actor_subject: Option<&str>,
    correlation_id: &str,
    canonical_occurred_at: &str,
    canonical_metadata_json: &str,
) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    let mut encoded = Vec::new();
    for part in [
        action,
        resource_type,
        resource_id,
        actor_subject.unwrap_or(""),
        correlation_id,
        canonical_occurred_at,
        canonical_metadata_json,
    ] {
        encoded.extend_from_slice(format!("{:016x}", part.len()).as_bytes());
        encoded.extend_from_slice(part.as_bytes());
    }
    let digest = Sha256::digest(&encoded);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
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
                Err(audit_conflict_error())
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
}

/// Stable machine code prefix marking an integrity conflict carried by
/// [`AuditError::Append`] (exact-head review 5060065907).
///
/// The same event id redelivered with a DIFFERENT semantic fingerprint
/// carries this code. The variant set of `AuditError` is a published
/// compatibility boundary — the conflict channel stays inside `Append`
/// so downstream exhaustive matches from 1.x keep compiling.
pub const AUDIT_CONFLICT_CODE: &str = "MINCO-AUDIT-CONFLICT";

/// Builds the conflict error through the stable `Append` channel.
#[must_use]
pub fn audit_conflict_error() -> AuditError {
    AuditError::Append(format!(
        "{AUDIT_CONFLICT_CODE}: same event id with different semantic fingerprint"
    ))
}

/// Returns true when an error is the stable-coded integrity conflict
/// (see [`AUDIT_CONFLICT_CODE`]) — works identically for the memory,
/// `SQLite` and `PostgreSQL` sinks.
#[must_use]
pub fn is_audit_conflict(error: &AuditError) -> bool {
    matches!(
        error,
        AuditError::Append(message) if message.starts_with(AUDIT_CONFLICT_CODE)
    )
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
        assert!(is_audit_conflict(&error));
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
    fn fingerprint_is_a_stable_sixty_four_hex_digest() {
        let event = AuditEvent::new(
            "ticketing.created",
            "ticketing.ticket",
            "one",
            uuid::Uuid::new_v4(),
        );
        let digest = event_fingerprint(&event);
        assert_eq!(digest.len(), 64);
        assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(digest, event_fingerprint(&event));
    }

    #[test]
    fn fingerprint_framing_survives_delimiter_injection() {
        // Exact-head review R31/P1-1: plain delimiter concatenation
        // lets distinct tuples collide when field values contain the
        // delimiter; length framing must not.
        let left = fingerprint_from_parts(
            "a|b|c",
            "d",
            "e",
            None,
            "00000000-0000-0000-0000-000000000001",
            "2026-08-29T00:00:00.000000Z",
            "{}",
        );
        let right = fingerprint_from_parts(
            "a",
            "b|c|d",
            "e",
            None,
            "00000000-0000-0000-0000-000000000001",
            "2026-08-29T00:00:00.000000Z",
            "{}",
        );
        assert_ne!(left, right);
        // Same framing characters inside metadata values, too.
        let mut with_delimiters = AuditEvent::new(
            "ticketing.noted",
            "ticketing.ticket",
            "one",
            uuid::Uuid::new_v4(),
        );
        with_delimiters.metadata.insert(
            "note|".into(),
            serde_json::json!({"deep|key": "value|with|pipes"}),
        );
        let mut split_differently = with_delimiters.clone();
        split_differently.metadata.remove("note|");
        split_differently.metadata.insert(
            "note".into(),
            serde_json::json!({"deep": "value|with|pipes", "deep|key": "value"}),
        );
        assert_ne!(
            event_fingerprint(&with_delimiters),
            event_fingerprint(&split_differently)
        );
    }

    #[test]
    fn fingerprint_recomputation_matches_from_stored_parts() {
        // The adapter backfill path (R27) recomputes from stored
        // columns; the canonical timestamp normalizes through the same
        // instant regardless of storage spelling.
        let event = AuditEvent::new(
            "queue.created",
            "ticketing.queue",
            "queue-9",
            uuid::Uuid::new_v4(),
        );
        let canonical = canonical_occurred_at(event.occurred_at);
        let space_separated = canonical.replacen('T', " ", 1).replace('Z', "+00:00");
        assert_eq!(
            event_fingerprint(&event),
            fingerprint_from_parts(
                &event.action,
                &event.resource_type,
                &event.resource_id,
                event.actor_subject.as_deref(),
                &event.correlation_id.to_string(),
                &canonical,
                &canonical_metadata(&event.metadata),
            )
        );
        assert_eq!(
            canonical_stored_occurred_at(&space_separated),
            canonical,
            "space-separated sqlx chrono encoding must canonicalize to the same digest"
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
