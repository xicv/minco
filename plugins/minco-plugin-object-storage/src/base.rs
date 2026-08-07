//! Provider-neutral object storage and a deterministic in-memory reference adapter.
#![forbid(unsafe_code)]

use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use minco_core::{
    CapabilityProvision, DataClass, Plugin, PluginContext, PluginDescriptor, PluginError, PluginId,
    PluginStability,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::RwLock;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObjectKey(String);

impl ObjectKey {
    pub fn parse(value: impl Into<String>) -> Result<Self, ObjectStoreError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 1024
            || value.starts_with('/')
            || value.ends_with('/')
            || value.split('/').any(|part| {
                part.is_empty() || part == "." || part == ".." || part.chars().any(char::is_control)
            })
        {
            return Err(ObjectStoreError::InvalidKey(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectMetadata {
    pub content_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredObject {
    pub key: ObjectKey,
    pub bytes: Vec<u8>,
    pub metadata: ObjectMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PutObject {
    pub key: ObjectKey,
    pub bytes: Vec<u8>,
    pub content_type: String,
    pub attributes: BTreeMap<String, String>,
}

#[async_trait]
pub trait ObjectStore: Send + Sync + std::fmt::Debug {
    async fn put(&self, object: PutObject) -> Result<ObjectMetadata, ObjectStoreError>;
    async fn get(&self, key: &ObjectKey) -> Result<Option<StoredObject>, ObjectStoreError>;
    async fn delete(&self, key: &ObjectKey) -> Result<bool, ObjectStoreError>;
}

#[derive(Clone)]
pub struct ObjectStoreService(pub Arc<dyn ObjectStore>);

impl std::fmt::Debug for ObjectStoreService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("ObjectStoreService").finish()
    }
}

impl ObjectStoreService {
    pub fn new(store: Arc<dyn ObjectStore>) -> Self {
        Self(store)
    }

    pub async fn put(&self, object: PutObject) -> Result<ObjectMetadata, ObjectStoreError> {
        self.0.put(object).await
    }

    pub async fn get(&self, key: &ObjectKey) -> Result<Option<StoredObject>, ObjectStoreError> {
        self.0.get(key).await
    }

    pub async fn delete(&self, key: &ObjectKey) -> Result<bool, ObjectStoreError> {
        self.0.delete(key).await
    }
}

/// HTTP method required by a signed direct-object request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PresignedMethod {
    Get,
    Put,
    Post,
}

/// Browser- or client-usable request produced by a provider adapter such as S3.
///
/// `form_fields` is populated for multipart POST uploads. This is required for
/// providers such as S3 where the signed POST policy, rather than a presigned
/// PUT URL, enforces an upload-size range.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresignedObjectRequest {
    pub method: PresignedMethod,
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub form_fields: BTreeMap<String, String>,
    pub expires_at: DateTime<Utc>,
}

impl std::fmt::Debug for PresignedObjectRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PresignedObjectRequest")
            .field("method", &self.method)
            .field("url", &"[REDACTED PRESIGNED URL]")
            .field("header_names", &self.headers.keys().collect::<Vec<_>>())
            .field(
                "form_field_names",
                &self.form_fields.keys().collect::<Vec<_>>(),
            )
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresignPutObject {
    pub key: ObjectKey,
    pub content_type: String,
    pub maximum_size_bytes: u64,
    pub expires_in: TimeDelta,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresignGetObject {
    pub key: ObjectKey,
    pub expires_in: TimeDelta,
    pub download_file_name: Option<String>,
}

/// Provider adapter for direct upload and download URLs.
///
/// Keeping signing separate from [`ObjectStore`] lets applications use server-side storage without
/// exposing direct browser access. AWS implementations can map this port to S3 presigning while
/// local/test implementations remain deterministic.
#[async_trait]
pub trait ObjectAccessSigner: Send + Sync + std::fmt::Debug {
    async fn sign_put(
        &self,
        request: PresignPutObject,
    ) -> Result<PresignedObjectRequest, ObjectStoreError>;

    async fn sign_get(
        &self,
        request: PresignGetObject,
    ) -> Result<PresignedObjectRequest, ObjectStoreError>;
}

#[derive(Clone)]
pub struct ObjectAccessService(pub Arc<dyn ObjectAccessSigner>);

impl std::fmt::Debug for ObjectAccessService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("ObjectAccessService").finish()
    }
}

impl ObjectAccessService {
    pub fn new(signer: Arc<dyn ObjectAccessSigner>) -> Self {
        Self(signer)
    }

    pub async fn sign_put(
        &self,
        request: PresignPutObject,
    ) -> Result<PresignedObjectRequest, ObjectStoreError> {
        validate_expiry(request.expires_in)?;
        if request.content_type.trim().is_empty() {
            return Err(ObjectStoreError::InvalidContentType);
        }
        if request.maximum_size_bytes == 0 {
            return Err(ObjectStoreError::InvalidMaximumSize);
        }
        self.0.sign_put(request).await
    }

    pub async fn sign_get(
        &self,
        request: PresignGetObject,
    ) -> Result<PresignedObjectRequest, ObjectStoreError> {
        validate_expiry(request.expires_in)?;
        self.0.sign_get(request).await
    }
}

fn validate_expiry(expires_in: TimeDelta) -> Result<(), ObjectStoreError> {
    if expires_in <= TimeDelta::zero() || expires_in > TimeDelta::hours(24) {
        return Err(ObjectStoreError::InvalidExpiry);
    }
    Ok(())
}

#[derive(Debug, Default)]
pub struct MemoryObjectStore {
    objects: RwLock<BTreeMap<ObjectKey, StoredObject>>,
}

impl MemoryObjectStore {
    /// Number of objects currently retained by the deterministic memory adapter.
    ///
    /// This is primarily useful for conformance tests and local diagnostics.
    pub async fn len(&self) -> usize {
        self.objects.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

#[async_trait]
impl ObjectStore for MemoryObjectStore {
    async fn put(&self, object: PutObject) -> Result<ObjectMetadata, ObjectStoreError> {
        if object.content_type.trim().is_empty() {
            return Err(ObjectStoreError::InvalidContentType);
        }
        let metadata = ObjectMetadata {
            content_type: object.content_type,
            size_bytes: u64::try_from(object.bytes.len())
                .map_err(|_| ObjectStoreError::ObjectTooLarge)?,
            sha256: format!("{:x}", Sha256::digest(&object.bytes)),
            created_at: Utc::now(),
            attributes: object.attributes,
        };
        self.objects.write().await.insert(
            object.key.clone(),
            StoredObject {
                key: object.key,
                bytes: object.bytes,
                metadata: metadata.clone(),
            },
        );
        Ok(metadata)
    }

    async fn get(&self, key: &ObjectKey) -> Result<Option<StoredObject>, ObjectStoreError> {
        Ok(self.objects.read().await.get(key).cloned())
    }

    async fn delete(&self, key: &ObjectKey) -> Result<bool, ObjectStoreError> {
        Ok(self.objects.write().await.remove(key).is_some())
    }
}

#[derive(Debug, Clone)]
pub struct ObjectStoragePlugin {
    store: ObjectStoreService,
    access: Option<ObjectAccessService>,
}

impl ObjectStoragePlugin {
    pub fn new(store: Arc<dyn ObjectStore>) -> Self {
        Self {
            store: ObjectStoreService::new(store),
            access: None,
        }
    }

    pub fn memory() -> Self {
        Self::new(Arc::new(MemoryObjectStore::default()))
    }

    #[must_use]
    pub fn with_access_signer(mut self, signer: Arc<dyn ObjectAccessSigner>) -> Self {
        self.access = Some(ObjectAccessService::new(signer));
        self
    }
}

impl Plugin for ObjectStoragePlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("object-storage").expect("static plugin ID"),
            Version::new(1, 0, 0),
            "Provider-neutral object storage used by uploads, exports, and feedback attachments",
        );
        descriptor.documentation = Some("https://docs.rs/minco-plugin-object-storage".into());
        descriptor.core_compatibility =
            VersionReq::parse(concat!("^", env!("CARGO_PKG_VERSION"))).expect("package version");
        descriptor.stability = PluginStability::Beta;
        descriptor
            .data_classes
            .extend([DataClass::CustomerProvided, DataClass::Confidential]);
        descriptor.provides.push(CapabilityProvision {
            name: "storage.object".into(),
            version: Version::new(1, 0, 0),
        });
        if self.access.is_some() {
            descriptor.provides.push(CapabilityProvision {
                name: "storage.object.presign".into(),
                version: Version::new(1, 0, 0),
            });
        }
        descriptor
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        context.services().insert(Arc::new(self.store.clone()))?;
        if let Some(access) = &self.access {
            context.services().insert(Arc::new(access.clone()))?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ObjectStoreError {
    #[error("invalid object key: {0}")]
    InvalidKey(String),
    #[error("content type must not be empty")]
    InvalidContentType,
    #[error("maximum object size must be greater than zero")]
    InvalidMaximumSize,
    #[error("presigned request expiry must be greater than zero and no more than 24 hours")]
    InvalidExpiry,
    #[error("object is too large for this platform")]
    ObjectTooLarge,
    #[error("object store failed: {0}")]
    Store(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_core::{PluginManager, PluginSelection};

    #[tokio::test]
    async fn memory_store_round_trips_bytes_and_metadata() {
        let store = MemoryObjectStore::default();
        let key = ObjectKey::parse("feedback/one/screenshot.png").unwrap();
        let metadata = store
            .put(PutObject {
                key: key.clone(),
                bytes: b"png".to_vec(),
                content_type: "image/png".into(),
                attributes: BTreeMap::new(),
            })
            .await
            .unwrap();
        assert_eq!(metadata.size_bytes, 3);
        assert_eq!(store.get(&key).await.unwrap().unwrap().bytes, b"png");
        assert!(store.delete(&key).await.unwrap());
        assert!(store.get(&key).await.unwrap().is_none());
    }

    #[test]
    fn unsafe_or_ambiguous_keys_are_rejected() {
        for key in ["", "/absolute", "folder/", "a//b", "a/../b"] {
            assert!(ObjectKey::parse(key).is_err(), "{key}");
        }
    }

    #[test]
    fn presigned_request_debug_redacts_capability_values() {
        let request = PresignedObjectRequest {
            method: PresignedMethod::Post,
            url: "https://objects.example/key?X-Amz-Signature=secret-signature".into(),
            headers: BTreeMap::from([("authorization".into(), "secret-header".into())]),
            form_fields: BTreeMap::from([
                ("x-amz-security-token".into(), "secret-token".into()),
                ("x-amz-signature".into(), "secret-signature".into()),
            ]),
            expires_at: Utc::now() + TimeDelta::minutes(5),
        };
        let debug = format!("{request:?}");
        assert!(!debug.contains("secret-token"));
        assert!(!debug.contains("secret-signature"));
        assert!(!debug.contains("secret-header"));
        assert!(debug.contains("x-amz-security-token"));
    }

    #[derive(Debug)]
    struct TestSigner;

    #[async_trait]
    impl ObjectAccessSigner for TestSigner {
        async fn sign_put(
            &self,
            request: PresignPutObject,
        ) -> Result<PresignedObjectRequest, ObjectStoreError> {
            Ok(PresignedObjectRequest {
                method: PresignedMethod::Put,
                url: format!("https://objects.example/{}", request.key.as_str()),
                headers: BTreeMap::from([("content-type".into(), request.content_type)]),
                form_fields: BTreeMap::new(),
                expires_at: Utc::now() + request.expires_in,
            })
        }

        async fn sign_get(
            &self,
            request: PresignGetObject,
        ) -> Result<PresignedObjectRequest, ObjectStoreError> {
            Ok(PresignedObjectRequest {
                method: PresignedMethod::Get,
                url: format!("https://objects.example/{}", request.key.as_str()),
                headers: BTreeMap::new(),
                form_fields: BTreeMap::new(),
                expires_at: Utc::now() + request.expires_in,
            })
        }
    }

    #[tokio::test]
    async fn optional_presigning_is_typed_and_advertised_only_when_configured() {
        let mut manager = PluginManager::default();
        manager
            .register(ObjectStoragePlugin::memory().with_access_signer(Arc::new(TestSigner)))
            .unwrap();
        let id = PluginId::new("object-storage").unwrap();
        let mut selection = PluginSelection::default();
        selection.enabled.insert(id);
        let application = manager.compose(&selection).unwrap();
        assert!(
            application
                .graph
                .capabilities
                .contains_key("storage.object.presign")
        );

        let access = application.services.get::<ObjectAccessService>().unwrap();
        let signed = access
            .sign_get(PresignGetObject {
                key: ObjectKey::parse("documents/report.pdf").unwrap(),
                expires_in: TimeDelta::minutes(5),
                download_file_name: Some("report.pdf".into()),
            })
            .await
            .unwrap();
        assert_eq!(signed.method, PresignedMethod::Get);
    }
}
