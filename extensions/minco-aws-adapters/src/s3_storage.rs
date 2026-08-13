use crate::s3::{MAX_SINGLE_POST_SIZE_BYTES, S3Addressing, S3ObjectAdapter};
use async_trait::async_trait;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_sdk_s3::types::ChecksumMode;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use minco_plugin_object_storage::{
    ManagedObjectStoragePlugin, ManagedObjectTransferServices, MultipartObjectService,
    MultipartUploadPolicy, ObjectDownloadPolicy, ObjectDownloadService, ObjectDownloadSigner,
    ObjectHead, ObjectKey, ObjectMetadataReader, ObjectReadService, ObjectStore, ObjectStoreError,
    ObjectStreamReader, ObjectUploadPolicy,
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

const META_SHA256: &str = "minco-sha256";
const META_CREATED_AT: &str = "minco-created-at";
const META_ATTRIBUTES: &str = "minco-attributes";

/// One explicitly configured S3 implementation for storage, signing, metadata,
/// and the verified direct-upload lifecycle.
#[derive(Clone)]
pub struct S3ObjectStorage {
    adapter: Arc<S3ObjectAdapter>,
    metadata: Arc<S3ObjectMetadataReader>,
}

impl std::fmt::Debug for S3ObjectStorage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("S3ObjectStorage")
            .field("adapter", &self.adapter)
            .field("metadata", &self.metadata)
            .finish()
    }
}

impl S3ObjectStorage {
    pub fn new(
        client: aws_sdk_s3::Client,
        credentials: SharedCredentialsProvider,
        bucket: impl Into<String>,
        key_prefix: impl Into<String>,
        region: impl Into<String>,
        endpoint_override: Option<String>,
    ) -> Result<Self, ObjectStoreError> {
        let bucket = bucket.into();
        let key_prefix = key_prefix.into();
        let addressing = S3Addressing::new(region, endpoint_override)?;
        Self::new_with_addressing(client, credentials, bucket, key_prefix, addressing)
    }

    /// Build the SDK client and manual POST signer from one authoritative
    /// endpoint, region, credential, and addressing configuration.
    pub fn from_sdk_builder(
        builder: aws_sdk_s3::config::Builder,
        credentials: SharedCredentialsProvider,
        bucket: impl Into<String>,
        key_prefix: impl Into<String>,
        addressing: S3Addressing,
    ) -> Result<Self, ObjectStoreError> {
        let bucket = bucket.into();
        let addressing = addressing.for_bucket(&bucket);
        let mut builder = builder
            .endpoint_resolver(aws_sdk_s3::config::endpoint::DefaultResolver::new())
            .region(aws_sdk_s3::config::Region::new(
                addressing.region().to_owned(),
            ))
            .force_path_style(addressing.force_path_style())
            .credentials_provider(credentials.clone());
        builder.set_endpoint_url(addressing.endpoint_override().map(str::to_owned));
        let client = aws_sdk_s3::Client::from_conf(builder.build());
        Self::new_with_addressing(client, credentials, bucket, key_prefix, addressing)
    }

    pub fn new_with_addressing(
        client: aws_sdk_s3::Client,
        credentials: SharedCredentialsProvider,
        bucket: impl Into<String>,
        key_prefix: impl Into<String>,
        addressing: S3Addressing,
    ) -> Result<Self, ObjectStoreError> {
        let bucket = bucket.into();
        let key_prefix = key_prefix.into();
        let adapter = Arc::new(S3ObjectAdapter::new_with_addressing(
            client.clone(),
            credentials,
            bucket.clone(),
            key_prefix.clone(),
            addressing,
        )?);
        let metadata = Arc::new(S3ObjectMetadataReader {
            client,
            bucket,
            key_prefix: normalize_prefix(&key_prefix)?,
        });
        Ok(Self { adapter, metadata })
    }

