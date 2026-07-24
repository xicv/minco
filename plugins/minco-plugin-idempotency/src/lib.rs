//! Default idempotency primitives and a deterministic in-memory implementation for tests/local use.
#![forbid(unsafe_code)]

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use minco_core::{CapabilityProvision, Plugin, PluginContext, PluginDescriptor, PluginError, PluginId};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::RwLock;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IdempotencyKey(String);
impl IdempotencyKey {
    pub fn parse(value: impl Into<String>) -> Result<Self, IdempotencyError> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > 200 || value.chars().any(char::is_control) { return Err(IdempotencyError::InvalidKey); }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str { &self.0 }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequestFingerprint(String);
impl RequestFingerprint {
    pub fn from_serializable<T: Serialize>(value: &T) -> Result<Self, IdempotencyError> {
        let bytes = serde_json::to_vec(value).map_err(IdempotencyError::Serialization)?;
        Ok(Self(format!("{:x}", Sha256::digest(bytes))))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdempotencyRecord {
    pub fingerprint: RequestFingerprint,
    pub response: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[async_trait]
pub trait IdempotencyStore: Send + Sync + std::fmt::Debug {
    async fn get(&self, key: &IdempotencyKey) -> Result<Option<IdempotencyRecord>, IdempotencyError>;
    async fn put_if_absent(&self, key: IdempotencyKey, record: IdempotencyRecord) -> Result<PutOutcome, IdempotencyError>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum PutOutcome { Inserted, Existing(IdempotencyRecord) }

#[derive(Debug, Default)]
pub struct MemoryIdempotencyStore { records: RwLock<BTreeMap<IdempotencyKey, IdempotencyRecord>> }

#[async_trait]
impl IdempotencyStore for MemoryIdempotencyStore {
    async fn get(&self, key: &IdempotencyKey) -> Result<Option<IdempotencyRecord>, IdempotencyError> { Ok(self.records.read().await.get(key).cloned()) }
    async fn put_if_absent(&self, key: IdempotencyKey, record: IdempotencyRecord) -> Result<PutOutcome, IdempotencyError> {
        let mut records = self.records.write().await;
        if let Some(existing) = records.get(&key) { return Ok(PutOutcome::Existing(existing.clone())); }
        records.insert(key, record);
        Ok(PutOutcome::Inserted)
    }
}

#[derive(Debug, Clone, Default)]
pub struct IdempotencyPlugin;
impl Plugin for IdempotencyPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(PluginId::new("idempotency").expect("static id"), Version::new(1,0,0), "Idempotency keys, fingerprints and storage port");
        descriptor.default_enabled = true;
        descriptor.provides.push(CapabilityProvision { name: "http.idempotency".into(), version: Version::new(1,0,0) });
        descriptor
    }
    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        context.services().insert(Arc::new(MemoryIdempotencyStore::default()))?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IdempotencyError {
    #[error("idempotency key must contain 1-200 visible characters")]
    InvalidKey,
    #[error("failed to serialize request fingerprint: {0}")]
    Serialization(serde_json::Error),
    #[error("idempotency store failed: {0}")]
    Store(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn put_if_absent_returns_the_original_record() {
        let store = MemoryIdempotencyStore::default();
        let key = IdempotencyKey::parse("request-1").unwrap();
        let record = IdempotencyRecord { fingerprint: RequestFingerprint::from_serializable(&serde_json::json!({"a":1})).unwrap(), response: serde_json::json!({"ok":true}), created_at: Utc::now() };
        assert_eq!(store.put_if_absent(key.clone(), record.clone()).await.unwrap(), PutOutcome::Inserted);
        assert_eq!(store.put_if_absent(key, record.clone()).await.unwrap(), PutOutcome::Existing(record));
    }
}
