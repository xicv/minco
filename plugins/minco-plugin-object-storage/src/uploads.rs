use crate::{
    MemoryObjectStore, MultipartObjectService, ObjectAccessSigner, ObjectDownloadService,
    ObjectKey, ObjectMetadata, ObjectReadService, ObjectStoragePlugin, ObjectStore,
    ObjectStoreError, PresignedObjectRequest,
};
use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use minco_core::{CapabilityProvision, Plugin, PluginContext, PluginDescriptor, PluginError};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use uuid::Uuid;

#[cfg(feature = "http")]
use crate::{ObjectTransferHttpService, object_transfer_http_module, object_transfer_operations};

const MAX_UPLOAD_EXPIRY_SECONDS: i64 = 24 * 60 * 60;
const DEFAULT_UPLOAD_EXPIRY_SECONDS: i64 = 15 * 60;
const UPLOAD_ID_ATTRIBUTE: &str = "minco.upload_id";
const RESERVED_ATTRIBUTE_PREFIX: &str = "minco.";

/// Metadata returned without loading an object's bytes into application memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectHead {
    pub key: ObjectKey,
    pub content_type: String,
    pub size_bytes: u64,
    /// SHA-256 is optional because a provider can report metadata for legacy
    /// objects that were not uploaded with a provider checksum.
    pub sha256: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
}

impl ObjectHead {
    fn from_metadata(key: ObjectKey, metadata: ObjectMetadata) -> Self {
        Self {
            key,
            content_type: metadata.content_type,
            size_bytes: metadata.size_bytes,
            sha256: Some(metadata.sha256),
            created_at: metadata.created_at,
            attributes: metadata.attributes,
        }
    }
}

/// Provider port for an inexpensive metadata lookup such as S3 `HeadObject`.
#[async_trait]
pub trait ObjectMetadataReader: Send + Sync + std::fmt::Debug {
    async fn head(&self, key: &ObjectKey) -> Result<Option<ObjectHead>, ObjectStoreError>;
}

#[async_trait]
impl ObjectMetadataReader for MemoryObjectStore {
    async fn head(&self, key: &ObjectKey) -> Result<Option<ObjectHead>, ObjectStoreError> {
        ObjectStore::get(self, key).await.map(|object| {
            object.map(|stored| ObjectHead::from_metadata(stored.key, stored.metadata))
        })
    }
}

/// Typed metadata service injected by a managed object-storage plugin.
#[derive(Clone)]
pub struct ObjectMetadataService(Arc<dyn ObjectMetadataReader>);

impl std::fmt::Debug for ObjectMetadataService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("ObjectMetadataService").finish()
    }
}

impl ObjectMetadataService {
    pub fn new(reader: Arc<dyn ObjectMetadataReader>) -> Self {
        Self(reader)
    }

    pub async fn head(&self, key: &ObjectKey) -> Result<Option<ObjectHead>, ObjectStoreError> {
        self.0.head(key).await
    }
}

/// Exact provider request for one checksummed direct upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignObjectUpload {
    pub key: ObjectKey,
    pub content_type: String,
    pub size_bytes: u64,
    /// Lowercase hexadecimal SHA-256 of the complete object body.
    pub sha256: String,
    pub expires_in: TimeDelta,
    pub attributes: BTreeMap<String, String>,
}

/// Provider adapter that can bind an upload capability to exact bytes.
///
/// This is separate from [`ObjectAccessSigner`] so the compatibility-preserving
/// `PresignPutObject` contract can remain available while managed uploads require
/// an exact size and SHA-256 checksum.
#[async_trait]
pub trait ObjectUploadSigner: Send + Sync + std::fmt::Debug {
    async fn sign_upload(
        &self,
        request: SignObjectUpload,
    ) -> Result<PresignedObjectRequest, ObjectUploadError>;
}

/// Closed upload policy owned by the application composition root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectUploadPolicy {
    key_prefix: ObjectKey,
    allowed_content_types: BTreeSet<String>,
    maximum_size_bytes: u64,
    expires_in: TimeDelta,
}