    pub fn plugin(
        &self,
        policy: ObjectUploadPolicy,
    ) -> Result<ManagedObjectStoragePlugin, ObjectStoreError> {
        validate_managed_policy(&policy)?;
        let store: Arc<dyn ObjectStore> = self.adapter.clone();
        let metadata: Arc<dyn ObjectMetadataReader> = self.metadata.clone();
        Ok(ManagedObjectStoragePlugin::new(
            store,
            self.adapter.clone(),
            metadata,
            policy,
        ))
    }

    /// Compose the bounded single-upload lifecycle together with direct
    /// resumable download and multipart services from one exact S3 client,
    /// bucket, prefix, and credential source.
    pub fn transfer_plugin(
        &self,
        upload_policy: ObjectUploadPolicy,
        multipart_policy: MultipartUploadPolicy,
        download_policy: ObjectDownloadPolicy,
    ) -> Result<ManagedObjectStoragePlugin, ObjectStoreError> {
        let plugin = self.plugin(upload_policy)?;
        let reader: Arc<dyn ObjectStreamReader> = self.adapter.clone();
        let download_signer: Arc<dyn ObjectDownloadSigner> = self.adapter.clone();
        let multipart_signer: Arc<dyn minco_plugin_object_storage::MultipartObjectSigner> =
            self.adapter.clone();
        let reads = ObjectReadService::new(reader);
        let downloads = ObjectDownloadService::new(download_signer, reads.clone(), download_policy);
        let multipart = MultipartObjectService::new(multipart_signer, multipart_policy);
        Ok(
            plugin.with_transfer_services(ManagedObjectTransferServices::new(
                reads, downloads, multipart,
            )),
        )
    }

    pub fn adapter(&self) -> Arc<S3ObjectAdapter> {
        Arc::clone(&self.adapter)
    }

    pub fn metadata_reader(&self) -> Arc<S3ObjectMetadataReader> {
        Arc::clone(&self.metadata)
    }
}

const fn validate_managed_policy(policy: &ObjectUploadPolicy) -> Result<(), ObjectStoreError> {
    if policy.maximum_size_bytes() > MAX_SINGLE_POST_SIZE_BYTES {
        Err(ObjectStoreError::ObjectTooLarge)
    } else {
        Ok(())
    }
}

/// S3 `HeadObject` implementation used to verify an upload without reading bytes.
#[derive(Clone)]
pub struct S3ObjectMetadataReader {
    client: aws_sdk_s3::Client,
    bucket: String,
    key_prefix: String,
}

impl std::fmt::Debug for S3ObjectMetadataReader {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("S3ObjectMetadataReader")
            .field("bucket", &self.bucket)
            .field("key_prefix", &self.key_prefix)
            .finish_non_exhaustive()
    }
}

impl S3ObjectMetadataReader {
    fn provider_key(&self, key: &ObjectKey) -> String {
        if self.key_prefix.is_empty() {
            key.as_str().to_owned()
        } else {
            format!("{}/{}", self.key_prefix, key.as_str())
        }
    }
}

#[async_trait]
impl ObjectMetadataReader for S3ObjectMetadataReader {
    async fn head(&self, key: &ObjectKey) -> Result<Option<ObjectHead>, ObjectStoreError> {
        let output = match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(self.provider_key(key))
            .checksum_mode(ChecksumMode::Enabled)
            .send()
            .await
        {
            Ok(output) => output,
            Err(error)
                if error.as_service_error().is_some_and(
                    aws_sdk_s3::operation::head_object::HeadObjectError::is_not_found,
                ) =>
            {
                return Ok(None);
            }
            Err(error) => {
                return Err(ObjectStoreError::Store(crate::provider_error(
                    "S3 HeadObject",
                    &error,
                )));
            }
        };
        decode_head(
            key,
            output.metadata(),
            output.content_type(),
            output.content_length(),
            output.last_modified(),
            output.checksum_sha256(),
        )
        .map(Some)
    }
}

