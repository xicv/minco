//! Additive V2 contracts for durable, queryable and lifecycle-aware audit ledgers.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use minco_core::DataClass;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Maximum encoded size of one V2 record before provider contact.
pub const MAX_AUDIT_RECORD_BYTES: usize = 64 * 1024;
/// Maximum records in one portable ledger batch.
pub const MAX_AUDIT_BATCH_RECORDS: usize = 100;
/// Maximum encoded payload in one portable ledger batch.
pub const MAX_AUDIT_BATCH_BYTES: usize = 4 * 1024 * 1024;
/// Maximum number of explicit related resources on one record.
pub const MAX_AUDIT_RELATED_RESOURCES: usize = 8;
/// Maximum changed fields on one record.
pub const MAX_AUDIT_CHANGED_FIELDS: usize = 128;
/// Maximum safe labels on one record.
pub const MAX_AUDIT_LABELS: usize = 32;
/// Maximum records returned by one resource-history query.
pub const MAX_AUDIT_PAGE_SIZE: usize = 100;
/// Maximum encoded size of one literal changed-field value.
pub const MAX_AUDIT_LITERAL_BYTES: usize = 4 * 1024;

const MIB: u64 = 1024 * 1024;

/// Opaque application resource identity. It never implies a database foreign key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditResourceRef {
    pub resource_type: String,
    pub resource_id: String,
}

impl AuditResourceRef {
    pub fn new(resource_type: impl Into<String>, resource_id: impl Into<String>) -> Self {
        Self {
            resource_type: resource_type.into(),
            resource_id: resource_id.into(),
        }
    }
}

/// A bounded relationship used to gather child or adjacent action history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditRelatedResource {
    pub relation: String,
    pub resource: AuditResourceRef,
}

/// Stable kind of principal responsible for an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditActorKind {
    Human,
    Service,
    System,
    Migration,
    DatabasePrincipal,
    Unknown,
}

/// Actor snapshot retained without a foreign key to an identity table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditActor {
    pub kind: AuditActorKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_subject: Option<String>,
}

impl AuditActor {
    pub fn human(subject: impl Into<String>) -> Self {
        Self {
            kind: AuditActorKind::Human,
            subject: Some(subject.into()),
            effective_subject: None,
        }
    }

    #[must_use]
    pub const fn unknown() -> Self {
        Self {
            kind: AuditActorKind::Unknown,
            subject: None,
            effective_subject: None,
        }
    }
}

/// Origin distinguishes authoritative semantic actions from supporting evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOrigin {
    Application,
    Migration,
    DatabaseEvidence,
    Import,
}

/// A field value that makes privacy handling explicit in the record itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuditValue {
    Literal { value: serde_json::Value },
    Redacted,
    Digest { digest: AuditDigest },
    Omitted,
}

impl AuditValue {
    pub fn literal(value: impl Into<serde_json::Value>) -> Self {
        Self::Literal {
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditDigestAlgorithm {
    Sha256,
}

/// Bounded digest used instead of retaining a sensitive raw value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditDigest {
    pub algorithm: AuditDigestAlgorithm,
    pub value: String,
}

impl AuditDigest {
    pub fn sha256(value: impl Into<String>) -> Self {
        Self {
            algorithm: AuditDigestAlgorithm::Sha256,
            value: value.into(),
        }
    }

    fn validate(&self) -> Result<(), AuditLedgerError> {
        if self.value.len() != 64 || !self.value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(AuditLedgerError::InvalidRecord(
                "SHA-256 digest must contain exactly 64 hexadecimal bytes".into(),
            ));
        }
        Ok(())
    }
}

/// Before/after representation for one allowlisted field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditFieldChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<AuditValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<AuditValue>,
}

/// Position inside one source transaction when multiple actions commit together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditTransactionRef {
    pub id: Uuid,
    pub ordinal: u32,
}

/// Durable semantic action record. This is additive to the legacy `AuditEvent`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditRecordV2 {
    pub schema_version: u16,
    pub event_id: Uuid,
    pub tenant_scope: String,
    pub action: String,
    pub resource: AuditResourceRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_resources: Vec<AuditRelatedResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_revision: Option<u64>,
    pub actor: AuditActor,
    pub operation_id: String,
    pub correlation_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key_digest: Option<AuditDigest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction: Option<AuditTransactionRef>,
    pub occurred_at: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
    pub origin: AuditOrigin,
    pub data_class: DataClass,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub changes: BTreeMap<String, AuditFieldChange>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
}