impl ObjectUploadPolicy {
    pub fn new<I, S>(
        key_prefix: ObjectKey,
        maximum_size_bytes: u64,
        allowed_content_types: I,
    ) -> Result<Self, ObjectUploadError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        if maximum_size_bytes == 0 {
            return Err(ObjectUploadError::InvalidMaximumSize);
        }
        let allowed_content_types = allowed_content_types
            .into_iter()
            .map(|value| normalize_content_type(value.as_ref()))
            .collect::<Result<BTreeSet<_>, _>>()?;
        if allowed_content_types.is_empty() {
            return Err(ObjectUploadError::EmptyContentTypeAllowlist);
        }
        Ok(Self {
            key_prefix,
            allowed_content_types,
            maximum_size_bytes,
            expires_in: TimeDelta::seconds(DEFAULT_UPLOAD_EXPIRY_SECONDS),
        })
    }

    pub fn with_expiry(mut self, expires_in: TimeDelta) -> Result<Self, ObjectUploadError> {
        validate_expiry(expires_in)?;
        self.expires_in = expires_in;
        Ok(self)
    }

    pub const fn key_prefix(&self) -> &ObjectKey {
        &self.key_prefix
    }

    pub const fn maximum_size_bytes(&self) -> u64 {
        self.maximum_size_bytes
    }

    pub const fn expires_in(&self) -> TimeDelta {
        self.expires_in
    }

    pub fn allows_content_type(&self, content_type: &str) -> bool {
        normalize_content_type(content_type)
            .is_ok_and(|value| self.allowed_content_types.contains(&value))
    }
}

/// Application-authorized request for one direct object upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueObjectUpload {
    pub content_type: String,
    pub size_bytes: u64,
    /// Lowercase or uppercase hexadecimal SHA-256 of the complete file.
    pub sha256: String,
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
}

/// Client-facing bearer capability. Send this value to the authorized client,
/// but never persist or log it as the trusted upload record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectUploadGrant {
    pub key: ObjectKey,
    pub request: PresignedObjectRequest,
}

/// Trusted server-side record for one issued upload capability.
///
/// Persist this value in application-owned state. Do not accept a replacement
/// from an untrusted client as authorization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingObjectUpload {
    pub key: ObjectKey,
    pub expected_content_type: String,
    pub expected_size_bytes: u64,
    pub expected_sha256: String,
    pub expected_attributes: BTreeMap<String, String>,
    /// Expiry of the bearer upload capability, not of the pending record.
    ///
    /// Verification may happen after this timestamp when the provider accepted
    /// the upload before the capability expired. Pending-record retention and
    /// cleanup remain application-owned.
    pub capability_expires_at: DateTime<Utc>,
}

/// Split result that prevents the bearer request from becoming trusted state by
/// accident. Return only [`Self::grant`] to the client and retain
/// [`Self::pending`] on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedObjectUpload {
    pub grant: ObjectUploadGrant,
    pub pending: PendingObjectUpload,
}

/// Metadata accepted after the provider confirms the issued upload contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedObjectUpload {
    pub key: ObjectKey,
    pub metadata: ObjectHead,
}

/// Issues unique direct-upload capabilities and verifies their provider metadata.
#[derive(Clone)]
pub struct ObjectUploadService {
    signer: Arc<dyn ObjectUploadSigner>,
    metadata: ObjectMetadataService,
    policy: ObjectUploadPolicy,
}