fn decode_head(
    key: &ObjectKey,
    metadata: Option<&HashMap<String, String>>,
    content_type: Option<&str>,
    content_length: Option<i64>,
    last_modified: Option<&aws_sdk_s3::primitives::DateTime>,
    provider_checksum_sha256: Option<&str>,
) -> Result<ObjectHead, ObjectStoreError> {
    let metadata = metadata.cloned().unwrap_or_default();
    let content_type = content_type
        .filter(|value| !value.trim().is_empty())
        .ok_or(ObjectStoreError::InvalidContentType)?
        .to_owned();
    let size_bytes = content_length
        .ok_or_else(|| ObjectStoreError::Store("S3 object has no content length".into()))
        .and_then(|value| {
            u64::try_from(value)
                .map_err(|_| ObjectStoreError::Store("S3 object size is invalid".into()))
        })?;
    let created_at = match metadata.get(META_CREATED_AT) {
        Some(value) => DateTime::parse_from_rfc3339(value)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|error| ObjectStoreError::Store(error.to_string()))?,
        None => last_modified
            .and_then(|value| DateTime::from_timestamp(value.secs(), value.subsec_nanos()))
            .ok_or_else(|| {
                ObjectStoreError::Store("S3 object creation time is unavailable".into())
            })?,
    };
    let attributes = decode_attributes(metadata.get(META_ATTRIBUTES))?;
    let sha256 = decode_sha256(metadata.get(META_SHA256), provider_checksum_sha256)?;
    Ok(ObjectHead {
        key: key.clone(),
        content_type,
        size_bytes,
        sha256,
        created_at,
        attributes,
    })
}

fn decode_attributes(value: Option<&String>) -> Result<BTreeMap<String, String>, ObjectStoreError> {
    let value = value.ok_or_else(|| {
        ObjectStoreError::Store("S3 object attributes metadata is missing".into())
    })?;
    STANDARD
        .decode(value)
        .map_err(|error| ObjectStoreError::Store(error.to_string()))
        .and_then(|bytes| {
            serde_json::from_slice(&bytes)
                .map_err(|error| ObjectStoreError::Store(error.to_string()))
        })
}

fn decode_sha256(
    metadata_sha256: Option<&String>,
    provider_checksum_sha256: Option<&str>,
) -> Result<Option<String>, ObjectStoreError> {
    let metadata_sha256 = metadata_sha256
        .map(|value| validate_sha256(value))
        .transpose()?;
    let provider_sha256 = provider_checksum_sha256
        .map(decode_provider_sha256)
        .transpose()?
        .flatten();
    match (metadata_sha256, provider_sha256) {
        (Some(metadata), Some(provider)) if metadata != provider => Err(ObjectStoreError::Store(
            "S3 checksum metadata does not match the provider checksum".into(),
        )),
        (_, Some(provider)) => Ok(Some(provider)),
        // User-controlled metadata is corroborating evidence only. Managed
        // verification must fail closed when S3 does not expose its own
        // checksum, while compatibility objects remain readable with no
        // verified checksum.
        (_, None) => Ok(None),
    }
}

fn decode_provider_sha256(value: &str) -> Result<Option<String>, ObjectStoreError> {
    let (encoded, is_composite) = match value.rsplit_once('-') {
        Some((encoded, part_count)) => {
            let part_count = part_count.parse::<u32>().map_err(|_| {
                ObjectStoreError::Store("S3 composite SHA-256 checksum is invalid".into())
            })?;
            if part_count == 0 {
                return Err(ObjectStoreError::Store(
                    "S3 composite SHA-256 checksum is invalid".into(),
                ));
            }
            (encoded, true)
        }
        None => (value, false),
    };
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|error| ObjectStoreError::Store(error.to_string()))?;
    if bytes.len() != 32 {
        return Err(ObjectStoreError::Store(
            "S3 SHA-256 checksum has an invalid length".into(),
        ));
    }
    if is_composite {
        // S3's multipart checksum is a checksum of part checksums, not the
        // SHA-256 of the complete byte sequence. Do not promote it to the
        // application field whose contract is the latter.
        Ok(None)
    } else {
        Ok(Some(hex(&bytes)))
    }
}

