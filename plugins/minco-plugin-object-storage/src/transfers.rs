//! Streaming reads, resumable multipart uploads, validation state, and cost shape.
#![forbid(unsafe_code)]

use crate::{ObjectKey, ObjectStore, ObjectStoreError, PresignedObjectRequest, StoredObject};
use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use futures::{Stream, stream};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    pin::Pin,
    sync::Arc,
};
use uuid::Uuid;

pub const MIN_MULTIPART_PART_SIZE_BYTES: u64 = 5 * 1024 * 1024;
pub const MAX_MULTIPART_PART_SIZE_BYTES: u64 = 5 * 1024 * 1024 * 1024;
pub const MAX_MULTIPART_PARTS: u32 = 10_000;
pub const MAX_MULTIPART_OBJECT_SIZE_BYTES: u64 =
    MAX_MULTIPART_PART_SIZE_BYTES * MAX_MULTIPART_PARTS as u64;
const MAX_CAPABILITY_EXPIRY_SECONDS: i64 = 24 * 60 * 60;
const DEFAULT_PART_EXPIRY_SECONDS: i64 = 15 * 60;
const RESERVED_ATTRIBUTE_PREFIX: &str = "minco.";
const UPLOAD_ID_ATTRIBUTE: &str = "minco.upload_id";

/// One HTTP byte range. Bounded ranges use an exclusive end in Rust and are
/// rendered with the inclusive end required by HTTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObjectByteRange {
    Bounded { start: u64, end_exclusive: u64 },
    From { start: u64 },
    Suffix { length: u64 },
}

impl ObjectByteRange {
    pub const fn bounded(start: u64, end_exclusive: u64) -> Result<Self, ObjectTransferError> {
        if start >= end_exclusive {
            Err(ObjectTransferError::InvalidRange)
        } else {
            Ok(Self::Bounded {
                start,
                end_exclusive,
            })
        }
    }

    pub const fn from(start: u64) -> Self {
        Self::From { start }
    }

    pub const fn suffix(length: u64) -> Result<Self, ObjectTransferError> {
        if length == 0 {
            Err(ObjectTransferError::InvalidRange)
        } else {
            Ok(Self::Suffix { length })
        }
    }

    #[must_use]
    pub fn to_http_value(self) -> String {
        match self {
            Self::Bounded {
                start,
                end_exclusive,
            } => format!("bytes={start}-{}", end_exclusive - 1),
            Self::From { start } => format!("bytes={start}-"),
            Self::Suffix { length } => format!("bytes=-{length}"),
        }
    }

    pub const fn validate(self) -> Result<(), ObjectTransferError> {
        match self {
            Self::Bounded {
                start,
                end_exclusive,
            } if start < end_exclusive => Ok(()),
            Self::From { .. } => Ok(()),
            Self::Suffix { length } if length > 0 => Ok(()),
            _ => Err(ObjectTransferError::InvalidRange),
        }
    }

    fn resolve(self, size_bytes: u64) -> Result<(u64, u64), ObjectTransferError> {
        if size_bytes == 0 {
            return Err(ObjectTransferError::RangeNotSatisfiable);
        }
        match self {
            Self::Bounded {
                start,
                end_exclusive,
            } if start < end_exclusive && start < size_bytes => {
                Ok((start, end_exclusive.min(size_bytes)))
            }
            Self::From { start } if start < size_bytes => Ok((start, size_bytes)),
            Self::Suffix { length } if length > 0 => {
                Ok((size_bytes.saturating_sub(length), size_bytes))
            }
            _ => Err(ObjectTransferError::RangeNotSatisfiable),
        }
    }
}

/// Metadata required for conditional and resumable reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectReadHead {
    pub key: ObjectKey,
    pub content_type: String,
    pub size_bytes: u64,
    pub entity_tag: String,
    pub version_id: Option<String>,
    pub last_modified: DateTime<Utc>,
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectReadRequest {
    pub key: ObjectKey,
    pub range: Option<ObjectByteRange>,
    pub expected_entity_tag: Option<String>,
    pub version_id: Option<String>,
}

pub type ObjectByteStream =
    Pin<Box<dyn Stream<Item = Result<Vec<u8>, ObjectStoreError>> + Send + 'static>>;

pub struct ObjectReadResponse {
    pub head: ObjectReadHead,
    pub content_range: Option<String>,
    pub stream: ObjectByteStream,
}

impl fmt::Debug for ObjectReadResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ObjectReadResponse")
            .field("head", &self.head)
            .field("content_range", &self.content_range)
            .field("stream", &"[STREAM]")
            .finish()
    }
}