impl std::fmt::Debug for ObjectUploadService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ObjectUploadService")
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl ObjectUploadService {
    pub const fn new(
        signer: Arc<dyn ObjectUploadSigner>,
        metadata: ObjectMetadataService,
        policy: ObjectUploadPolicy,
    ) -> Self {
        Self {
            signer,
            metadata,
            policy,
        }
    }

    pub async fn issue(
        &self,
        request: IssueObjectUpload,
    ) -> Result<IssuedObjectUpload, ObjectUploadError> {
        let IssueObjectUpload {
            content_type,
            size_bytes,
            sha256,
            attributes,
        } = request;
        let content_type = normalize_content_type(&content_type)?;
        if !self.policy.allowed_content_types.contains(&content_type) {
            return Err(ObjectUploadError::UnsupportedContentType(content_type));
        }
        if size_bytes == 0 {
            return Err(ObjectUploadError::EmptyObject);
        }
        if size_bytes > self.policy.maximum_size_bytes {
            return Err(ObjectUploadError::ObjectTooLarge {
                actual: size_bytes,
                maximum: self.policy.maximum_size_bytes,
            });
        }
        let sha256 = normalize_sha256(&sha256)?;
        let mut attributes = validate_attributes(attributes)?;
        let upload_id = Uuid::now_v7().to_string();
        attributes.insert(UPLOAD_ID_ATTRIBUTE.to_owned(), upload_id.clone());
        let key = generated_key(&self.policy.key_prefix, &upload_id)?;
        let signed = self
            .signer
            .sign_upload(SignObjectUpload {
                key: key.clone(),
                content_type: content_type.clone(),
                size_bytes,
                sha256: sha256.clone(),
                expires_in: self.policy.expires_in,
                attributes: attributes.clone(),
            })
            .await?;
        let capability_expires_at = signed.expires_at;
        Ok(IssuedObjectUpload {
            grant: ObjectUploadGrant {
                key: key.clone(),
                request: signed,
            },
            pending: PendingObjectUpload {
                key,
                expected_content_type: content_type,
                expected_size_bytes: size_bytes,
                expected_sha256: sha256,
                expected_attributes: attributes,
                capability_expires_at,
            },
        })
    }

    pub async fn verify(
        &self,
        pending: &PendingObjectUpload,
    ) -> Result<VerifiedObjectUpload, ObjectUploadError> {
        let Some(metadata) = self.metadata.head(&pending.key).await? else {
            return Err(ObjectUploadError::MissingObject);
        };
        if metadata.key != pending.key {
            return Err(ObjectUploadError::ObjectKeyMismatch);
        }
        let actual_content_type = normalize_content_type(&metadata.content_type)
            .map_err(|_| ObjectUploadError::ContentTypeMismatch)?;
        if actual_content_type != pending.expected_content_type {
            return Err(ObjectUploadError::ContentTypeMismatch);
        }
        if metadata.size_bytes != pending.expected_size_bytes {
            return Err(ObjectUploadError::ObjectSizeMismatch {
                actual: metadata.size_bytes,
                expected: pending.expected_size_bytes,
            });
        }
        let actual_sha256 = metadata
            .sha256
            .as_deref()
            .map(normalize_sha256)
            .transpose()
            .map_err(|_| ObjectUploadError::ChecksumMismatch)?;
        if actual_sha256.as_deref() != Some(pending.expected_sha256.as_str()) {
            return Err(ObjectUploadError::ChecksumMismatch);
        }
        if metadata.attributes != pending.expected_attributes {
            return Err(ObjectUploadError::AttributeMismatch);
        }
        Ok(VerifiedObjectUpload {
            key: pending.key.clone(),
            metadata,
        })
    }
}

/// Object-storage plugin with the direct-upload and metadata lifecycle installed.
#[derive(Debug, Clone)]
pub struct ManagedObjectStoragePlugin {
    storage: ObjectStoragePlugin,
    metadata: ObjectMetadataService,
    uploads: ObjectUploadService,
    transfers: Option<ManagedObjectTransferServices>,
    #[cfg(feature = "http")]
    http: Option<ObjectTransferHttpService>,
}

/// Statically selected large-transfer services installed as one coherent
/// provider profile. The application still owns authorization and session
/// persistence.
#[derive(Debug, Clone)]
pub struct ManagedObjectTransferServices {
    pub reads: ObjectReadService,
    pub downloads: ObjectDownloadService,
    pub multipart: MultipartObjectService,
}

impl ManagedObjectTransferServices {
    pub const fn new(
        reads: ObjectReadService,
        downloads: ObjectDownloadService,
        multipart: MultipartObjectService,
    ) -> Self {
        Self {
            reads,
            downloads,
            multipart,
        }
    }
}