fn validate_sha256(value: &str) -> Result<String, ObjectStoreError> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(value.to_ascii_lowercase())
    } else {
        Err(ObjectStoreError::Store(
            "S3 SHA-256 metadata is invalid".into(),
        ))
    }
}

fn normalize_prefix(prefix: &str) -> Result<String, ObjectStoreError> {
    let prefix = prefix.trim_matches('/');
    if prefix.is_empty() {
        return Ok(String::new());
    }
    if prefix.chars().any(char::is_control)
        || prefix
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ObjectStoreError::Store("S3 key prefix is invalid".into()));
    }
    Ok(prefix.to_owned())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn head_metadata_preserves_attributes_and_a_verified_checksum() {
        let attributes = BTreeMap::from([("tenant".to_owned(), "acme".to_owned())]);
        let digest = Sha256::digest(b"png");
        let checksum = STANDARD.encode(digest);
        let mut metadata = HashMap::new();
        metadata.insert(
            META_CREATED_AT.to_owned(),
            "2026-08-07T00:00:00.000Z".to_owned(),
        );
        metadata.insert(
            META_ATTRIBUTES.to_owned(),
            STANDARD.encode(serde_json::to_vec(&attributes).unwrap()),
        );
        metadata.insert(META_SHA256.to_owned(), hex(&digest));
        let key = ObjectKey::parse("uploads/image.png").unwrap();
        let head = decode_head(
            &key,
            Some(&metadata),
            Some("image/png"),
            Some(3),
            None,
            Some(&checksum),
        )
        .unwrap();
        assert_eq!(head.key, key);
        assert_eq!(head.attributes, attributes);
        assert_eq!(head.sha256, Some(hex(&digest)));
    }

    #[test]
    fn head_metadata_allows_a_direct_upload_without_a_checksum() {
        let attributes = BTreeMap::<String, String>::new();
        let mut metadata = HashMap::new();
        metadata.insert(
            META_CREATED_AT.to_owned(),
            "2026-08-07T00:00:00.000Z".to_owned(),
        );
        metadata.insert(
            META_ATTRIBUTES.to_owned(),
            STANDARD.encode(serde_json::to_vec(&attributes).unwrap()),
        );
        let key = ObjectKey::parse("uploads/file.bin").unwrap();
        let head = decode_head(
            &key,
            Some(&metadata),
            Some("application/octet-stream"),
            Some(0),
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            head.created_at,
            "2026-08-07T00:00:00Z".parse::<DateTime<Utc>>().unwrap()
        );
        assert_eq!(head.sha256, None);
    }

    #[test]
    fn conflicting_checksums_fail_closed() {
        let metadata = "00".repeat(32);
        let provider = STANDARD.encode([1_u8; 32]);
        let error = decode_sha256(Some(&metadata), Some(&provider)).unwrap_err();
        assert!(error.to_string().contains("does not match"));
    }

    #[test]
    fn metadata_only_checksum_is_not_treated_as_provider_verified() {
        let metadata = hex(&Sha256::digest(b"metadata-only"));
        assert_eq!(decode_sha256(Some(&metadata), None).unwrap(), None);
    }

    #[test]
    fn multipart_composite_checksum_is_not_treated_as_a_whole_object_checksum() {
        let metadata = hex(&Sha256::digest(b"whole-object"));
        let composite = format!("{}-2", STANDARD.encode(Sha256::digest(b"parts")));
        assert_eq!(
            decode_sha256(Some(&metadata), Some(&composite)).unwrap(),
            None
        );
    }

    #[test]
    fn managed_s3_policy_rejects_an_unissuable_single_post_size() {
        let policy = ObjectUploadPolicy::new(
            ObjectKey::parse("uploads").unwrap(),
            MAX_SINGLE_POST_SIZE_BYTES + 1,
            ["application/octet-stream"],
        )
        .unwrap();
        assert!(matches!(
            validate_managed_policy(&policy),
            Err(ObjectStoreError::ObjectTooLarge)
        ));
    }
}