/// Provider-neutral read port. Implementations must produce bounded chunks and
/// stop provider reads when the returned stream is dropped.
#[async_trait]
pub trait ObjectStreamReader: Send + Sync + fmt::Debug {
    async fn head(&self, key: &ObjectKey) -> Result<Option<ObjectReadHead>, ObjectStoreError>;
    async fn read(
        &self,
        request: ObjectReadRequest,
    ) -> Result<Option<ObjectReadResponse>, ObjectStoreError>;
}

#[async_trait]
impl ObjectStreamReader for crate::MemoryObjectStore {
    async fn head(&self, key: &ObjectKey) -> Result<Option<ObjectReadHead>, ObjectStoreError> {
        let object = ObjectStore::get(self, key).await?;
        Ok(object.map(|object| read_head(&object)))
    }

    async fn read(
        &self,
        request: ObjectReadRequest,
    ) -> Result<Option<ObjectReadResponse>, ObjectStoreError> {
        let Some(object) = ObjectStore::get(self, &request.key).await? else {
            return Ok(None);
        };
        let head = read_head(&object);
        validate_read_preconditions(&head, &request).map_err(|error| transfer_to_store(&error))?;
        let (bytes, content_range) = match request.range {
            Some(range) => {
                let (start, end) = range
                    .resolve(head.size_bytes)
                    .map_err(|error| transfer_to_store(&error))?;
                let start = usize::try_from(start).map_err(|_| ObjectStoreError::ObjectTooLarge)?;
                let end = usize::try_from(end).map_err(|_| ObjectStoreError::ObjectTooLarge)?;
                (
                    object.bytes[start..end].to_vec(),
                    Some(format!(
                        "bytes {start}-{}/{size}",
                        end - 1,
                        size = head.size_bytes
                    )),
                )
            }
            None => (object.bytes, None),
        };
        Ok(Some(ObjectReadResponse {
            head,
            content_range,
            stream: Box::pin(stream::once(async move { Ok(bytes) })),
        }))
    }
}

fn read_head(object: &StoredObject) -> ObjectReadHead {
    ObjectReadHead {
        key: object.key.clone(),
        content_type: object.metadata.content_type.clone(),
        size_bytes: object.metadata.size_bytes,
        entity_tag: format!("\"{}\"", object.metadata.sha256),
        version_id: None,
        last_modified: object.metadata.created_at,
        attributes: object.metadata.attributes.clone(),
    }
}

#[derive(Clone)]
pub struct ObjectReadService(Arc<dyn ObjectStreamReader>);

impl fmt::Debug for ObjectReadService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("ObjectReadService").finish()
    }
}

impl ObjectReadService {
    pub fn new(reader: Arc<dyn ObjectStreamReader>) -> Self {
        Self(reader)
    }

    pub async fn head(&self, key: &ObjectKey) -> Result<Option<ObjectReadHead>, ObjectStoreError> {
        self.0.head(key).await
    }