impl ManagedObjectStoragePlugin {
    pub fn new<S>(
        store: Arc<dyn ObjectStore>,
        signer: Arc<S>,
        metadata_reader: Arc<dyn ObjectMetadataReader>,
        policy: ObjectUploadPolicy,
    ) -> Self
    where
        S: ObjectAccessSigner + ObjectUploadSigner + 'static,
    {
        let access_signer: Arc<dyn ObjectAccessSigner> = signer.clone();
        let upload_signer: Arc<dyn ObjectUploadSigner> = signer;
        Self::new_with_signers(store, access_signer, upload_signer, metadata_reader, policy)
    }

    /// Construct managed storage with independent private-download and upload
    /// signers while retaining static, typed composition.
    pub fn new_with_signers(
        store: Arc<dyn ObjectStore>,
        access_signer: Arc<dyn ObjectAccessSigner>,
        upload_signer: Arc<dyn ObjectUploadSigner>,
        metadata_reader: Arc<dyn ObjectMetadataReader>,
        policy: ObjectUploadPolicy,
    ) -> Self {
        let metadata = ObjectMetadataService::new(metadata_reader);
        let uploads = ObjectUploadService::new(upload_signer, metadata.clone(), policy);
        Self {
            storage: ObjectStoragePlugin::new(store).with_access_signer(access_signer),
            metadata,
            uploads,
            transfers: None,
            #[cfg(feature = "http")]
            http: None,
        }
    }

    /// Install range streaming, private download grants, and multipart upload
    /// through the same explicit provider composition.
    #[must_use]
    pub fn with_transfer_services(mut self, transfers: ManagedObjectTransferServices) -> Self {
        self.transfers = Some(transfers);
        self
    }

    /// Contribute the authenticated HTTP control plane. The injected use-case
    /// service owns authorization and durable upload/object state.
    #[cfg(feature = "http")]
    #[must_use]
    pub fn with_http_api(mut self, http: ObjectTransferHttpService) -> Self {
        self.http = Some(http);
        self
    }
}

