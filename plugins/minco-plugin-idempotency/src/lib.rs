//! Atomic idempotency claims, request fingerprints, and deterministic replay primitives.
#![forbid(unsafe_code)]

use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use minco_core::{
    CapabilityProvision, ConfigurationField, ConfigurationValueKind, DataClass, Plugin,
    PluginContext, PluginDescriptor, PluginError, PluginId, PluginStability,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    pub fn parse(value: impl Into<String>) -> Result<Self, IdempotencyError> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > 200 || value.chars().any(char::is_control) {
            return Err(IdempotencyError::InvalidKey);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequestFingerprint(String);

impl RequestFingerprint {
    pub fn from_serializable<T: Serialize>(value: &T) -> Result<Self, IdempotencyError> {
        let bytes = serde_json::to_vec(value).map_err(IdempotencyError::Serialization)?;
        Ok(Self(format!("{:x}", Sha256::digest(bytes))))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, IdempotencyError> {
        let value = value.into();
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(IdempotencyError::InvalidFingerprint);
        }
        Ok(Self(value.to_ascii_lowercase()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdempotencyRecord {
    pub fingerprint: RequestFingerprint,
    pub response: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Exclusive claim returned before a caller performs the side effect.
///
/// Stores must compare the lease identifier during completion and abort so an expired worker
/// cannot overwrite the result produced by a newer claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyLease {
    pub key: IdempotencyKey,
    pub fingerprint: RequestFingerprint,
    pub lease_id: Uuid,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeginOutcome {
    Started(IdempotencyLease),
    Replay(IdempotencyRecord),
    Conflict,
    InProgress { started_at: DateTime<Utc> },
}

#[async_trait]
pub trait IdempotencyStore: Send + Sync + std::fmt::Debug {
    async fn get(
        &self,
        key: &IdempotencyKey,
    ) -> Result<Option<IdempotencyRecord>, IdempotencyError>;

    /// Atomically acquires an execution lease or returns the existing state.
    async fn begin(
        &self,
        key: IdempotencyKey,
        fingerprint: RequestFingerprint,
        now: DateTime<Utc>,
        stale_after: TimeDelta,
    ) -> Result<BeginOutcome, IdempotencyError>;

    /// Atomically replaces the matching in-progress lease with a completed response.
    async fn complete(
        &self,
        lease: IdempotencyLease,
        response: serde_json::Value,
        completed_at: DateTime<Utc>,
    ) -> Result<IdempotencyRecord, IdempotencyError>;

    /// Releases only the matching in-progress lease after the application side effect failed.
    async fn abort(&self, lease: &IdempotencyLease) -> Result<bool, IdempotencyError>;
}

#[derive(Debug, Clone)]
pub struct IdempotencyService {
    store: Arc<dyn IdempotencyStore>,
    stale_after: TimeDelta,
}

impl IdempotencyService {
    pub fn new(
        store: Arc<dyn IdempotencyStore>,
        stale_after: TimeDelta,
    ) -> Result<Self, IdempotencyError> {
        validate_claim_timeout(stale_after)?;
        Ok(Self { store, stale_after })
    }

    pub async fn get(
        &self,
        key: &IdempotencyKey,
    ) -> Result<Option<IdempotencyRecord>, IdempotencyError> {
        self.store.get(key).await
    }

    pub async fn begin(
        &self,
        key: IdempotencyKey,
        fingerprint: RequestFingerprint,
    ) -> Result<BeginOutcome, IdempotencyError> {
        self.store
            .begin(key, fingerprint, Utc::now(), self.stale_after)
            .await
    }

    pub async fn complete(
        &self,
        lease: IdempotencyLease,
        response: serde_json::Value,
    ) -> Result<IdempotencyRecord, IdempotencyError> {
        self.store.complete(lease, response, Utc::now()).await
    }

    pub async fn abort(&self, lease: &IdempotencyLease) -> Result<bool, IdempotencyError> {
        self.store.abort(lease).await
    }
}

#[derive(Debug, Clone)]
enum MemoryEntry {
    InProgress(IdempotencyLease),
    Completed(IdempotencyRecord),
}

#[derive(Debug, Default)]
pub struct MemoryIdempotencyStore {
    records: RwLock<BTreeMap<IdempotencyKey, MemoryEntry>>,
}

#[async_trait]
impl IdempotencyStore for MemoryIdempotencyStore {
    async fn get(
        &self,
        key: &IdempotencyKey,
    ) -> Result<Option<IdempotencyRecord>, IdempotencyError> {
        Ok(match self.records.read().await.get(key) {
            Some(MemoryEntry::Completed(record)) => Some(record.clone()),
            Some(MemoryEntry::InProgress(_)) | None => None,
        })
    }

    async fn begin(
        &self,
        key: IdempotencyKey,
        fingerprint: RequestFingerprint,
        now: DateTime<Utc>,
        stale_after: TimeDelta,
    ) -> Result<BeginOutcome, IdempotencyError> {
        validate_claim_timeout(stale_after)?;
        let mut records = self.records.write().await;
        let existing = records.get(&key).cloned();
        let outcome = match existing {
            Some(MemoryEntry::Completed(record)) => {
                if record.fingerprint == fingerprint {
                    BeginOutcome::Replay(record)
                } else {
                    BeginOutcome::Conflict
                }
            }
            Some(MemoryEntry::InProgress(existing)) => {
                if existing.fingerprint != fingerprint {
                    BeginOutcome::Conflict
                } else if existing.started_at + stale_after > now {
                    BeginOutcome::InProgress {
                        started_at: existing.started_at,
                    }
                } else {
                    let lease = IdempotencyLease {
                        key: key.clone(),
                        fingerprint,
                        lease_id: Uuid::new_v4(),
                        started_at: now,
                    };
                    records.insert(key, MemoryEntry::InProgress(lease.clone()));
                    BeginOutcome::Started(lease)
                }
            }
            None => {
                let lease = IdempotencyLease {
                    key: key.clone(),
                    fingerprint,
                    lease_id: Uuid::new_v4(),
                    started_at: now,
                };
                records.insert(key, MemoryEntry::InProgress(lease.clone()));
                BeginOutcome::Started(lease)
            }
        };
        drop(records);
        Ok(outcome)
    }

    async fn complete(
        &self,
        lease: IdempotencyLease,
        response: serde_json::Value,
        completed_at: DateTime<Utc>,
    ) -> Result<IdempotencyRecord, IdempotencyError> {
        let mut records = self.records.write().await;
        let Some(MemoryEntry::InProgress(current)) = records.get(&lease.key) else {
            return Err(IdempotencyError::InvalidLease);
        };
        if current.lease_id != lease.lease_id || current.fingerprint != lease.fingerprint {
            return Err(IdempotencyError::InvalidLease);
        }
        let record = IdempotencyRecord {
            fingerprint: lease.fingerprint,
            response,
            created_at: completed_at,
        };
        records.insert(lease.key, MemoryEntry::Completed(record.clone()));
        drop(records);
        Ok(record)
    }

    async fn abort(&self, lease: &IdempotencyLease) -> Result<bool, IdempotencyError> {
        let mut records = self.records.write().await;
        let matching = matches!(
            records.get(&lease.key),
            Some(MemoryEntry::InProgress(current))
                if current.lease_id == lease.lease_id && current.fingerprint == lease.fingerprint
        );
        if matching {
            records.remove(&lease.key);
        }
        drop(records);
        Ok(matching)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdempotencyPluginConfig {
    #[serde(default = "default_claim_timeout_seconds")]
    claim_timeout_seconds: i64,
}

impl Default for IdempotencyPluginConfig {
    fn default() -> Self {
        Self {
            claim_timeout_seconds: default_claim_timeout_seconds(),
        }
    }
}

const fn default_claim_timeout_seconds() -> i64 {
    300
}

#[derive(Debug, Clone)]
pub struct IdempotencyPlugin {
    store: Arc<dyn IdempotencyStore>,
}

impl IdempotencyPlugin {
    pub fn new(store: Arc<dyn IdempotencyStore>) -> Self {
        Self { store }
    }

    pub fn memory() -> Self {
        Self::new(Arc::new(MemoryIdempotencyStore::default()))
    }
}

impl Default for IdempotencyPlugin {
    fn default() -> Self {
        Self::memory()
    }
}

impl Plugin for IdempotencyPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("idempotency").expect("static ID"),
            Version::new(1, 0, 0),
            "Atomic idempotency claims, conflict detection, replay, and storage port",
        );
        descriptor.core_compatibility =
            VersionReq::parse(concat!("^", env!("CARGO_PKG_VERSION"))).expect("package version");
        descriptor.stability = PluginStability::Stable;
        descriptor.documentation = Some("https://docs.rs/minco-plugin-idempotency".into());
        descriptor.default_enabled = true;
        descriptor.data_classes.push(DataClass::Internal);
        descriptor.provides.extend([
            CapabilityProvision {
                name: "http.idempotency".into(),
                version: Version::new(1, 0, 0),
            },
            CapabilityProvision {
                name: "idempotency.store".into(),
                version: Version::new(1, 0, 0),
            },
            CapabilityProvision {
                name: "idempotency.claim".into(),
                version: Version::new(1, 0, 0),
            },
        ]);
        descriptor.configuration.push(ConfigurationField {
            key: "claim_timeout_seconds".into(),
            kind: ConfigurationValueKind::Integer,
            required: false,
            secret: false,
            description: "Time after which an abandoned in-progress claim may be recovered".into(),
            default: Some(serde_json::json!(default_claim_timeout_seconds())),
        });
        descriptor
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        let config = context.configuration::<IdempotencyPluginConfig>()?;
        let service = IdempotencyService::new(
            Arc::clone(&self.store),
            TimeDelta::seconds(config.claim_timeout_seconds),
        )
        .map_err(|error| PluginError::Installation(error.to_string()))?;
        context.services().insert(Arc::new(service))?;
        Ok(())
    }
}

pub fn validate_claim_timeout(stale_after: TimeDelta) -> Result<(), IdempotencyError> {
    if stale_after <= TimeDelta::zero() || stale_after > TimeDelta::hours(24) {
        Err(IdempotencyError::InvalidClaimTimeout)
    } else {
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IdempotencyError {
    #[error("idempotency key must contain 1-200 visible characters")]
    InvalidKey,
    #[error("request fingerprint must be a 64-character hexadecimal SHA-256 digest")]
    InvalidFingerprint,
    #[error("idempotency claim timeout must be greater than zero and no more than 24 hours")]
    InvalidClaimTimeout,
    #[error("idempotency lease is stale, missing, or belongs to another worker")]
    InvalidLease,
    #[error("failed to serialize request fingerprint: {0}")]
    Serialization(serde_json::Error),
    #[error("idempotency store failed: {0}")]
    Store(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_core::{PluginManager, PluginSelection};

    #[test]
    fn claim_timeout_boundary_is_shared_by_services_and_stores() {
        assert!(validate_claim_timeout(TimeDelta::seconds(1)).is_ok());
        assert!(validate_claim_timeout(TimeDelta::zero()).is_err());
        assert!(validate_claim_timeout(TimeDelta::hours(24) + TimeDelta::seconds(1)).is_err());
    }

    #[tokio::test]
    async fn claim_protocol_prevents_concurrent_side_effects_and_replays_completion() {
        let service = IdempotencyService::new(
            Arc::new(MemoryIdempotencyStore::default()),
            TimeDelta::minutes(5),
        )
        .unwrap();
        let key = IdempotencyKey::parse("request-1").unwrap();
        let fingerprint =
            RequestFingerprint::from_serializable(&serde_json::json!({"a": 1})).unwrap();
        let lease = match service
            .begin(key.clone(), fingerprint.clone())
            .await
            .unwrap()
        {
            BeginOutcome::Started(lease) => lease,
            other => panic!("expected a new lease, got {other:?}"),
        };
        assert!(matches!(
            service
                .begin(key.clone(), fingerprint.clone())
                .await
                .unwrap(),
            BeginOutcome::InProgress { .. }
        ));
        let other = RequestFingerprint::from_serializable(&serde_json::json!({"a": 2})).unwrap();
        assert_eq!(
            service.begin(key.clone(), other).await.unwrap(),
            BeginOutcome::Conflict
        );
        let record = service
            .complete(lease, serde_json::json!({"ok": true}))
            .await
            .unwrap();
        assert_eq!(
            service.begin(key, fingerprint).await.unwrap(),
            BeginOutcome::Replay(record)
        );
    }

    #[tokio::test]
    async fn abort_releases_only_the_matching_lease() {
        let service = IdempotencyService::new(
            Arc::new(MemoryIdempotencyStore::default()),
            TimeDelta::minutes(5),
        )
        .unwrap();
        let key = IdempotencyKey::parse("request-2").unwrap();
        let fingerprint = RequestFingerprint::from_serializable(&"payload").unwrap();
        let BeginOutcome::Started(lease) = service
            .begin(key.clone(), fingerprint.clone())
            .await
            .unwrap()
        else {
            unreachable!()
        };
        assert!(service.abort(&lease).await.unwrap());
        assert!(matches!(
            service.begin(key, fingerprint).await.unwrap(),
            BeginOutcome::Started(_)
        ));
    }

    #[tokio::test]
    async fn plugin_exposes_an_injectable_claim_service() {
        let mut manager = PluginManager::default();
        manager.register(IdempotencyPlugin::memory()).unwrap();
        let application = manager.compose(&PluginSelection::default()).unwrap();
        let service = application.services.get::<IdempotencyService>().unwrap();
        let key = IdempotencyKey::parse("request-3").unwrap();
        assert!(service.get(&key).await.unwrap().is_none());
    }

    #[test]
    fn descriptor_matches_the_stable_catalog_contract() {
        assert_eq!(
            IdempotencyPlugin::memory().descriptor().stability,
            PluginStability::Stable
        );
    }
}