impl AuditRecordV2 {
    pub fn new(
        tenant_scope: impl Into<String>,
        action: impl Into<String>,
        resource: AuditResourceRef,
        actor: AuditActor,
        operation_id: impl Into<String>,
        correlation_id: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            schema_version: 2,
            event_id: Uuid::now_v7(),
            tenant_scope: tenant_scope.into(),
            action: action.into(),
            resource,
            related_resources: Vec::new(),
            resource_revision: None,
            actor,
            operation_id: operation_id.into(),
            correlation_id,
            causation_id: None,
            idempotency_key_digest: None,
            transaction: None,
            occurred_at: now,
            recorded_at: now,
            origin: AuditOrigin::Application,
            data_class: DataClass::Internal,
            changes: BTreeMap::new(),
            labels: BTreeMap::new(),
        }
    }

    /// Validates portable provider limits and returns the encoded byte size.
    pub fn validate(&self) -> Result<usize, AuditLedgerError> {
        if self.schema_version != 2 {
            return Err(AuditLedgerError::InvalidRecord(
                "schema_version must be 2".into(),
            ));
        }
        if self.event_id.is_nil() || self.correlation_id.is_nil() {
            return Err(AuditLedgerError::InvalidRecord(
                "event_id and correlation_id must be non-nil".into(),
            ));
        }
        if self.causation_id.is_some_and(|id| id.is_nil())
            || self
                .transaction
                .as_ref()
                .is_some_and(|item| item.id.is_nil())
        {
            return Err(AuditLedgerError::InvalidRecord(
                "causation and transaction IDs must be non-nil when present".into(),
            ));
        }
        validate_text("tenant_scope", &self.tenant_scope, 256)?;
        validate_text("action", &self.action, 128)?;
        validate_resource(&self.resource)?;
        validate_text("operation_id", &self.operation_id, 128)?;
        validate_optional_text("actor.subject", self.actor.subject.as_deref(), 512)?;
        validate_optional_text(
            "actor.effective_subject",
            self.actor.effective_subject.as_deref(),
            512,
        )?;
        if let Some(digest) = &self.idempotency_key_digest {
            digest.validate()?;
        }
        match self.actor.kind {
            AuditActorKind::Unknown
                if self.actor.subject.is_some() || self.actor.effective_subject.is_some() =>
            {
                return Err(AuditLedgerError::InvalidRecord(
                    "unknown actors cannot carry an attributed subject".into(),
                ));
            }
            AuditActorKind::Unknown => {}
            _ if self.actor.subject.is_none() => {
                return Err(AuditLedgerError::InvalidRecord(
                    "attributed actors require a stable subject".into(),
                ));
            }
            _ => {}
        }
        if self.related_resources.len() > MAX_AUDIT_RELATED_RESOURCES {
            return Err(AuditLedgerError::InvalidRecord(format!(
                "related_resources exceeds {MAX_AUDIT_RELATED_RESOURCES}"
            )));
        }
        let mut related = BTreeSet::new();
        for relation in &self.related_resources {
            validate_text("related_resources.relation", &relation.relation, 64)?;
            validate_resource(&relation.resource)?;
            let key = (
                relation.relation.as_str(),
                relation.resource.resource_type.as_str(),
                relation.resource.resource_id.as_str(),
            );
            if !related.insert(key) {
                return Err(AuditLedgerError::InvalidRecord(
                    "related_resources contains a duplicate".into(),
                ));
            }
        }
        if self.changes.len() > MAX_AUDIT_CHANGED_FIELDS {
            return Err(AuditLedgerError::InvalidRecord(format!(
                "changes exceeds {MAX_AUDIT_CHANGED_FIELDS}"
            )));
        }
        for (field, change) in &self.changes {
            validate_text("changes field", field, 128)?;
            if change.before == change.after {
                return Err(AuditLedgerError::InvalidRecord(format!(
                    "change {field} has identical before and after values"
                )));
            }
            validate_audit_value(change.before.as_ref(), self.data_class)?;
            validate_audit_value(change.after.as_ref(), self.data_class)?;
        }
        if self.labels.len() > MAX_AUDIT_LABELS {
            return Err(AuditLedgerError::InvalidRecord(format!(
                "labels exceeds {MAX_AUDIT_LABELS}"
            )));
        }
        for (key, value) in &self.labels {
            validate_text("label key", key, 64)?;
            validate_text("label value", value, 512)?;
        }
        let bytes = serde_json::to_vec(self)
            .map_err(|_| AuditLedgerError::Encoding)?
            .len();
        if bytes > MAX_AUDIT_RECORD_BYTES {
            return Err(AuditLedgerError::RecordTooLarge {
                bytes,
                maximum: MAX_AUDIT_RECORD_BYTES,
            });
        }
        Ok(bytes)
    }
}

fn validate_resource(resource: &AuditResourceRef) -> Result<(), AuditLedgerError> {
    validate_text("resource_type", &resource.resource_type, 128)?;
    validate_text("resource_id", &resource.resource_id, 512)
}

fn validate_text(name: &str, value: &str, maximum: usize) -> Result<(), AuditLedgerError> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(AuditLedgerError::InvalidRecord(format!(
            "{name} must contain between 1 and {maximum} non-control bytes"
        )));
    }
    Ok(())
}

fn validate_optional_text(
    name: &str,
    value: Option<&str>,
    maximum: usize,
) -> Result<(), AuditLedgerError> {
    value.map_or(Ok(()), |value| validate_text(name, value, maximum))
}

fn validate_audit_value(
    value: Option<&AuditValue>,
    data_class: DataClass,
) -> Result<(), AuditLedgerError> {
    match value {
        Some(AuditValue::Literal { value }) => {
            if data_class == DataClass::Secret {
                return Err(AuditLedgerError::InvalidRecord(
                    "secret-class audit changes must be redacted, digested or omitted".into(),
                ));
            }
            if value.is_array() || value.is_object() {
                return Err(AuditLedgerError::InvalidRecord(
                    "literal audit values must be scalar".into(),
                ));
            }
            let bytes = serde_json::to_vec(value)
                .map_err(|_| AuditLedgerError::Encoding)?
                .len();
            if bytes > MAX_AUDIT_LITERAL_BYTES {
                return Err(AuditLedgerError::InvalidRecord(format!(
                    "literal audit value exceeds {MAX_AUDIT_LITERAL_BYTES} bytes"
                )));
            }
        }
        Some(AuditValue::Digest { digest }) => digest.validate()?,
        Some(AuditValue::Redacted | AuditValue::Omitted) | None => {}
    }
    Ok(())
}