impl Plugin for ManagedObjectStoragePlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = self.storage.descriptor();
        descriptor.provides.extend([
            CapabilityProvision {
                name: "storage.object.metadata".into(),
                version: Version::new(1, 0, 0),
            },
            CapabilityProvision {
                name: "storage.object.upload".into(),
                version: Version::new(1, 0, 0),
            },
        ]);
        if self.transfers.is_some() {
            descriptor.provides.extend([
                CapabilityProvision {
                    name: "storage.object.stream".into(),
                    version: Version::new(1, 0, 0),
                },
                CapabilityProvision {
                    name: "storage.object.download".into(),
                    version: Version::new(1, 0, 0),
                },
                CapabilityProvision {
                    name: "storage.object.multipart".into(),
                    version: Version::new(1, 0, 0),
                },
            ]);
        }
        #[cfg(feature = "http")]
        if self.http.is_some() {
            descriptor.provides.push(CapabilityProvision {
                name: "storage.object.http".into(),
                version: Version::new(1, 0, 0),
            });
            descriptor.operations.extend(object_transfer_operations());
        }
        descriptor
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        self.storage.install(context)?;
        context.services().insert(Arc::new(self.metadata.clone()))?;
        context.services().insert(Arc::new(self.uploads.clone()))?;
        if let Some(transfers) = &self.transfers {
            context
                .services()
                .insert(Arc::new(transfers.reads.clone()))?;
            context
                .services()
                .insert(Arc::new(transfers.downloads.clone()))?;
            context
                .services()
                .insert(Arc::new(transfers.multipart.clone()))?;
        }
        #[cfg(feature = "http")]
        if let Some(http) = &self.http {
            object_transfer_http_module(context.plugin_id().clone(), http.clone())
                .contribute(context);
        }
        Ok(())
    }
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum ObjectUploadError {
    #[error("upload policy maximum size must be greater than zero")]
    InvalidMaximumSize,
    #[error("upload policy must allow at least one exact content type")]
    EmptyContentTypeAllowlist,
    #[error("upload content type is invalid")]
    InvalidContentType,
    #[error("upload content type is not allowed: {0}")]
    UnsupportedContentType(String),
    #[error("upload body must not be empty")]
    EmptyObject,
    #[error("upload SHA-256 must be exactly 64 hexadecimal characters")]
    InvalidSha256,
    #[error("upload attributes are invalid or use a reserved Minco key")]
    InvalidAttributes,
    #[error("upload expiry must be greater than zero and no more than 24 hours")]
    InvalidExpiry,
    #[error("the uploaded object does not exist")]
    MissingObject,
    #[error("the provider reported metadata for a different object key")]
    ObjectKeyMismatch,
    #[error("the uploaded object's content type does not match the issued capability")]
    ContentTypeMismatch,
    #[error("the uploaded object's signed attributes do not match the issued capability")]
    AttributeMismatch,
    #[error("the uploaded object's SHA-256 does not match the issued capability")]
    ChecksumMismatch,
    #[error("the requested upload is {actual} bytes; the policy maximum is {maximum} bytes")]
    ObjectTooLarge { actual: u64, maximum: u64 },
    #[error("the uploaded object is {actual} bytes; the issued size is {expected} bytes")]
    ObjectSizeMismatch { actual: u64, expected: u64 },
    #[error("the provider endpoint for the upload capability could not be resolved")]
    EndpointResolution,
    #[error("the signing credentials have an invalid expiration time")]
    InvalidCredentialExpiry,
    #[error("the signing credentials expire too soon to issue an upload capability")]
    CredentialLifetimeTooShort,
    #[error(transparent)]
    ObjectStore(#[from] ObjectStoreError),
}

fn validate_expiry(expires_in: TimeDelta) -> Result<(), ObjectUploadError> {
    if expires_in <= TimeDelta::zero() || expires_in > TimeDelta::seconds(MAX_UPLOAD_EXPIRY_SECONDS)
    {
        Err(ObjectUploadError::InvalidExpiry)
    } else {
        Ok(())
    }
}

fn normalize_content_type(value: &str) -> Result<String, ObjectUploadError> {
    let value = value.trim().to_ascii_lowercase();
    let Some((top_level, subtype)) = value.split_once('/') else {
        return Err(ObjectUploadError::InvalidContentType);
    };
    if value.len() > 255
        || subtype.contains('/')
        || !valid_media_token(top_level)
        || !valid_media_token(subtype)
    {
        return Err(ObjectUploadError::InvalidContentType);
    }
    Ok(value)
}

fn valid_media_token(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-'
                )
        })
}

fn normalize_sha256(value: &str) -> Result<String, ObjectUploadError> {
    let value = value.trim();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ObjectUploadError::InvalidSha256);
    }
    Ok(value.to_ascii_lowercase())
}