    pub async fn read(
        &self,
        request: ObjectReadRequest,
    ) -> Result<Option<ObjectReadResponse>, ObjectStoreError> {
        self.0.read(request).await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DownloadCachePolicy {
    NoStore,
    Private {
        max_age_seconds: u32,
        immutable: bool,
    },
}

impl DownloadCachePolicy {
    #[must_use]
    pub fn to_header_value(self) -> String {
        match self {
            Self::NoStore => "private, no-store".into(),
            Self::Private {
                max_age_seconds,
                immutable: true,
            } => format!("private, max-age={max_age_seconds}, immutable"),
            Self::Private {
                max_age_seconds,
                immutable: false,
            } => format!("private, max-age={max_age_seconds}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectDownloadPolicy {
    expires_in: TimeDelta,
    cache: DownloadCachePolicy,
}

impl ObjectDownloadPolicy {
    pub fn new(
        expires_in: TimeDelta,
        cache: DownloadCachePolicy,
    ) -> Result<Self, ObjectTransferError> {
        validate_expiry(expires_in)?;
        Ok(Self { expires_in, cache })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueObjectDownload {
    pub key: ObjectKey,
    pub range: Option<ObjectByteRange>,
    pub expected_entity_tag: Option<String>,
    pub version_id: Option<String>,
    pub download_file_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignObjectDownload {
    pub key: ObjectKey,
    pub range: Option<ObjectByteRange>,
    pub if_match: String,
    pub version_id: Option<String>,
    pub download_file_name: Option<String>,
    pub cache_control: String,
    pub expires_in: TimeDelta,
}

#[async_trait]
pub trait ObjectDownloadSigner: Send + Sync + fmt::Debug {
    async fn sign_download(
        &self,
        request: SignObjectDownload,
    ) -> Result<PresignedObjectRequest, ObjectTransferError>;
}

/// Client-facing private download capability. Its `Debug` representation uses
/// the presigned request's redacted formatter.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectDownloadGrant {
    pub key: ObjectKey,
    pub request: PresignedObjectRequest,
    pub content_type: String,
    pub size_bytes: u64,
    pub entity_tag: String,
    pub version_id: Option<String>,
    pub last_modified: DateTime<Utc>,
    pub range: Option<ObjectByteRange>,
    pub cache_control: String,
}

impl fmt::Debug for ObjectDownloadGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ObjectDownloadGrant")
            .field("key", &self.key)
            .field("request", &self.request)
            .field("content_type", &self.content_type)
            .field("size_bytes", &self.size_bytes)
            .field("entity_tag", &self.entity_tag)
            .field("version_id", &self.version_id)
            .field("last_modified", &self.last_modified)
            .field("range", &self.range)
            .field("cache_control", &self.cache_control)
            .finish()
    }
}

#[derive(Clone)]
pub struct ObjectDownloadService {
    signer: Arc<dyn ObjectDownloadSigner>,
    reads: ObjectReadService,
    policy: ObjectDownloadPolicy,
}

impl fmt::Debug for ObjectDownloadService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ObjectDownloadService")
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl ObjectDownloadService {
    pub fn new(
        signer: Arc<dyn ObjectDownloadSigner>,
        reads: ObjectReadService,
        policy: ObjectDownloadPolicy,
    ) -> Self {
        Self {
            signer,
            reads,
            policy,
        }
    }

    pub async fn issue(
        &self,
        request: IssueObjectDownload,
    ) -> Result<ObjectDownloadGrant, ObjectTransferError> {
        let head = self
            .reads
            .head(&request.key)
            .await?
            .ok_or(ObjectTransferError::MissingObject)?;
        validate_read_preconditions(
            &head,
            &ObjectReadRequest {
                key: request.key.clone(),
                range: request.range,
                expected_entity_tag: request.expected_entity_tag,
                version_id: request.version_id,
            },
        )?;
        if let Some(range) = request.range {
            range.resolve(head.size_bytes)?;
        }
        let download_file_name = request
            .download_file_name
            .unwrap_or_else(|| "download".into());
        validate_download_name(&download_file_name)?;
        let cache_control = self.policy.cache.to_header_value();
        let signed = self
            .signer
            .sign_download(SignObjectDownload {
                key: request.key.clone(),
                range: request.range,
                if_match: head.entity_tag.clone(),
                version_id: head.version_id.clone(),
                download_file_name: Some(download_file_name),
                cache_control: cache_control.clone(),
                expires_in: self.policy.expires_in,
            })
            .await?;
        Ok(ObjectDownloadGrant {
            key: request.key,
            request: signed,
            content_type: head.content_type,
            size_bytes: head.size_bytes,
            entity_tag: head.entity_tag,
            version_id: head.version_id,
            last_modified: head.last_modified,
            range: request.range,
            cache_control,
        })
    }
}

fn validate_read_preconditions(
    head: &ObjectReadHead,
    request: &ObjectReadRequest,
) -> Result<(), ObjectTransferError> {
    if request
        .expected_entity_tag
        .as_deref()
        .is_some_and(|expected| expected != head.entity_tag)
        || request
            .version_id
            .as_deref()
            .is_some_and(|expected| Some(expected) != head.version_id.as_deref())
    {
        return Err(ObjectTransferError::PreconditionFailed);
    }
    Ok(())
}

/// Opaque trusted provider state. It is serializable for application-owned
/// session persistence but always redacted from `Debug`.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ProviderMultipartUploadId(String);

impl ProviderMultipartUploadId {
    pub fn parse(value: impl Into<String>) -> Result<Self, ObjectTransferError> {
        let value = value.into();
        if value.is_empty() || value.len() > 2_048 || value.chars().any(char::is_control) {
            Err(ObjectTransferError::InvalidProviderUploadId)
        } else {
            Ok(Self(value))
        }
    }

    /// Expose only at the provider adapter call boundary. Never log this value.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ProviderMultipartUploadId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value)
            .map_err(|_| serde::de::Error::custom("invalid provider multipart upload ID"))
    }
}

impl fmt::Debug for ProviderMultipartUploadId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProviderMultipartUploadId([REDACTED])")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultipartUploadPolicy {
    key_prefix: ObjectKey,
    maximum_size_bytes: u64,
    part_size_bytes: u64,
    allowed_content_types: BTreeSet<String>,
    part_expires_in: TimeDelta,
}

impl MultipartUploadPolicy {
    pub fn new<I, S>(
        key_prefix: ObjectKey,
        maximum_size_bytes: u64,
        part_size_bytes: u64,
        allowed_content_types: I,
    ) -> Result<Self, ObjectTransferError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        if maximum_size_bytes == 0 || maximum_size_bytes > MAX_MULTIPART_OBJECT_SIZE_BYTES {
            return Err(ObjectTransferError::InvalidMaximumSize);
        }
        if !(MIN_MULTIPART_PART_SIZE_BYTES..=MAX_MULTIPART_PART_SIZE_BYTES)
            .contains(&part_size_bytes)
        {
            return Err(ObjectTransferError::InvalidPartSize);
        }
        let allowed_content_types = allowed_content_types
            .into_iter()
            .map(|value| normalize_content_type(value.as_ref()))
            .collect::<Result<BTreeSet<_>, _>>()?;
        if allowed_content_types.is_empty() {
            return Err(ObjectTransferError::EmptyContentTypeAllowlist);
        }
        let policy = Self {
            key_prefix,
            maximum_size_bytes,
            part_size_bytes,
            allowed_content_types,
            part_expires_in: TimeDelta::seconds(DEFAULT_PART_EXPIRY_SECONDS),
        };
        policy.plan(maximum_size_bytes)?;
        Ok(policy)
    }

    pub fn with_part_expiry(mut self, expires_in: TimeDelta) -> Result<Self, ObjectTransferError> {
        validate_expiry(expires_in)?;
        self.part_expires_in = expires_in;
        Ok(self)
    }

    pub fn plan(&self, size_bytes: u64) -> Result<MultipartUploadPlan, ObjectTransferError> {
        if size_bytes == 0 {
            return Err(ObjectTransferError::EmptyObject);
        }
        if size_bytes > self.maximum_size_bytes {
            return Err(ObjectTransferError::ObjectTooLarge {
                actual: size_bytes,
                maximum: self.maximum_size_bytes,
            });
        }
        let count = size_bytes.div_ceil(self.part_size_bytes);
        let part_count = u32::try_from(count).map_err(|_| ObjectTransferError::TooManyParts)?;
        if part_count == 0 || part_count > MAX_MULTIPART_PARTS {
            return Err(ObjectTransferError::TooManyParts);
        }
        Ok(MultipartUploadPlan {
            size_bytes,
            part_size_bytes: self.part_size_bytes,
            part_count,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultipartUploadPlan {
    pub size_bytes: u64,
    pub part_size_bytes: u64,
    pub part_count: u32,
}

impl MultipartUploadPlan {
    pub fn expected_part_size(&self, part_number: u32) -> Result<u64, ObjectTransferError> {
        if part_number == 0 || part_number > self.part_count {
            return Err(ObjectTransferError::InvalidPartNumber);
        }
        if part_number < self.part_count {
            Ok(self.part_size_bytes)
        } else {
            Ok(self.size_bytes - self.part_size_bytes * u64::from(self.part_count - 1))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueMultipartObjectUpload {
    pub content_type: String,
    pub size_bytes: u64,
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignMultipartObject {
    pub key: ObjectKey,
    pub content_type: String,
    pub size_bytes: u64,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignMultipartPart {
    pub key: ObjectKey,
    pub upload_id: ProviderMultipartUploadId,
    pub part_number: u32,
    pub size_bytes: u64,
    /// Hexadecimal SHA-256 for this part. Providers may transport it as base64.
    pub sha256: String,
    pub expires_in: TimeDelta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MultipartPartReceipt {
    pub part_number: u32,
    pub entity_tag: String,
    /// Hexadecimal SHA-256 echoed by the provider for this exact part.
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedMultipartPart {
    pub part_number: u32,
    pub size_bytes: u64,
    pub entity_tag: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteMultipartObject {
    pub key: ObjectKey,
    pub upload_id: ProviderMultipartUploadId,
    pub content_type: String,
    pub size_bytes: u64,
    pub attributes: BTreeMap<String, String>,
    pub parts: Vec<TrustedMultipartPart>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedMultipartObject {
    pub key: ObjectKey,
    pub content_type: String,
    pub size_bytes: u64,
    pub entity_tag: Option<String>,
    pub version_id: Option<String>,
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
}

#[async_trait]
pub trait MultipartObjectSigner: Send + Sync + fmt::Debug {
    async fn initiate_multipart(
        &self,
        request: SignMultipartObject,
    ) -> Result<ProviderMultipartUploadId, ObjectTransferError>;
    async fn sign_multipart_part(
        &self,
        request: SignMultipartPart,
    ) -> Result<PresignedObjectRequest, ObjectTransferError>;
    async fn complete_multipart(
        &self,
        request: CompleteMultipartObject,
    ) -> Result<CompletedMultipartObject, ObjectTransferError>;
    async fn abort_multipart(
        &self,
        key: &ObjectKey,
        upload_id: &ProviderMultipartUploadId,
    ) -> Result<(), ObjectTransferError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultipartUploadGrant {
    pub upload_id: Uuid,
    pub key: ObjectKey,
    pub size_bytes: u64,
    pub part_size_bytes: u64,
    pub part_count: u32,
}

impl MultipartUploadGrant {
    pub fn expected_part_size(&self, part_number: u32) -> Result<u64, ObjectTransferError> {
        MultipartUploadPlan {
            size_bytes: self.size_bytes,
            part_size_bytes: self.part_size_bytes,
            part_count: self.part_count,
        }
        .expected_part_size(part_number)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingMultipartUpload {
    pub upload_id: Uuid,
    pub key: ObjectKey,
    pub provider_upload_id: ProviderMultipartUploadId,
    pub expected_content_type: String,
    pub expected_size_bytes: u64,
    pub expected_attributes: BTreeMap<String, String>,
    pub part_size_bytes: u64,
    pub part_count: u32,
}

impl fmt::Debug for PendingMultipartUpload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingMultipartUpload")
            .field("upload_id", &self.upload_id)
            .field("key", &self.key)
            .field("provider_upload_id", &"[REDACTED]")
            .field("expected_content_type", &self.expected_content_type)
            .field("expected_size_bytes", &self.expected_size_bytes)
            .field("expected_attribute_names", &self.expected_attributes.keys())
            .field("part_size_bytes", &self.part_size_bytes)
            .field("part_count", &self.part_count)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedMultipartUpload {
    pub grant: MultipartUploadGrant,
    pub pending: PendingMultipartUpload,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultipartPartGrant {
    pub upload_id: Uuid,
    pub part_number: u32,
    pub size_bytes: u64,
    pub request: PresignedObjectRequest,
}

impl fmt::Debug for MultipartPartGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MultipartPartGrant")
            .field("upload_id", &self.upload_id)
            .field("part_number", &self.part_number)
            .field("size_bytes", &self.size_bytes)
            .field("request", &self.request)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedMultipartPart {
    pub upload_id: Uuid,
    pub part_number: u32,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedMultipartPart {
    pub grant: MultipartPartGrant,
    pub expected: ExpectedMultipartPart,
}

#[derive(Clone)]
pub struct MultipartObjectService {
    signer: Arc<dyn MultipartObjectSigner>,
    policy: MultipartUploadPolicy,
}

impl fmt::Debug for MultipartObjectService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MultipartObjectService")
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl MultipartObjectService {
    pub fn new(signer: Arc<dyn MultipartObjectSigner>, policy: MultipartUploadPolicy) -> Self {
        Self { signer, policy }
    }

    pub async fn issue(
        &self,
        request: IssueMultipartObjectUpload,
    ) -> Result<IssuedMultipartUpload, ObjectTransferError> {
        let content_type = normalize_content_type(&request.content_type)?;
        if !self.policy.allowed_content_types.contains(&content_type) {
            return Err(ObjectTransferError::UnsupportedContentType(content_type));
        }
        let plan = self.policy.plan(request.size_bytes)?;
        let upload_id = Uuid::now_v7();
        let key = ObjectKey::parse(format!("{}/{upload_id}", self.policy.key_prefix.as_str()))?;
        let mut attributes = validate_attributes(request.attributes)?;
        attributes.insert(UPLOAD_ID_ATTRIBUTE.into(), upload_id.to_string());
        let provider_upload_id = self
            .signer
            .initiate_multipart(SignMultipartObject {
                key: key.clone(),
                content_type: content_type.clone(),
                size_bytes: request.size_bytes,
                attributes: attributes.clone(),
            })
            .await?;
        Ok(IssuedMultipartUpload {
            grant: MultipartUploadGrant {
                upload_id,
                key: key.clone(),
                size_bytes: plan.size_bytes,
                part_size_bytes: plan.part_size_bytes,
                part_count: plan.part_count,
            },
            pending: PendingMultipartUpload {
                upload_id,
                key,
                provider_upload_id,
                expected_content_type: content_type,
                expected_size_bytes: plan.size_bytes,
                expected_attributes: attributes,
                part_size_bytes: plan.part_size_bytes,
                part_count: plan.part_count,
            },
        })
    }

    pub async fn issue_part(
        &self,
        pending: &PendingMultipartUpload,
        part_number: u32,
        sha256: String,
    ) -> Result<IssuedMultipartPart, ObjectTransferError> {
        self.validate_pending(pending)?;
        let plan = pending.plan();
        let size_bytes = plan.expected_part_size(part_number)?;
        let sha256 = normalize_sha256(&sha256)?;
        let signed = self
            .signer
            .sign_multipart_part(SignMultipartPart {
                key: pending.key.clone(),
                upload_id: pending.provider_upload_id.clone(),
                part_number,
                size_bytes,
                sha256: sha256.clone(),
                expires_in: self.policy.part_expires_in,
            })
            .await?;
        Ok(IssuedMultipartPart {
            grant: MultipartPartGrant {
                upload_id: pending.upload_id,
                part_number,
                size_bytes,
                request: signed,
            },
            expected: ExpectedMultipartPart {
                upload_id: pending.upload_id,
                part_number,
                size_bytes,
                sha256,
            },
        })
    }

    pub fn accept_part(
        &self,
        pending: &PendingMultipartUpload,
        expected: &ExpectedMultipartPart,
        receipt: MultipartPartReceipt,
    ) -> Result<TrustedMultipartPart, ObjectTransferError> {
        self.validate_pending(pending)?;
        if expected.upload_id != pending.upload_id
            || receipt.part_number != expected.part_number
            || normalize_sha256(&receipt.sha256)? != expected.sha256
            || pending.plan().expected_part_size(receipt.part_number)? != expected.size_bytes
        {
            return Err(ObjectTransferError::PartReceiptMismatch);
        }
        validate_entity_tag(&receipt.entity_tag)?;
        Ok(TrustedMultipartPart {
            part_number: receipt.part_number,
            size_bytes: expected.size_bytes,
            entity_tag: receipt.entity_tag,
            sha256: expected.sha256.clone(),
        })
    }

    pub async fn complete(
        &self,
        pending: &PendingMultipartUpload,
        parts: &[TrustedMultipartPart],
    ) -> Result<CompletedMultipartObject, ObjectTransferError> {
        self.validate_pending(pending)?;
        validate_complete_parts(pending, parts)?;
        let completed = self
            .signer
            .complete_multipart(CompleteMultipartObject {
                key: pending.key.clone(),
                upload_id: pending.provider_upload_id.clone(),
                content_type: pending.expected_content_type.clone(),
                size_bytes: pending.expected_size_bytes,
                attributes: pending.expected_attributes.clone(),
                parts: parts.to_vec(),
            })
            .await?;
        if completed.key != pending.key {
            return Err(ObjectTransferError::CompletedObjectMismatch);
        }
        if completed.content_type != pending.expected_content_type
            || completed.size_bytes != pending.expected_size_bytes
            || completed.attributes != pending.expected_attributes
        {
            return Err(ObjectTransferError::CompletedObjectMismatch);
        }
        Ok(completed)
    }

    pub async fn abort(&self, pending: &PendingMultipartUpload) -> Result<(), ObjectTransferError> {
        self.validate_pending_identity(pending)?;
        self.signer
            .abort_multipart(&pending.key, &pending.provider_upload_id)
            .await
    }

    fn validate_pending(
        &self,
        pending: &PendingMultipartUpload,
    ) -> Result<(), ObjectTransferError> {
        self.validate_pending_identity(pending)?;
        let content_type = normalize_content_type(&pending.expected_content_type)?;
        let plan = self.policy.plan(pending.expected_size_bytes)?;
        let mut attributes = pending.expected_attributes.clone();
        let upload_id = attributes.remove(UPLOAD_ID_ATTRIBUTE);
        let expected_upload_id = pending.upload_id.to_string();
        validate_attributes(attributes)?;
        if content_type != pending.expected_content_type
            || !self.policy.allowed_content_types.contains(&content_type)
            || pending.part_size_bytes != plan.part_size_bytes
            || pending.part_count != plan.part_count
            || upload_id.as_deref() != Some(expected_upload_id.as_str())
        {
            return Err(ObjectTransferError::InvalidPendingUpload);
        }
        Ok(())
    }

    fn validate_pending_identity(
        &self,
        pending: &PendingMultipartUpload,
    ) -> Result<(), ObjectTransferError> {
        let expected_key = ObjectKey::parse(format!(
            "{}/{}",
            self.policy.key_prefix.as_str(),
            pending.upload_id
        ))?;
        if pending.key != expected_key {
            return Err(ObjectTransferError::InvalidPendingUpload);
        }
        Ok(())
    }
}

impl PendingMultipartUpload {
    const fn plan(&self) -> MultipartUploadPlan {
        MultipartUploadPlan {
            size_bytes: self.expected_size_bytes,
            part_size_bytes: self.part_size_bytes,
            part_count: self.part_count,
        }
    }
}

fn validate_complete_parts(
    pending: &PendingMultipartUpload,
    parts: &[TrustedMultipartPart],
) -> Result<(), ObjectTransferError> {
    if parts.len() != usize::try_from(pending.part_count).unwrap_or(usize::MAX) {
        return Err(ObjectTransferError::IncompletePartManifest {
            expected: pending.part_count,
            actual: u32::try_from(parts.len()).unwrap_or(u32::MAX),
        });
    }
    for (index, part) in parts.iter().enumerate() {
        let part_number =
            u32::try_from(index + 1).map_err(|_| ObjectTransferError::TooManyParts)?;
        if part.part_number != part_number
            || part.size_bytes != pending.plan().expected_part_size(part_number)?
        {
            return Err(ObjectTransferError::InvalidPartManifest);
        }
        normalize_sha256(&part.sha256)?;
        validate_entity_tag(&part.entity_tag)?;
    }
    Ok(())
}

/// Application-visible content state. Provider integrity never skips
/// quarantine; only an application inspection policy records `Accepted`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ObjectValidationState {
    Quarantined,
    Accepted {
        inspector: String,
        inspected_at: DateTime<Utc>,
    },
    Rejected {
        inspector: String,
        code: String,
        inspected_at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectInspectionVerdict {
    Accept,
    Reject { code: String },
}

#[async_trait]
pub trait ObjectContentInspector: Send + Sync + fmt::Debug {
    fn id(&self) -> &str;
    async fn inspect(
        &self,
        object: &ObjectReadHead,
    ) -> Result<ObjectInspectionVerdict, ObjectTransferError>;
}

pub async fn inspect_quarantined_object(
    inspector: &dyn ObjectContentInspector,
    object: &ObjectReadHead,
    now: DateTime<Utc>,
) -> Result<ObjectValidationState, ObjectTransferError> {
    let id = validate_inspector_id(inspector.id())?;
    match inspector.inspect(object).await? {
        ObjectInspectionVerdict::Accept => Ok(ObjectValidationState::Accepted {
            inspector: id,
            inspected_at: now,
        }),
        ObjectInspectionVerdict::Reject { code } => {
            let code = validate_inspection_code(&code)?;
            Ok(ObjectValidationState::Rejected {
                inspector: id,
                code,
                inspected_at: now,
            })
        }
    }
}

/// Usage inputs for structural cost review. Monetary rates are deliberately
/// absent because Region, account, storage class and destination are external.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectTransferCostUsage {
    pub retained_bytes: u64,
    pub incomplete_multipart_bytes: u64,
    pub single_upload_requests: u64,
    pub multipart_initiations: u64,
    pub multipart_part_attempts: u64,
    pub multipart_completions: u64,
    pub multipart_aborts: u64,
    pub metadata_requests: u64,
    pub download_requests: u64,
    pub downloaded_bytes: u64,
    pub accelerated_bytes: u64,
    pub edge_requests: u64,
    pub edge_egress_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectTransferCostProjection {
    pub complete: bool,
    pub fixed_compute: bool,
    pub api_relay_bytes: u64,
    pub usage: ObjectTransferCostUsage,
    pub missing_rates: Vec<String>,
    pub notes: Vec<String>,
}

#[must_use]
pub fn estimate_object_transfer_cost(
    usage: ObjectTransferCostUsage,
) -> ObjectTransferCostProjection {
    let mut missing_rates = Vec::new();
    if usage.retained_bytes > 0 || usage.incomplete_multipart_bytes > 0 {
        missing_rates.push("storage_byte_month".into());
    }
    if usage.single_upload_requests
        + usage.multipart_initiations
        + usage.multipart_part_attempts
        + usage.multipart_completions
        + usage.multipart_aborts
        + usage.metadata_requests
        + usage.download_requests
        > 0
    {
        missing_rates.push("provider_request".into());
    }
    if usage.downloaded_bytes > 0 {
        missing_rates.push("provider_egress_byte".into());
    }
    if usage.accelerated_bytes > 0 {
        missing_rates.push("acceleration_byte".into());
    }
    if usage.edge_requests > 0 {
        missing_rates.push("edge_request".into());
    }
    if usage.edge_egress_bytes > 0 {
        missing_rates.push("edge_egress_byte".into());
    }
    ObjectTransferCostProjection {
        complete: missing_rates.is_empty(),
        fixed_compute: false,
        api_relay_bytes: 0,
        usage,
        missing_rates,
        notes: vec![
            "Direct transfer excludes Lambda and API Gateway file-body relay.".into(),
            "Incomplete multipart bytes accrue storage cost until completion, abort, or lifecycle cleanup.".into(),
            "A provider bill additionally requires account, Region, storage class, destination, retention, and current rates.".into(),
        ],
    }
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum ObjectTransferError {
    #[error("object byte range is invalid")]
    InvalidRange,
    #[error("object byte range cannot be satisfied")]
    RangeNotSatisfiable,
    #[error("the requested object does not exist")]
    MissingObject,
    #[error("the object changed after the client observed it")]
    PreconditionFailed,
    #[error("download filename is invalid")]
    InvalidDownloadName,
    #[error("capability expiry must be greater than zero and no more than 24 hours")]
    InvalidExpiry,
    #[error("multipart maximum size is invalid")]
    InvalidMaximumSize,
    #[error("multipart part size is invalid")]
    InvalidPartSize,
    #[error("multipart upload would exceed the provider part limit")]
    TooManyParts,
    #[error("multipart part number is invalid")]
    InvalidPartNumber,
    #[error("upload content type is invalid")]
    InvalidContentType,
    #[error("upload policy must allow at least one content type")]
    EmptyContentTypeAllowlist,
    #[error("upload content type is not allowed: {0}")]
    UnsupportedContentType(String),
    #[error("upload body must not be empty")]
    EmptyObject,
    #[error("the requested upload is {actual} bytes; the maximum is {maximum} bytes")]
    ObjectTooLarge { actual: u64, maximum: u64 },
    #[error("multipart part SHA-256 must be exactly 64 hexadecimal characters")]
    InvalidSha256,
    #[error("multipart upload attributes are invalid or reserved")]
    InvalidAttributes,
    #[error("provider multipart upload ID is invalid")]
    InvalidProviderUploadId,
    #[error("persisted multipart upload state does not match its configured policy")]
    InvalidPendingUpload,
    #[error("multipart part entity tag is invalid")]
    InvalidEntityTag,
    #[error("multipart part receipt does not match the issued part")]
    PartReceiptMismatch,
    #[error("multipart manifest is incomplete: expected {expected} parts, received {actual}")]
    IncompletePartManifest { expected: u32, actual: u32 },
    #[error("multipart manifest is not consecutive and exact")]
    InvalidPartManifest,
    #[error("provider completed object metadata does not match the trusted session")]
    CompletedObjectMismatch,
    #[error("content inspector identity or rejection code is invalid")]
    InvalidInspectionResult,
    #[error(transparent)]
    ObjectStore(#[from] ObjectStoreError),
    #[error("object transfer provider failed: {0}")]
    Provider(String),
}

fn validate_expiry(value: TimeDelta) -> Result<(), ObjectTransferError> {
    if value <= TimeDelta::zero() || value > TimeDelta::seconds(MAX_CAPABILITY_EXPIRY_SECONDS) {
        Err(ObjectTransferError::InvalidExpiry)
    } else {
        Ok(())
    }
}

fn validate_download_name(value: &str) -> Result<(), ObjectTransferError> {
    if value.is_empty()
        || value.len() > 255
        || !value
            .bytes()
            .all(|byte| byte == b' ' || byte.is_ascii_graphic())
        || value
            .chars()
            .any(|character| matches!(character, '"' | '\\' | '/' | ';'))
    {
        Err(ObjectTransferError::InvalidDownloadName)
    } else {
        Ok(())
    }
}

fn normalize_content_type(value: &str) -> Result<String, ObjectTransferError> {
    let value = value.trim().to_ascii_lowercase();
    let Some((top, subtype)) = value.split_once('/') else {
        return Err(ObjectTransferError::InvalidContentType);
    };
    if value.len() > 255
        || subtype.contains('/')
        || !valid_media_token(top)
        || !valid_media_token(subtype)
    {
        Err(ObjectTransferError::InvalidContentType)
    } else {
        Ok(value)
    }
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

fn normalize_sha256(value: &str) -> Result<String, ObjectTransferError> {
    let value = value.trim();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Err(ObjectTransferError::InvalidSha256)
    } else {
        Ok(value.to_ascii_lowercase())
    }
}

fn validate_attributes(
    attributes: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, ObjectTransferError> {
    if attributes.len() > 31
        || attributes.iter().any(|(key, value)| {
            key.trim().is_empty()
                || key.starts_with(RESERVED_ATTRIBUTE_PREFIX)
                || key.len() > 128
                || value.len() > 1_024
                || key.chars().any(char::is_control)
                || value.chars().any(char::is_control)
        })
    {
        Err(ObjectTransferError::InvalidAttributes)
    } else {
        Ok(attributes)
    }
}

fn validate_entity_tag(value: &str) -> Result<(), ObjectTransferError> {
    if value.is_empty()
        || value.len() > 256
        || value.starts_with("W/")
        || value.chars().any(char::is_control)
    {
        Err(ObjectTransferError::InvalidEntityTag)
    } else {
        Ok(())
    }
}

fn validate_inspector_id(value: &str) -> Result<String, ObjectTransferError> {
    if valid_identifier(value) {
        Ok(value.into())
    } else {
        Err(ObjectTransferError::InvalidInspectionResult)
    }
}

fn validate_inspection_code(value: &str) -> Result<String, ObjectTransferError> {
    if valid_identifier(value) {
        Ok(value.into())
    } else {
        Err(ObjectTransferError::InvalidInspectionResult)
    }
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
}

fn transfer_to_store(error: &ObjectTransferError) -> ObjectStoreError {
    ObjectStoreError::Store(error.to_string())
}