/// Stable ledger position independent of a physical table, file or segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditCursor {
    pub occurred_at: DateTime<Utc>,
    pub event_id: Uuid,
}

impl From<&AuditRecordV2> for AuditCursor {
    fn from(record: &AuditRecordV2) -> Self {
        Self {
            occurred_at: record.occurred_at,
            event_id: record.event_id,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditSortDirection {
    OldestFirst,
    #[default]
    NewestFirst,
}

/// Bounded resource-history query. Authorization remains in the application use case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditQuery {
    pub tenant_scope: String,
    pub resource: AuditResourceRef,
    #[serde(default)]
    pub include_related: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
    #[serde(default)]
    pub direction: AuditSortDirection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<AuditCursor>,
    pub limit: usize,
}

impl AuditQuery {
    pub fn for_resource(tenant_scope: impl Into<String>, resource: AuditResourceRef) -> Self {
        Self {
            tenant_scope: tenant_scope.into(),
            resource,
            include_related: false,
            relation: None,
            direction: AuditSortDirection::NewestFirst,
            after: None,
            limit: 50,
        }
    }

    pub fn validate(&self) -> Result<(), AuditLedgerError> {
        validate_text("tenant_scope", &self.tenant_scope, 256)?;
        validate_resource(&self.resource)?;
        validate_optional_text("relation", self.relation.as_deref(), 64)?;
        if self.relation.is_some() && !self.include_related {
            return Err(AuditLedgerError::InvalidQuery(
                "relation requires include_related".into(),
            ));
        }
        if self.limit == 0 || self.limit > MAX_AUDIT_PAGE_SIZE {
            return Err(AuditLedgerError::InvalidQuery(format!(
                "limit must be between 1 and {MAX_AUDIT_PAGE_SIZE}"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditPage {
    pub records: Vec<AuditRecordV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<AuditCursor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditAppendReport {
    pub requested: usize,
    pub inserted: usize,
    pub duplicates: usize,
}

#[async_trait]
pub trait AuditLedgerWriter: Send + Sync + std::fmt::Debug {
    /// Atomically appends a bounded batch.
    ///
    /// Repeating the same ID and content is an idempotent duplicate. Reusing an
    /// ID for different content fails the complete batch.
    async fn append_batch(
        &self,
        records: &[AuditRecordV2],
    ) -> Result<AuditAppendReport, AuditLedgerError>;
}

#[async_trait]
pub trait AuditReader: Send + Sync + std::fmt::Debug {
    async fn list_resource_history(
        &self,
        query: &AuditQuery,
    ) -> Result<AuditPage, AuditLedgerError>;
}

/// Size thresholds for finite-disk ledgers. `None` means not applicable, not unchecked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditSizePolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warn_at_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotate_at_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reject_at_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_free_bytes: Option<u64>,
}

impl AuditSizePolicy {
    /// Initial `SQLite` recommendation: 80 MiB warning, 100 MiB rotation and
    /// 125 MiB hard stop. Deployments must separately size free-disk reserve.
    #[must_use]
    pub const fn sqlite_100_mib(minimum_free_bytes: u64) -> Self {
        Self {
            warn_at_bytes: Some(80 * MIB),
            rotate_at_bytes: Some(100 * MIB),
            reject_at_bytes: Some(125 * MIB),
            minimum_free_bytes: Some(minimum_free_bytes),
        }
    }

    fn validate(self) -> Result<(), AuditLedgerError> {
        let ordered = [
            self.warn_at_bytes,
            self.rotate_at_bytes,
            self.reject_at_bytes,
        ];
        if ordered.iter().any(Option::is_some) && ordered.iter().any(Option::is_none) {
            return Err(AuditLedgerError::InvalidLifecycle(
                "warn, rotate and reject byte thresholds must be declared together".into(),
            ));
        }
        if let [Some(warn), Some(rotate), Some(reject)] = ordered
            && (warn == 0 || warn >= rotate || rotate >= reject)
        {
            return Err(AuditLedgerError::InvalidLifecycle(
                "byte thresholds must satisfy 0 < warn < rotate < reject".into(),
            ));
        }
        Ok(())
    }
}

/// Retention requires archive proof before any provider may delete hot history.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditRetentionPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_after_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delete_after_seconds: Option<u64>,
    #[serde(default)]
    pub require_archive_receipt: bool,
}

impl AuditRetentionPolicy {
    fn validate(self) -> Result<(), AuditLedgerError> {
        if let Some(delete_after) = self.delete_after_seconds {
            let archive_after = self.archive_after_seconds.ok_or_else(|| {
                AuditLedgerError::InvalidLifecycle(
                    "delete_after_seconds requires archive_after_seconds".into(),
                )
            })?;
            if delete_after < archive_after || !self.require_archive_receipt {
                return Err(AuditLedgerError::InvalidLifecycle(
                    "deletion must follow archive and require an archive receipt".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditLifecyclePolicy {
    pub size: AuditSizePolicy,
    pub retention: AuditRetentionPolicy,
    pub maximum_pending_records: u64,
    pub maximum_pending_bytes: u64,
    pub maximum_oldest_pending_seconds: u64,
}

impl AuditLifecyclePolicy {
    #[must_use]
    pub const fn cloud_online() -> Self {
        Self {
            size: AuditSizePolicy {
                warn_at_bytes: None,
                rotate_at_bytes: None,
                reject_at_bytes: None,
                minimum_free_bytes: None,
            },
            retention: AuditRetentionPolicy {
                archive_after_seconds: None,
                delete_after_seconds: None,
                require_archive_receipt: false,
            },
            maximum_pending_records: 0,
            maximum_pending_bytes: 0,
            maximum_oldest_pending_seconds: 0,
        }
    }

    #[must_use]
    pub const fn sqlite_100_mib(minimum_free_bytes: u64) -> Self {
        Self {
            size: AuditSizePolicy::sqlite_100_mib(minimum_free_bytes),
            retention: AuditRetentionPolicy {
                archive_after_seconds: None,
                delete_after_seconds: None,
                require_archive_receipt: false,
            },
            maximum_pending_records: 100_000,
            maximum_pending_bytes: 64 * MIB,
            maximum_oldest_pending_seconds: 3_600,
        }
    }

    pub fn validate(self) -> Result<(), AuditLedgerError> {
        self.size.validate()?;
        self.retention.validate()?;
        let pending = [
            self.maximum_pending_records,
            self.maximum_pending_bytes,
            self.maximum_oldest_pending_seconds,
        ];
        if pending.iter().any(|value| *value > 0) && pending.contains(&0) {
            return Err(AuditLedgerError::InvalidLifecycle(
                "pending record, byte and age limits must be declared together".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditSegmentState {
    Active,
    Sealed,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditArchiveReceipt {
    pub archive_id: String,
    pub digest: AuditDigest,
    pub archived_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditSegmentStatus {
    pub segment_id: u64,
    pub state: AuditSegmentState,
    pub record_count: u64,
    pub encoded_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first: Option<AuditCursor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<AuditCursor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_receipt: Option<AuditArchiveReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditStorageSnapshot {
    pub provider: String,
    pub hot_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_bytes: Option<u64>,
    pub pending_records: u64,
    pub pending_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oldest_pending_seconds: Option<u64>,
    pub quarantined_records: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_watermark: Option<AuditCursor>,
    pub segments: Vec<AuditSegmentStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditHealthSeverity {
    Healthy,
    Warning,
    RotationRequired,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditStorageHealth {
    pub severity: AuditHealthSeverity,
    pub reasons: Vec<String>,
    pub snapshot: AuditStorageSnapshot,
}

#[async_trait]
pub trait AuditStorageInspector: Send + Sync + std::fmt::Debug {
    async fn storage_health(&self) -> Result<AuditStorageHealth, AuditLedgerError>;
}

/// Applies one provider-neutral severity model to a provider snapshot.
pub fn evaluate_storage_health(
    policy: AuditLifecyclePolicy,
    snapshot: AuditStorageSnapshot,
) -> Result<AuditStorageHealth, AuditLedgerError> {
    policy.validate()?;
    validate_text("storage provider", &snapshot.provider, 128)?;
    let mut severity = AuditHealthSeverity::Healthy;
    let mut reasons = Vec::new();
    let active_bytes = snapshot
        .segments
        .iter()
        .find(|segment| segment.state == AuditSegmentState::Active)
        .map_or(0, |segment| segment.encoded_bytes);
    if let Some(warn) = policy.size.warn_at_bytes
        && active_bytes >= warn
    {
        severity = severity.max(AuditHealthSeverity::Warning);
        reasons.push(format!("active segment reached warning threshold {warn}"));
    }
    if let Some(rotate) = policy.size.rotate_at_bytes
        && active_bytes >= rotate
    {
        severity = severity.max(AuditHealthSeverity::RotationRequired);
        reasons.push(format!(
            "active segment reached rotation threshold {rotate}"
        ));
    }
    if let Some(reject) = policy.size.reject_at_bytes
        && active_bytes >= reject
    {
        severity = AuditHealthSeverity::Critical;
        reasons.push(format!(
            "active segment reached rejection threshold {reject}"
        ));
    }
    if let (Some(free), Some(minimum)) = (snapshot.free_bytes, policy.size.minimum_free_bytes) {
        if free < minimum {
            severity = AuditHealthSeverity::Critical;
            reasons.push(format!(
                "free storage {free} is below required reserve {minimum}"
            ));
        } else if free < minimum.saturating_add(minimum / 4) {
            severity = severity.max(AuditHealthSeverity::Warning);
            reasons.push(format!(
                "free storage {free} is approaching required reserve {minimum}"
            ));
        }
    }
    evaluate_limit(
        snapshot.pending_records,
        policy.maximum_pending_records,
        "pending journal record",
        &mut severity,
        &mut reasons,
    );
    evaluate_limit(
        snapshot.pending_bytes,
        policy.maximum_pending_bytes,
        "pending journal byte",
        &mut severity,
        &mut reasons,
    );
    if let Some(age) = snapshot.oldest_pending_seconds {
        evaluate_limit(
            age,
            policy.maximum_oldest_pending_seconds,
            "oldest pending journal age",
            &mut severity,
            &mut reasons,
        );
    }
    if snapshot.quarantined_records > 0 {
        severity = severity.max(AuditHealthSeverity::Warning);
        reasons.push("quarantined audit records require attention".into());
    }
    Ok(AuditStorageHealth {
        severity,
        reasons,
        snapshot,
    })
}

fn evaluate_limit(
    value: u64,
    limit: u64,
    name: &str,
    severity: &mut AuditHealthSeverity,
    reasons: &mut Vec<String>,
) {
    if limit == 0 {
        return;
    }
    if value >= limit {
        *severity = AuditHealthSeverity::Critical;
        reasons.push(format!("{name} limit {limit} reached"));
    } else if value >= limit.saturating_sub(limit / 5) {
        *severity = (*severity).max(AuditHealthSeverity::Warning);
        reasons.push(format!("{name} is approaching limit {limit}"));
    }
}

#[derive(Clone)]
pub struct AuditLedgerServices {
    pub writer: Arc<dyn AuditLedgerWriter>,
    pub reader: Arc<dyn AuditReader>,
    pub inspector: Arc<dyn AuditStorageInspector>,
}

impl std::fmt::Debug for AuditLedgerServices {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuditLedgerServices")
            .finish_non_exhaustive()
    }
}

impl AuditLedgerServices {
    pub fn new<L>(ledger: Arc<L>) -> Self
    where
        L: AuditLedgerWriter + AuditReader + AuditStorageInspector + 'static,
    {
        Self {
            writer: ledger.clone(),
            reader: ledger.clone(),
            inspector: ledger,
        }
    }

    #[must_use]
    pub fn from_parts(
        writer: Arc<dyn AuditLedgerWriter>,
        reader: Arc<dyn AuditReader>,
        inspector: Arc<dyn AuditStorageInspector>,
    ) -> Self {
        Self {
            writer,
            reader,
            inspector,
        }
    }
}

#[derive(Debug, Clone)]
struct MemorySegment {
    status: AuditSegmentStatus,
    record_ids: Vec<Uuid>,
}

impl MemorySegment {
    const fn active(segment_id: u64) -> Self {
        Self {
            status: AuditSegmentStatus {
                segment_id,
                state: AuditSegmentState::Active,
                record_count: 0,
                encoded_bytes: 0,
                first: None,
                last: None,
                archive_receipt: None,
            },
            record_ids: Vec::new(),
        }
    }
}

#[derive(Debug)]
struct MemoryState {
    records: BTreeMap<Uuid, AuditRecordV2>,
    segments: Vec<MemorySegment>,
}

/// Deterministic segmented reference ledger used by tests and local composition.
#[derive(Debug)]
pub struct MemoryAuditLedger {
    policy: AuditLifecyclePolicy,
    state: RwLock<MemoryState>,
}

impl Default for MemoryAuditLedger {
    fn default() -> Self {
        Self::new(AuditLifecyclePolicy::cloud_online()).expect("static memory policy")
    }
}

impl MemoryAuditLedger {
    pub fn new(policy: AuditLifecyclePolicy) -> Result<Self, AuditLedgerError> {
        policy.validate()?;
        Ok(Self {
            policy,
            state: RwLock::new(MemoryState {
                records: BTreeMap::new(),
                segments: vec![MemorySegment::active(1)],
            }),
        })
    }

    pub async fn segment_statuses(&self) -> Vec<AuditSegmentStatus> {
        self.state
            .read()
            .await
            .segments
            .iter()
            .map(|segment| segment.status.clone())
            .collect()
    }

    pub async fn mark_archived(
        &self,
        segment_id: u64,
        receipt: AuditArchiveReceipt,
    ) -> Result<(), AuditLedgerError> {
        validate_text("archive_id", &receipt.archive_id, 512)?;
        receipt.digest.validate()?;
        let mut state = self.state.write().await;
        let segment = state
            .segments
            .iter_mut()
            .find(|segment| segment.status.segment_id == segment_id)
            .ok_or(AuditLedgerError::MissingSegment(segment_id))?;
        if segment.status.state != AuditSegmentState::Sealed {
            return Err(AuditLedgerError::InvalidSegmentState(segment_id));
        }
        segment.status.state = AuditSegmentState::Archived;
        segment.status.archive_receipt = Some(receipt);
        drop(state);
        Ok(())
    }
}

#[async_trait]
impl AuditLedgerWriter for MemoryAuditLedger {
    async fn append_batch(
        &self,
        records: &[AuditRecordV2],
    ) -> Result<AuditAppendReport, AuditLedgerError> {
        if records.is_empty() || records.len() > MAX_AUDIT_BATCH_RECORDS {
            return Err(AuditLedgerError::InvalidBatch(format!(
                "batch must contain between 1 and {MAX_AUDIT_BATCH_RECORDS} records"
            )));
        }
        let mut encoded_sizes = Vec::with_capacity(records.len());
        let mut total_bytes = 0usize;
        let mut request_records = BTreeMap::new();
        let mut duplicate_request_ids = 0usize;
        for record in records {
            let bytes = record.validate()?;
            total_bytes = total_bytes
                .checked_add(bytes)
                .ok_or_else(|| AuditLedgerError::InvalidBatch("batch bytes overflow".into()))?;
            if total_bytes > MAX_AUDIT_BATCH_BYTES {
                return Err(AuditLedgerError::BatchTooLarge {
                    bytes: total_bytes,
                    maximum: MAX_AUDIT_BATCH_BYTES,
                });
            }
            if let Some(existing) = request_records.insert(record.event_id, record) {
                if existing != record {
                    return Err(AuditLedgerError::EventConflict(record.event_id));
                }
                duplicate_request_ids += 1;
            } else {
                encoded_sizes.push((record.event_id, bytes));
            }
        }

        let mut state = self.state.write().await;
        let mut new_records = Vec::new();
        let mut duplicates = duplicate_request_ids;
        for (event_id, record) in &request_records {
            match state.records.get(event_id) {
                Some(existing) if existing == *record => duplicates += 1,
                Some(_) => return Err(AuditLedgerError::EventConflict(*event_id)),
                None => new_records.push((*event_id, (*record).clone())),
            }
        }

        let mut planned_segments = state.segments.clone();
        for (event_id, record) in &new_records {
            let bytes = encoded_sizes
                .iter()
                .find_map(|(id, bytes)| (id == event_id).then_some(*bytes))
                .expect("validated unique record size");
            let bytes = u64::try_from(bytes).map_err(|_| AuditLedgerError::Encoding)?;
            let active = planned_segments.last_mut().expect("active memory segment");
            if let Some(rotate) = self.policy.size.rotate_at_bytes
                && active.status.record_count > 0
                && active.status.encoded_bytes.saturating_add(bytes) > rotate
            {
                active.status.state = AuditSegmentState::Sealed;
                let next_id = active.status.segment_id.saturating_add(1);
                planned_segments.push(MemorySegment::active(next_id));
            }
            let active = planned_segments
                .last_mut()
                .expect("new active memory segment");
            let projected = active.status.encoded_bytes.saturating_add(bytes);
            if self
                .policy
                .size
                .reject_at_bytes
                .is_some_and(|reject| projected > reject)
            {
                return Err(AuditLedgerError::StorageLimitExceeded { bytes: projected });
            }
            let cursor = AuditCursor::from(record);
            active.status.first = active.status.first.or(Some(cursor));
            active.status.last = Some(cursor);
            active.status.record_count = active.status.record_count.saturating_add(1);
            active.status.encoded_bytes = projected;
            active.record_ids.push(*event_id);
        }

        let inserted = new_records.len();
        for (event_id, record) in new_records {
            state.records.insert(event_id, record);
        }
        state.segments = planned_segments;
        drop(state);
        Ok(AuditAppendReport {
            requested: records.len(),
            inserted,
            duplicates,
        })
    }
}

#[async_trait]
impl AuditReader for MemoryAuditLedger {
    async fn list_resource_history(
        &self,
        query: &AuditQuery,
    ) -> Result<AuditPage, AuditLedgerError> {
        query.validate()?;
        let mut records = {
            let state = self.state.read().await;
            state
                .records
                .values()
                .filter(|record| record.tenant_scope == query.tenant_scope)
                .filter(|record| {
                    if record.resource == query.resource {
                        return true;
                    }
                    query.include_related
                        && record.related_resources.iter().any(|related| {
                            related.resource == query.resource
                                && query
                                    .relation
                                    .as_ref()
                                    .is_none_or(|relation| relation == &related.relation)
                        })
                })
                .cloned()
                .collect::<Vec<_>>()
        };
        records.sort_by_key(|record| AuditCursor::from(record));
        if query.direction == AuditSortDirection::NewestFirst {
            records.reverse();
        }
        if let Some(after) = query.after {
            records.retain(|record| match query.direction {
                AuditSortDirection::OldestFirst => AuditCursor::from(record) > after,
                AuditSortDirection::NewestFirst => AuditCursor::from(record) < after,
            });
        }
        let has_more = records.len() > query.limit;
        records.truncate(query.limit);
        let next_cursor = has_more.then(|| {
            records
                .last()
                .map(AuditCursor::from)
                .expect("positive validated page limit")
        });
        Ok(AuditPage {
            records,
            next_cursor,
        })
    }
}

#[async_trait]
impl AuditStorageInspector for MemoryAuditLedger {
    async fn storage_health(&self) -> Result<AuditStorageHealth, AuditLedgerError> {
        let segments = {
            let state = self.state.read().await;
            state
                .segments
                .iter()
                .map(|segment| segment.status.clone())
                .collect::<Vec<_>>()
        };
        let hot_bytes = segments
            .iter()
            .filter(|segment| segment.state != AuditSegmentState::Archived)
            .map(|segment| segment.encoded_bytes)
            .sum();
        let archive_watermark = segments
            .iter()
            .filter(|segment| segment.state == AuditSegmentState::Archived)
            .filter_map(|segment| segment.last)
            .max();
        let snapshot = AuditStorageSnapshot {
            provider: "memory".into(),
            hot_bytes,
            free_bytes: None,
            pending_records: 0,
            pending_bytes: 0,
            oldest_pending_seconds: None,
            quarantined_records: 0,
            archive_watermark,
            segments,
        };
        evaluate_storage_health(self.policy, snapshot)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuditLedgerError {
    #[error("invalid audit record: {0}")]
    InvalidRecord(String),
    #[error("audit record is {bytes} bytes; maximum is {maximum}")]
    RecordTooLarge { bytes: usize, maximum: usize },
    #[error("invalid audit batch: {0}")]
    InvalidBatch(String),
    #[error("audit batch is {bytes} bytes; maximum is {maximum}")]
    BatchTooLarge { bytes: usize, maximum: usize },
    #[error("audit event ID {0} was reused for different content")]
    EventConflict(Uuid),
    #[error("invalid audit query: {0}")]
    InvalidQuery(String),
    #[error("invalid audit lifecycle policy: {0}")]
    InvalidLifecycle(String),
    #[error("audit storage hard limit would be exceeded at {bytes} bytes")]
    StorageLimitExceeded { bytes: u64 },
    #[error("audit segment {0} does not exist")]
    MissingSegment(u64),
    #[error("audit segment {0} is not sealed and cannot be archived")]
    InvalidSegmentState(u64),
    #[error("audit encoding failed")]
    Encoding,
    #[error("audit ledger operation failed")]
    Infrastructure,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    fn record(index: u32) -> AuditRecordV2 {
        let occurred_at = DateTime::from_timestamp(1_800_000_000 + i64::from(index), 0).unwrap();
        let mut record = AuditRecordV2::new(
            "tenant-one",
            "order.status_changed",
            AuditResourceRef::new("order", "order-one"),
            AuditActor::human("user-one"),
            "updateOrder",
            Uuid::from_u128(1),
        );
        record.event_id = Uuid::from_u128(10_000 + u128::from(index));
        record.occurred_at = occurred_at;
        record.recorded_at = occurred_at + TimeDelta::seconds(1);
        record.resource_revision = Some(u64::from(index));
        record.changes.insert(
            "status".into(),
            AuditFieldChange {
                before: Some(AuditValue::literal(format!("before-{index}"))),
                after: Some(AuditValue::literal(format!("after-{index}"))),
            },
        );
        record
    }

    fn tiny_rotation_policy() -> AuditLifecyclePolicy {
        AuditLifecyclePolicy {
            size: AuditSizePolicy {
                warn_at_bytes: Some(1),
                rotate_at_bytes: Some(1_200),
                reject_at_bytes: Some(64 * 1024),
                minimum_free_bytes: None,
            },
            retention: AuditRetentionPolicy::default(),
            maximum_pending_records: 0,
            maximum_pending_bytes: 0,
            maximum_oldest_pending_seconds: 0,
        }
    }

    #[test]
    fn record_rejects_unbounded_or_noop_changes() {
        let mut invalid = record(1);
        invalid.changes.insert(
            "same".into(),
            AuditFieldChange {
                before: Some(AuditValue::Redacted),
                after: Some(AuditValue::Redacted),
            },
        );
        assert!(matches!(
            invalid.validate(),
            Err(AuditLedgerError::InvalidRecord(_))
        ));

        let mut oversized = record(2);
        oversized
            .labels
            .insert("large".into(), "x".repeat(MAX_AUDIT_RECORD_BYTES));
        assert!(matches!(
            oversized.validate(),
            Err(AuditLedgerError::InvalidRecord(_) | AuditLedgerError::RecordTooLarge { .. })
        ));
    }

    #[tokio::test]
    async fn batch_append_is_idempotent_and_conflicts_fail_before_writes() {
        let ledger = MemoryAuditLedger::default();
        let first = record(1);
        let second = record(2);
        let report = ledger
            .append_batch(&[first.clone(), second.clone()])
            .await
            .unwrap();
        assert_eq!(report.inserted, 2);
        assert_eq!(report.duplicates, 0);

        let duplicate = ledger
            .append_batch(std::slice::from_ref(&first))
            .await
            .unwrap();
        assert_eq!(duplicate.inserted, 0);
        assert_eq!(duplicate.duplicates, 1);

        let mut conflict = first.clone();
        conflict.action = "order.deleted".into();
        let third = record(3);
        assert!(matches!(
            ledger.append_batch(&[third, conflict]).await,
            Err(AuditLedgerError::EventConflict(id)) if id == first.event_id
        ));
        let page = ledger
            .list_resource_history(&AuditQuery::for_resource(
                "tenant-one",
                AuditResourceRef::new("order", "order-one"),
            ))
            .await
            .unwrap();
        assert_eq!(page.records.len(), 2);
    }

    #[tokio::test]
    async fn concurrent_duplicate_delivery_creates_one_record() {
        let ledger = Arc::new(MemoryAuditLedger::default());
        let action = record(1);
        let left = {
            let ledger = ledger.clone();
            let action = action.clone();
            tokio::spawn(async move { ledger.append_batch(&[action]).await.unwrap() })
        };
        let right = {
            let ledger = ledger.clone();
            tokio::spawn(async move { ledger.append_batch(&[action]).await.unwrap() })
        };
        let left = left.await.unwrap();
        let right = right.await.unwrap();
        assert_eq!(left.inserted + right.inserted, 1);
        assert_eq!(left.duplicates + right.duplicates, 1);
    }

    #[tokio::test]
    async fn cursor_pages_cross_rotated_and_archived_segments_without_gaps() {
        let ledger = MemoryAuditLedger::new(tiny_rotation_policy()).unwrap();
        let records = (1..=8).map(record).collect::<Vec<_>>();
        ledger.append_batch(&records).await.unwrap();
        let segments = ledger.segment_statuses().await;
        assert!(segments.len() >= 2);
        assert_eq!(segments.last().unwrap().state, AuditSegmentState::Active);

        let sealed = segments
            .iter()
            .find(|segment| segment.state == AuditSegmentState::Sealed)
            .unwrap();
        ledger
            .mark_archived(
                sealed.segment_id,
                AuditArchiveReceipt {
                    archive_id: "archive/orders/segment-1".into(),
                    digest: AuditDigest::sha256("a".repeat(64)),
                    archived_at: Utc::now(),
                },
            )
            .await
            .unwrap();

        let mut query =
            AuditQuery::for_resource("tenant-one", AuditResourceRef::new("order", "order-one"));
        query.direction = AuditSortDirection::OldestFirst;
        query.limit = 3;
        let mut revisions = Vec::new();
        loop {
            let page = ledger.list_resource_history(&query).await.unwrap();
            revisions.extend(
                page.records
                    .iter()
                    .map(|record| record.resource_revision.unwrap()),
            );
            let Some(cursor) = page.next_cursor else {
                break;
            };
            query.after = Some(cursor);
        }
        assert_eq!(revisions, (1_u32..=8).map(u64::from).collect::<Vec<_>>());
        assert!(
            ledger
                .storage_health()
                .await
                .unwrap()
                .snapshot
                .archive_watermark
                .is_some()
        );
    }

    #[tokio::test]
    async fn related_resource_query_is_explicit_and_relation_scoped() {
        let ledger = MemoryAuditLedger::default();
        let direct = record(1);
        let mut related = record(2);
        related.resource = AuditResourceRef::new("shift", "shift-one");
        related.related_resources.push(AuditRelatedResource {
            relation: "order".into(),
            resource: AuditResourceRef::new("order", "order-one"),
        });
        ledger.append_batch(&[direct, related]).await.unwrap();

        let mut query =
            AuditQuery::for_resource("tenant-one", AuditResourceRef::new("order", "order-one"));
        assert_eq!(
            ledger
                .list_resource_history(&query)
                .await
                .unwrap()
                .records
                .len(),
            1
        );
        query.include_related = true;
        query.relation = Some("order".into());
        assert_eq!(
            ledger
                .list_resource_history(&query)
                .await
                .unwrap()
                .records
                .len(),
            2
        );
    }

    #[test]
    fn lifecycle_rejects_unsafe_deletion_and_incoherent_size_thresholds() {
        let invalid_delete = AuditLifecyclePolicy {
            size: AuditSizePolicy::default(),
            retention: AuditRetentionPolicy {
                archive_after_seconds: Some(60),
                delete_after_seconds: Some(120),
                require_archive_receipt: false,
            },
            maximum_pending_records: 0,
            maximum_pending_bytes: 0,
            maximum_oldest_pending_seconds: 0,
        };
        assert!(matches!(
            invalid_delete.validate(),
            Err(AuditLedgerError::InvalidLifecycle(_))
        ));

        let invalid_sizes = AuditLifecyclePolicy {
            size: AuditSizePolicy {
                warn_at_bytes: Some(100),
                rotate_at_bytes: Some(50),
                reject_at_bytes: Some(200),
                minimum_free_bytes: None,
            },
            ..AuditLifecyclePolicy::cloud_online()
        };
        assert!(matches!(
            invalid_sizes.validate(),
            Err(AuditLedgerError::InvalidLifecycle(_))
        ));
    }

    #[test]
    fn sensitive_values_require_bounded_explicit_representation() {
        let mut nested = record(1);
        nested.changes.insert(
            "profile".into(),
            AuditFieldChange {
                before: None,
                after: Some(AuditValue::literal(serde_json::json!({"token": "unsafe"}))),
            },
        );
        assert!(matches!(
            nested.validate(),
            Err(AuditLedgerError::InvalidRecord(_))
        ));

        let mut secret = record(2);
        secret.data_class = DataClass::Secret;
        assert!(matches!(
            secret.validate(),
            Err(AuditLedgerError::InvalidRecord(_))
        ));
        secret.changes.values_mut().for_each(|change| {
            change.before = Some(AuditValue::Redacted);
            change.after = Some(AuditValue::Digest {
                digest: AuditDigest::sha256("b".repeat(64)),
            });
        });
        secret.validate().unwrap();

        let mut unknown = record(3);
        unknown.actor = AuditActor {
            kind: AuditActorKind::Unknown,
            subject: Some("claimed-user".into()),
            effective_subject: None,
        };
        assert!(matches!(
            unknown.validate(),
            Err(AuditLedgerError::InvalidRecord(_))
        ));
    }

    #[test]
    fn storage_health_warns_early_and_fails_at_disk_or_backlog_limits() {
        let policy = AuditLifecyclePolicy {
            size: AuditSizePolicy {
                warn_at_bytes: Some(100),
                rotate_at_bytes: Some(200),
                reject_at_bytes: Some(300),
                minimum_free_bytes: Some(1_000),
            },
            retention: AuditRetentionPolicy::default(),
            maximum_pending_records: 100,
            maximum_pending_bytes: 1_000,
            maximum_oldest_pending_seconds: 100,
        };
        let snapshot = AuditStorageSnapshot {
            provider: "sqlite".into(),
            hot_bytes: 150,
            free_bytes: Some(2_000),
            pending_records: 80,
            pending_bytes: 200,
            oldest_pending_seconds: Some(10),
            quarantined_records: 0,
            archive_watermark: None,
            segments: vec![AuditSegmentStatus {
                segment_id: 1,
                state: AuditSegmentState::Active,
                record_count: 1,
                encoded_bytes: 150,
                first: None,
                last: None,
                archive_receipt: None,
            }],
        };
        let warning = evaluate_storage_health(policy, snapshot.clone()).unwrap();
        assert_eq!(warning.severity, AuditHealthSeverity::Warning);
        assert!(
            warning
                .reasons
                .iter()
                .any(|reason| reason.contains("approaching limit"))
        );

        let critical = evaluate_storage_health(
            policy,
            AuditStorageSnapshot {
                free_bytes: Some(999),
                pending_records: 100,
                ..snapshot
            },
        )
        .unwrap();
        assert_eq!(critical.severity, AuditHealthSeverity::Critical);
    }
}