fn validate_attributes(
    attributes: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, ObjectUploadError> {
    if attributes.len() > 31
        || attributes.iter().any(|(key, value)| {
            key.trim().is_empty()
                || key.starts_with(RESERVED_ATTRIBUTE_PREFIX)
                || key.len() > 128
                || value.len() > 1024
                || key.chars().any(char::is_control)
                || value.chars().any(char::is_control)
        })
    {
        return Err(ObjectUploadError::InvalidAttributes);
    }
    Ok(attributes)
}

fn generated_key(prefix: &ObjectKey, upload_id: &str) -> Result<ObjectKey, ObjectUploadError> {
    ObjectKey::parse(format!("{}/{upload_id}", prefix.as_str())).map_err(ObjectUploadError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PresignGetObject, PresignPutObject, PresignedMethod, PutObject};
    use minco_core::{PluginId, PluginManager, PluginSelection};
    use sha2::{Digest, Sha256};

    #[derive(Debug)]
    struct TestSigner;

    #[async_trait]
    impl ObjectAccessSigner for TestSigner {
        async fn sign_put(
            &self,
            request: PresignPutObject,
        ) -> Result<PresignedObjectRequest, ObjectStoreError> {
            Ok(PresignedObjectRequest {
                method: PresignedMethod::Post,
                url: "https://objects.example/upload".into(),
                headers: BTreeMap::new(),
                form_fields: BTreeMap::from([
                    ("key".into(), request.key.as_str().to_owned()),
                    ("content-type".into(), request.content_type),
                    (
                        "maximum-size".into(),
                        request.maximum_size_bytes.to_string(),
                    ),
                ]),
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

    #[async_trait]
    impl ObjectUploadSigner for TestSigner {
        async fn sign_upload(
            &self,
            request: SignObjectUpload,
        ) -> Result<PresignedObjectRequest, ObjectUploadError> {
            Ok(PresignedObjectRequest {
                method: PresignedMethod::Post,
                url: "https://objects.example/upload".into(),
                headers: BTreeMap::new(),
                form_fields: BTreeMap::from([
                    ("key".into(), request.key.as_str().to_owned()),
                    ("content-type".into(), request.content_type),
                    ("size-bytes".into(), request.size_bytes.to_string()),
                    ("sha256".into(), request.sha256),
                ]),
                expires_at: Utc::now() + request.expires_in,
            })
        }
    }

    fn sha256(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    fn policy() -> ObjectUploadPolicy {
        ObjectUploadPolicy::new(
            ObjectKey::parse("uploads/images").unwrap(),
            1_024,
            ["image/png", "image/jpeg"],
        )
        .unwrap()
    }

    fn service(store: Arc<MemoryObjectStore>) -> ObjectUploadService {
        let metadata: Arc<dyn ObjectMetadataReader> = store;
        ObjectUploadService::new(
            Arc::new(TestSigner),
            ObjectMetadataService::new(metadata),
            policy(),
        )
    }

    fn request() -> IssueObjectUpload {
        IssueObjectUpload {
            content_type: "IMAGE/PNG".into(),
            size_bytes: 3,
            sha256: sha256(b"png"),
            attributes: BTreeMap::from([("tenant".into(), "acme".into())]),
        }
    }

    #[test]
    fn policy_is_closed_and_bounded() {
        assert!(
            ObjectUploadPolicy::new(ObjectKey::parse("uploads").unwrap(), 0, ["image/png"])
                .is_err()
        );
        assert!(
            ObjectUploadPolicy::new(
                ObjectKey::parse("uploads").unwrap(),
                1,
                std::iter::empty::<&str>()
            )
            .is_err()
        );
        assert!(policy().with_expiry(TimeDelta::hours(25)).is_err());
        assert!(policy().allows_content_type("IMAGE/PNG"));
        assert!(!policy().allows_content_type("text/html"));
    }

    #[tokio::test]
    async fn issuing_an_upload_separates_the_bearer_grant_from_trusted_state() {
        let service = service(Arc::new(MemoryObjectStore::default()));
        let first = service.issue(request()).await.unwrap();
        let second = service.issue(request()).await.unwrap();
        assert_ne!(first.grant.key, second.grant.key);
        assert_eq!(first.grant.key, first.pending.key);
        assert!(first.grant.key.as_str().starts_with("uploads/images/"));
        assert!(
            std::path::Path::new(first.grant.key.as_str())
                .extension()
                .is_none()
        );
        assert_eq!(first.pending.expected_content_type, "image/png");
        assert_eq!(first.pending.expected_size_bytes, 3);
        assert_eq!(first.pending.expected_sha256, sha256(b"png"));
        assert!(
            first
                .pending
                .expected_attributes
                .contains_key(UPLOAD_ID_ATTRIBUTE)
        );
        assert_eq!(
            first
                .grant
                .request
                .form_fields
                .get("size-bytes")
                .map(String::as_str),
            Some("3")
        );
        assert_eq!(
            first.grant.request.expires_at,
            first.pending.capability_expires_at
        );
    }

    #[tokio::test]
    async fn issuance_rejects_empty_oversized_or_unchecksummed_objects() {
        let service = service(Arc::new(MemoryObjectStore::default()));

        let mut empty = request();
        empty.size_bytes = 0;
        assert!(matches!(
            service.issue(empty).await,
            Err(ObjectUploadError::EmptyObject)
        ));

        let mut oversized = request();
        oversized.size_bytes = 1_025;
        assert!(matches!(
            service.issue(oversized).await,
            Err(ObjectUploadError::ObjectTooLarge {
                actual: 1_025,
                maximum: 1_024
            })
        ));

        let mut invalid_checksum = request();
        invalid_checksum.sha256 = "not-a-sha256".into();
        assert!(matches!(
            service.issue(invalid_checksum).await,
            Err(ObjectUploadError::InvalidSha256)
        ));
    }

    #[tokio::test]
    async fn verification_accepts_only_the_issued_metadata_contract() {
        let store = Arc::new(MemoryObjectStore::default());
        let service = service(Arc::clone(&store));
        let issued = service.issue(request()).await.unwrap();
        ObjectStore::put(
            store.as_ref(),
            PutObject {
                key: issued.pending.key.clone(),
                bytes: b"png".to_vec(),
                content_type: issued.pending.expected_content_type.clone(),
                attributes: issued.pending.expected_attributes.clone(),
            },
        )
        .await
        .unwrap();
        let verified = service.verify(&issued.pending).await.unwrap();
        assert_eq!(verified.metadata.size_bytes, 3);
        assert_eq!(
            verified.metadata.sha256.as_deref(),
            Some(sha256(b"png").as_str())
        );

        let mut wrong_size = issued.pending.clone();
        wrong_size.expected_size_bytes = 4;
        assert!(matches!(
            service.verify(&wrong_size).await,
            Err(ObjectUploadError::ObjectSizeMismatch {
                actual: 3,
                expected: 4
            })
        ));

        let mut wrong_checksum = issued.pending.clone();
        wrong_checksum.expected_sha256 = sha256(b"jpg");
        assert!(matches!(
            service.verify(&wrong_checksum).await,
            Err(ObjectUploadError::ChecksumMismatch)
        ));

        let mut wrong_attributes = issued.pending;
        wrong_attributes.expected_attributes.clear();
        assert!(matches!(
            service.verify(&wrong_attributes).await,
            Err(ObjectUploadError::AttributeMismatch)
        ));
    }

    #[derive(Debug)]
    struct WrongKeyMetadataReader;

    #[async_trait]
    impl ObjectMetadataReader for WrongKeyMetadataReader {
        async fn head(&self, _key: &ObjectKey) -> Result<Option<ObjectHead>, ObjectStoreError> {
            Ok(Some(ObjectHead {
                key: ObjectKey::parse("uploads/images/different").unwrap(),
                content_type: "image/png".into(),
                size_bytes: 3,
                sha256: Some(sha256(b"png")),
                created_at: Utc::now(),
                attributes: BTreeMap::new(),
            }))
        }
    }

    #[tokio::test]
    async fn verification_rejects_metadata_for_a_different_logical_key() {
        let service = ObjectUploadService::new(
            Arc::new(TestSigner),
            ObjectMetadataService::new(Arc::new(WrongKeyMetadataReader)),
            policy(),
        );
        let issued = service.issue(request()).await.unwrap();
        assert!(matches!(
            service.verify(&issued.pending).await,
            Err(ObjectUploadError::ObjectKeyMismatch)
        ));
    }

    #[test]
    fn managed_plugin_advertises_and_installs_the_lifecycle() {
        let store = Arc::new(MemoryObjectStore::default());
        let object_store: Arc<dyn ObjectStore> = store.clone();
        let metadata: Arc<dyn ObjectMetadataReader> = store;
        let mut manager = PluginManager::default();
        manager
            .register(ManagedObjectStoragePlugin::new_with_signers(
                object_store,
                Arc::new(TestSigner),
                Arc::new(TestSigner),
                metadata,
                policy(),
            ))
            .unwrap();
        let id = PluginId::new("object-storage").unwrap();
        let mut selection = PluginSelection::default();
        selection.enabled.insert(id);
        let application = manager.compose(&selection).unwrap();
        assert!(
            application
                .graph
                .capabilities
                .contains_key("storage.object.upload")
        );
        assert!(application.services.get::<ObjectUploadService>().is_ok());
        assert!(application.services.get::<ObjectMetadataService>().is_ok());
    }
}
