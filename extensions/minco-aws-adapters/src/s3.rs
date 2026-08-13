use async_trait::async_trait;
use aws_credential_types::{
    Credentials,
    provider::{ProvideCredentials, SharedCredentialsProvider},
};
use aws_sdk_s3::{
    config::endpoint::{DefaultResolver, Params, ResolveEndpoint},
    presigning::PresigningConfig,
    primitives::ByteStream,
    types::{
        ChecksumAlgorithm, ChecksumType, CompletedMultipartUpload, CompletedPart,
        ServerSideEncryption,
    },
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use futures::stream;
use hmac::{Hmac, KeyInit, Mac};
use minco_plugin_object_storage::{
    CompleteMultipartObject, CompletedMultipartObject, MultipartObjectSigner, ObjectAccessSigner,
    ObjectDownloadSigner, ObjectKey, ObjectMetadata, ObjectReadHead, ObjectReadRequest,
    ObjectReadResponse, ObjectStore, ObjectStoreError, ObjectStreamReader, ObjectTransferError,
    ObjectUploadError, ObjectUploadSigner, PresignGetObject, PresignPutObject, PresignedMethod,
    PresignedObjectRequest, ProviderMultipartUploadId, PutObject, SignMultipartObject,
    SignMultipartPart, SignObjectDownload, SignObjectUpload, StoredObject,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    time::{SystemTime, UNIX_EPOCH},
};

type HmacSha256 = Hmac<Sha256>;

const META_SHA256: &str = "minco-sha256";
const META_CREATED_AT: &str = "minco-created-at";
const META_ATTRIBUTES: &str = "minco-attributes";
pub const MAX_SINGLE_POST_SIZE_BYTES: u64 = 5 * 1024 * 1024 * 1024;
const CREDENTIAL_EXPIRY_SAFETY_SKEW: TimeDelta = TimeDelta::seconds(60);

struct S3PostObject {
    key: ObjectKey,
    content_type: String,
    minimum_size_bytes: u64,
    maximum_size_bytes: u64,
    expires_in: TimeDelta,
    attributes: BTreeMap<String, String>,
    sha256: Option<String>,
}

/// Endpoint inputs shared by the generated S3 endpoint resolver and the SDK
/// client used for object operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3Addressing {
    region: String,
    endpoint_override: Option<String>,
    force_path_style: bool,
}

impl S3Addressing {
    pub fn new(
        region: impl Into<String>,
        endpoint_override: Option<String>,
    ) -> Result<Self, ObjectStoreError> {
        let region = region.into();
        validate_region(&region)?;
        if endpoint_override
            .as_deref()
            .is_some_and(|endpoint| !valid_endpoint_override(endpoint))
        {
            return Err(ObjectStoreError::Store(
                "S3 endpoint override is invalid".into(),
            ));
        }
        let endpoint_override =
            endpoint_override.map(|endpoint| endpoint.trim_end_matches('/').to_owned());
        Ok(Self {
            region,
            force_path_style: endpoint_override.is_some(),
            endpoint_override,
        })
    }

    /// Explicitly select path-style bucket addressing.
    #[must_use]
    pub const fn with_path_style(mut self, force_path_style: bool) -> Self {
        self.force_path_style = force_path_style;
        self
    }

    pub fn region(&self) -> &str {
        &self.region
    }

    pub fn endpoint_override(&self) -> Option<&str> {
        self.endpoint_override.as_deref()
    }

    pub const fn force_path_style(&self) -> bool {
        self.force_path_style
    }

    pub(crate) fn for_bucket(mut self, bucket: &str) -> Self {
        if bucket.contains('.') {
            self.force_path_style = true;
        }
        self
    }

    async fn resolve_bucket_endpoint(&self, bucket: &str) -> Result<String, ObjectUploadError> {
        let params = Params::builder()
            .bucket(bucket)
            .region(&self.region)
            .force_path_style(self.force_path_style)
            .set_endpoint(self.endpoint_override.clone())
            .build()
            .map_err(|_| ObjectUploadError::EndpointResolution)?;
        let endpoint = ResolveEndpoint::resolve_endpoint(&DefaultResolver::new(), &params)
            .await
            .map_err(|_| ObjectUploadError::EndpointResolution)?;
        Ok(endpoint.url().trim_end_matches('/').to_owned())
    }
}

#[derive(Clone)]
pub struct S3ObjectAdapter {
    client: aws_sdk_s3::Client,
    credentials: SharedCredentialsProvider,
    bucket: String,
    key_prefix: String,
    addressing: S3Addressing,
}

impl std::fmt::Debug for S3ObjectAdapter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("S3ObjectAdapter")
            .field("bucket", &self.bucket)
            .field("key_prefix", &self.key_prefix)
            .field("addressing", &self.addressing)
            .field("credentials", &"[REDACTED PROVIDER]")
            .finish_non_exhaustive()
    }
}

impl S3ObjectAdapter {
    pub fn new(
        client: aws_sdk_s3::Client,
        credentials: SharedCredentialsProvider,
        bucket: impl Into<String>,
        key_prefix: impl Into<String>,
        region: impl Into<String>,
        endpoint_override: Option<String>,
    ) -> Result<Self, ObjectStoreError> {
        let bucket = bucket.into();
        let addressing = S3Addressing::new(region, endpoint_override)?;
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
        let key_prefix = normalize_prefix(&key_prefix.into())?;
        validate_bucket(&bucket)?;
        let addressing = addressing.for_bucket(&bucket);
        Ok(Self {
            client,
            credentials,
            bucket,
            key_prefix,
            addressing,
        })
    }

    fn provider_key(&self, key: &ObjectKey) -> String {
        provider_key(&self.key_prefix, key)
    }

    async fn sign_post(
        &self,
        request: S3PostObject,
    ) -> Result<PresignedObjectRequest, ObjectUploadError> {
        let credentials = self
            .credentials
            .provide_credentials()
            .await
            .map_err(|_| ObjectStoreError::Store("AWS credentials are unavailable".into()))?;
        sign_post_at(
            &self.bucket,
            &self.key_prefix,
            &self.addressing,
            request,
            &credentials,
            Utc::now(),
        )
        .await
    }

    async fn presigning_window(
        &self,
        expires_in: TimeDelta,
    ) -> Result<(PresigningConfig, DateTime<Utc>), ObjectStoreError> {
        validate_expiry(expires_in)?;
        let credentials = self
            .credentials
            .provide_credentials()
            .await
            .map_err(|_| ObjectStoreError::Store("AWS credentials are unavailable".into()))?;
        let now = Utc::now();
        let expires_at = effective_capability_expiry(now, expires_in, &credentials)
            .map_err(upload_error_to_store)?;
        let seconds = (expires_at - now)
            .num_seconds()
            .try_into()
            .map_err(|_| ObjectStoreError::InvalidExpiry)?;
        let config = PresigningConfig::expires_in(std::time::Duration::from_secs(seconds))
            .map_err(|error| ObjectStoreError::Store(error.to_string()))?;
        Ok((config, expires_at))
    }

    async fn read_head(&self, key: &ObjectKey) -> Result<Option<ObjectReadHead>, ObjectStoreError> {
        let output = match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(self.provider_key(key))
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
        decode_read_head(
            key,
            output.metadata(),
            output.content_type(),
            output.content_length(),
            output.e_tag(),
            output.version_id(),
            output.last_modified(),
        )
        .map(Some)
    }
}

async fn sign_post_at(
    bucket: &str,
    key_prefix: &str,
    addressing: &S3Addressing,
    request: S3PostObject,
    credentials: &Credentials,
    now: DateTime<Utc>,
) -> Result<PresignedObjectRequest, ObjectUploadError> {
    validate_expiry(request.expires_in)?;
    validate_content_type(&request.content_type)?;
    validate_post_size(request.minimum_size_bytes, request.maximum_size_bytes)?;
    let checksum = request.sha256.as_deref().map(sha256_base64).transpose()?;
    let expires_at = effective_capability_expiry(now, request.expires_in, credentials)?;
    let date = now.format("%Y%m%d").to_string();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let scope = format!("{date}/{}/s3/aws4_request", addressing.region());
    let credential = format!("{}/{}", credentials.access_key_id(), scope);
    let attributes = encode_attributes(&request.attributes)?;
    let key = provider_key(key_prefix, &request.key);
    let created_at = now.to_rfc3339_opts(SecondsFormat::Millis, true);
    let mut conditions = vec![
        serde_json::json!({"bucket": bucket}),
        serde_json::json!({"key": key}),
        serde_json::json!({"Content-Type": request.content_type}),
        serde_json::json!([
            "content-length-range",
            request.minimum_size_bytes,
            request.maximum_size_bytes
        ]),
        serde_json::json!({"x-amz-algorithm": "AWS4-HMAC-SHA256"}),
        serde_json::json!({"x-amz-credential": credential}),
        serde_json::json!({"x-amz-date": amz_date}),
        serde_json::json!({"x-amz-server-side-encryption": "AES256"}),
        serde_json::json!({"x-amz-meta-minco-attributes": attributes}),
        serde_json::json!({"x-amz-meta-minco-created-at": created_at}),
    ];
    if let Some(checksum) = &checksum {
        conditions.push(serde_json::json!({"x-amz-checksum-algorithm": "SHA256"}));
        conditions.push(serde_json::json!({"x-amz-checksum-sha256": checksum}));
    }
    if let Some(token) = credentials.session_token() {
        conditions.push(serde_json::json!({"x-amz-security-token": token}));
    }
    let policy = serde_json::json!({
        "expiration": expires_at.to_rfc3339_opts(SecondsFormat::Millis, true),
        "conditions": conditions
    });
    let encoded_policy = STANDARD.encode(
        serde_json::to_vec(&policy).map_err(|error| ObjectStoreError::Store(error.to_string()))?,
    );
    let signature = post_signature(
        credentials.secret_access_key(),
        &date,
        addressing.region(),
        encoded_policy.as_bytes(),
    )?;
    let mut form_fields = BTreeMap::from([
        ("Content-Type".into(), request.content_type),
        ("key".into(), key),
        ("policy".into(), encoded_policy),
        ("x-amz-algorithm".into(), "AWS4-HMAC-SHA256".into()),
        ("x-amz-credential".into(), credential),
        ("x-amz-date".into(), amz_date),
        ("x-amz-server-side-encryption".into(), "AES256".into()),
        ("x-amz-meta-minco-attributes".into(), attributes),
        ("x-amz-meta-minco-created-at".into(), created_at),
        ("x-amz-signature".into(), signature),
    ]);
    if let Some(checksum) = checksum {
        form_fields.insert("x-amz-checksum-algorithm".into(), "SHA256".into());
        form_fields.insert("x-amz-checksum-sha256".into(), checksum);
    }
    if let Some(token) = credentials.session_token() {
        form_fields.insert("x-amz-security-token".into(), token.into());
    }
    Ok(PresignedObjectRequest {
        method: PresignedMethod::Post,
        url: addressing.resolve_bucket_endpoint(bucket).await?,
        headers: BTreeMap::new(),
        form_fields,
        expires_at,
    })
}

#[async_trait]
impl ObjectStore for S3ObjectAdapter {
    async fn put(&self, object: PutObject) -> Result<ObjectMetadata, ObjectStoreError> {
        validate_content_type(&object.content_type)?;
        let attributes = encode_attributes(&object.attributes)?;
        let created_at = Utc::now();
        let digest = Sha256::digest(&object.bytes);
        let sha256 = hex(&digest);
        let checksum = STANDARD.encode(digest);
        let size_bytes =
            u64::try_from(object.bytes.len()).map_err(|_| ObjectStoreError::ObjectTooLarge)?;
        let content_length =
            i64::try_from(size_bytes).map_err(|_| ObjectStoreError::ObjectTooLarge)?;

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(self.provider_key(&object.key))
            .body(ByteStream::from(object.bytes))
            .content_length(content_length)
            .content_type(&object.content_type)
            .checksum_sha256(checksum)
            .server_side_encryption(ServerSideEncryption::Aes256)
            .metadata(META_SHA256, &sha256)
            .metadata(
                META_CREATED_AT,
                created_at.to_rfc3339_opts(SecondsFormat::Millis, true),
            )
            .metadata(META_ATTRIBUTES, attributes)
            .send()
            .await
            .map_err(|error| {
                ObjectStoreError::Store(crate::provider_error("S3 PutObject", &error))
            })?;

        Ok(ObjectMetadata {
            content_type: object.content_type,
            size_bytes,
            sha256,
            created_at,
            attributes: object.attributes,
        })
    }

    async fn get(&self, key: &ObjectKey) -> Result<Option<StoredObject>, ObjectStoreError> {
        let output = match self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(self.provider_key(key))
            .send()
            .await
        {
            Ok(output) => output,
            Err(error)
                if error.as_service_error().is_some_and(
                    aws_sdk_s3::operation::get_object::GetObjectError::is_no_such_key,
                ) =>
            {
                return Ok(None);
            }
            Err(error) => {
                return Err(ObjectStoreError::Store(crate::provider_error(
                    "S3 GetObject",
                    &error,
                )));
            }
        };
        let provider_metadata = output.metadata().cloned();
        let content_type = output.content_type().map(str::to_owned);
        let content_length = output.content_length();
        let last_modified = output.last_modified().copied();
        let bytes = output
            .body
            .collect()
            .await
            .map_err(|error| ObjectStoreError::Store(format!("S3 body read failed: {error}")))?
            .into_bytes()
            .to_vec();
        let computed_sha256 = hex(&Sha256::digest(&bytes));
        let metadata = decode_metadata(
            provider_metadata.as_ref(),
            content_type.as_deref(),
            content_length,
            last_modified.as_ref(),
            &computed_sha256,
        )?;
        if usize::try_from(metadata.size_bytes).ok() != Some(bytes.len()) {
            return Err(ObjectStoreError::Store(
                "S3 object size does not match the downloaded body".into(),
            ));
        }
        Ok(Some(StoredObject {
            key: key.clone(),
            bytes,
            metadata,
        }))
    }

    async fn delete(&self, key: &ObjectKey) -> Result<bool, ObjectStoreError> {
        let provider_key = self.provider_key(key);
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&provider_key)
            .send()
            .await
        {
            Ok(_) => {}
            Err(error)
                if error.as_service_error().is_some_and(
                    aws_sdk_s3::operation::head_object::HeadObjectError::is_not_found,
                ) =>
            {
                return Ok(false);
            }
            Err(error) => {
                return Err(ObjectStoreError::Store(crate::provider_error(
                    "S3 HeadObject",
                    &error,
                )));
            }
        }
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(provider_key)
            .send()
            .await
            .map_err(|error| {
                ObjectStoreError::Store(crate::provider_error("S3 DeleteObject", &error))
            })?;
        Ok(true)
    }
}

#[async_trait]
impl ObjectAccessSigner for S3ObjectAdapter {
    async fn sign_put(
        &self,
        request: PresignPutObject,
    ) -> Result<PresignedObjectRequest, ObjectStoreError> {
        self.sign_post(S3PostObject {
            key: request.key,
            content_type: request.content_type,
            minimum_size_bytes: 0,
            maximum_size_bytes: request.maximum_size_bytes,
            expires_in: request.expires_in,
            attributes: request.attributes,
            sha256: None,
        })
        .await
        .map_err(upload_error_to_store)
    }

    async fn sign_get(
        &self,
        request: PresignGetObject,
    ) -> Result<PresignedObjectRequest, ObjectStoreError> {
        let (config, expires_at) = self.presigning_window(request.expires_in).await?;
        let mut operation = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(self.provider_key(&request.key));
        if let Some(file_name) = request.download_file_name {
            validate_download_name(&file_name)?;
            operation = operation
                .response_content_disposition(format!("attachment; filename=\"{file_name}\""));
        }
        let signed = operation
            .presigned(config)
            .await
            .map_err(|error| ObjectStoreError::Store(format!("S3 presigning failed: {error}")))?;
        Ok(PresignedObjectRequest {
            method: PresignedMethod::Get,
            url: signed.uri().to_owned(),
            headers: signed
                .headers()
                .map(|(name, value)| (name.to_ascii_lowercase(), value.to_owned()))
                .collect(),
            form_fields: BTreeMap::new(),
            expires_at,
        })
    }
}

#[async_trait]
impl ObjectDownloadSigner for S3ObjectAdapter {
    async fn sign_download(
        &self,
        request: SignObjectDownload,
    ) -> Result<PresignedObjectRequest, ObjectTransferError> {
        validate_download_name(request.download_file_name.as_deref().unwrap_or("download"))?;
        let (config, expires_at) = self
            .presigning_window(request.expires_in)
            .await
            .map_err(ObjectTransferError::from)?;
        let mut operation = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(self.provider_key(&request.key))
            .if_match(request.if_match)
            .response_cache_control(request.cache_control);
        if let Some(range) = request.range {
            operation = operation.range(range.to_http_value());
        }
        if let Some(version_id) = request.version_id {
            operation = operation.version_id(version_id);
        }
        if let Some(file_name) = request.download_file_name {
            operation = operation
                .response_content_disposition(format!("attachment; filename=\"{file_name}\""));
        }
        let signed = operation.presigned(config).await.map_err(|error| {
            ObjectTransferError::Provider(format!("S3 presigning failed: {error}"))
        })?;
        Ok(PresignedObjectRequest {
            method: PresignedMethod::Get,
            url: signed.uri().to_owned(),
            headers: signed
                .headers()
                .map(|(name, value)| (name.to_ascii_lowercase(), value.to_owned()))
                .collect(),
            form_fields: BTreeMap::new(),
            expires_at,
        })
    }
}

#[async_trait]
impl ObjectStreamReader for S3ObjectAdapter {
    async fn head(&self, key: &ObjectKey) -> Result<Option<ObjectReadHead>, ObjectStoreError> {
        self.read_head(key).await
    }

    async fn read(
        &self,
        request: ObjectReadRequest,
    ) -> Result<Option<ObjectReadResponse>, ObjectStoreError> {
        let Some(head) = self.read_head(&request.key).await? else {
            return Ok(None);
        };
        if request
            .expected_entity_tag
            .as_deref()
            .is_some_and(|expected| expected != head.entity_tag)
            || request
                .version_id
                .as_deref()
                .is_some_and(|expected| Some(expected) != head.version_id.as_deref())
        {
            return Err(ObjectStoreError::Store(
                ObjectTransferError::PreconditionFailed.to_string(),
            ));
        }
        let mut operation = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(self.provider_key(&request.key))
            .if_match(&head.entity_tag);
        if let Some(range) = request.range {
            operation = operation.range(range.to_http_value());
        }
        if let Some(version_id) = request.version_id.or_else(|| head.version_id.clone()) {
            operation = operation.version_id(version_id);
        }
        let output = match operation.send().await {
            Ok(output) => output,
            Err(error)
                if error.as_service_error().is_some_and(
                    aws_sdk_s3::operation::get_object::GetObjectError::is_no_such_key,
                ) =>
            {
                return Ok(None);
            }
            Err(error) => {
                return Err(ObjectStoreError::Store(crate::provider_error(
                    "S3 GetObject stream",
                    &error,
                )));
            }
        };
        let content_range = output.content_range().map(str::to_owned);
        let body = output.body;
        let chunks = stream::unfold(body, |mut body| async move {
            body.next().await.map(|chunk| {
                let chunk = chunk.map(|bytes| bytes.to_vec()).map_err(|error| {
                    ObjectStoreError::Store(format!("S3 body read failed: {error}"))
                });
                (chunk, body)
            })
        });
        Ok(Some(ObjectReadResponse {
            head,
            content_range,
            stream: Box::pin(chunks),
        }))
    }
}

#[async_trait]
impl MultipartObjectSigner for S3ObjectAdapter {
    async fn initiate_multipart(
        &self,
        request: SignMultipartObject,
    ) -> Result<ProviderMultipartUploadId, ObjectTransferError> {
        validate_content_type(&request.content_type)?;
        let attributes = encode_attributes(&request.attributes)?;
        let created_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let output = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(self.provider_key(&request.key))
            .content_type(request.content_type)
            .checksum_algorithm(ChecksumAlgorithm::Sha256)
            .checksum_type(ChecksumType::Composite)
            .server_side_encryption(ServerSideEncryption::Aes256)
            .metadata(META_CREATED_AT, created_at)
            .metadata(META_ATTRIBUTES, attributes)
            .send()
            .await
            .map_err(|error| {
                ObjectTransferError::Provider(crate::provider_error(
                    "S3 CreateMultipartUpload",
                    &error,
                ))
            })?;
        ProviderMultipartUploadId::parse(
            output
                .upload_id()
                .ok_or_else(|| {
                    ObjectTransferError::Provider(
                        "S3 CreateMultipartUpload returned no upload ID".into(),
                    )
                })?
                .to_owned(),
        )
    }

    async fn sign_multipart_part(
        &self,
        request: SignMultipartPart,
    ) -> Result<PresignedObjectRequest, ObjectTransferError> {
        let (config, expires_at) = self
            .presigning_window(request.expires_in)
            .await
            .map_err(ObjectTransferError::from)?;
        let content_length =
            i64::try_from(request.size_bytes).map_err(|_| ObjectTransferError::InvalidPartSize)?;
        let part_number = i32::try_from(request.part_number)
            .map_err(|_| ObjectTransferError::InvalidPartNumber)?;
        let checksum = sha256_base64(&request.sha256)?;
        let signed = self
            .client
            .upload_part()
            .bucket(&self.bucket)
            .key(self.provider_key(&request.key))
            .upload_id(request.upload_id.expose_secret())
            .part_number(part_number)
            .content_length(content_length)
            .checksum_algorithm(ChecksumAlgorithm::Sha256)
            .checksum_sha256(checksum)
            .presigned(config)
            .await
            .map_err(|error| {
                ObjectTransferError::Provider(format!("S3 presigning failed: {error}"))
            })?;
        Ok(PresignedObjectRequest {
            method: PresignedMethod::Put,
            url: signed.uri().to_owned(),
            headers: signed
                .headers()
                .map(|(name, value)| (name.to_ascii_lowercase(), value.to_owned()))
                .collect(),
            form_fields: BTreeMap::new(),
            expires_at,
        })
    }

    async fn complete_multipart(
        &self,
        request: CompleteMultipartObject,
    ) -> Result<CompletedMultipartObject, ObjectTransferError> {
        let parts = request
            .parts
            .iter()
            .map(|part| {
                Ok(CompletedPart::builder()
                    .part_number(
                        i32::try_from(part.part_number)
                            .map_err(|_| ObjectTransferError::InvalidPartNumber)?,
                    )
                    .e_tag(&part.entity_tag)
                    .checksum_sha256(sha256_base64(&part.sha256)?)
                    .build())
            })
            .collect::<Result<Vec<_>, ObjectTransferError>>()?;
        self.client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(self.provider_key(&request.key))
            .upload_id(request.upload_id.expose_secret())
            .multipart_upload(
                CompletedMultipartUpload::builder()
                    .set_parts(Some(parts))
                    .build(),
            )
            .checksum_type(ChecksumType::Composite)
            .send()
            .await
            .map_err(|error| {
                ObjectTransferError::Provider(crate::provider_error(
                    "S3 CompleteMultipartUpload",
                    &error,
                ))
            })?;
        let head = self
            .read_head(&request.key)
            .await?
            .ok_or(ObjectTransferError::MissingObject)?;
        Ok(CompletedMultipartObject {
            key: head.key,
            content_type: head.content_type,
            size_bytes: head.size_bytes,
            entity_tag: Some(head.entity_tag),
            version_id: head.version_id,
            attributes: head.attributes,
        })
    }

    async fn abort_multipart(
        &self,
        key: &ObjectKey,
        upload_id: &ProviderMultipartUploadId,
    ) -> Result<(), ObjectTransferError> {
        self.client
            .abort_multipart_upload()
            .bucket(&self.bucket)
            .key(self.provider_key(key))
            .upload_id(upload_id.expose_secret())
            .send()
            .await
            .map_err(|error| {
                ObjectTransferError::Provider(crate::provider_error(
                    "S3 AbortMultipartUpload",
                    &error,
                ))
            })?;
        Ok(())
    }
}

#[async_trait]
impl ObjectUploadSigner for S3ObjectAdapter {
    async fn sign_upload(
        &self,
        request: SignObjectUpload,
    ) -> Result<PresignedObjectRequest, ObjectUploadError> {
        self.sign_post(S3PostObject {
            key: request.key,
            content_type: request.content_type,
            minimum_size_bytes: request.size_bytes,
            maximum_size_bytes: request.size_bytes,
            expires_in: request.expires_in,
            attributes: request.attributes,
            sha256: Some(request.sha256),
        })
        .await
    }
}

fn valid_endpoint_override(endpoint: &str) -> bool {
    crate::validated_service_uri(endpoint).is_some_and(|uri| uri.path() == "/")
}

fn validate_region(region: &str) -> Result<(), ObjectStoreError> {
    if region.trim().is_empty()
        || region.len() > 64
        || !region
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        Err(ObjectStoreError::Store("AWS region is invalid".into()))
    } else {
        Ok(())
    }
}

fn validate_bucket(bucket: &str) -> Result<(), ObjectStoreError> {
    if !valid_bucket_name(bucket) {
        return Err(ObjectStoreError::Store("S3 bucket name is invalid".into()));
    }
    Ok(())
}

pub(crate) fn valid_bucket_name(bucket: &str) -> bool {
    let first_and_last_are_alphanumeric = bucket
        .as_bytes()
        .first()
        .zip(bucket.as_bytes().last())
        .is_some_and(|(first, last)| first.is_ascii_alphanumeric() && last.is_ascii_alphanumeric());
    (3..=63).contains(&bucket.len())
        && first_and_last_are_alphanumeric
        && !bucket.contains("..")
        && !bucket.contains(".-")
        && !bucket.contains("-.")
        && !bucket.starts_with("xn--")
        && !bucket.starts_with("sthree-")
        && !bucket.starts_with("amzn-s3-demo-")
        && !bucket.ends_with("-s3alias")
        && !bucket.ends_with("--ol-s3")
        && bucket.strip_suffix(".mrap").is_none()
        && !bucket.ends_with("--x-s3")
        && !bucket.ends_with("--table-s3")
        && bucket.parse::<std::net::Ipv4Addr>().is_err()
        && bucket.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'.'
        })
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

fn validate_content_type(value: &str) -> Result<(), ObjectStoreError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value.chars().any(char::is_control)
        || !value.contains('/')
    {
        Err(ObjectStoreError::InvalidContentType)
    } else {
        Ok(())
    }
}

fn validate_expiry(value: TimeDelta) -> Result<(), ObjectStoreError> {
    if value <= TimeDelta::zero() || value > TimeDelta::hours(24) {
        Err(ObjectStoreError::InvalidExpiry)
    } else {
        Ok(())
    }
}

const fn validate_post_size(
    minimum_size_bytes: u64,
    maximum_size_bytes: u64,
) -> Result<(), ObjectStoreError> {
    if maximum_size_bytes == 0 || minimum_size_bytes > maximum_size_bytes {
        return Err(ObjectStoreError::InvalidMaximumSize);
    }
    if maximum_size_bytes > MAX_SINGLE_POST_SIZE_BYTES {
        return Err(ObjectStoreError::ObjectTooLarge);
    }
    Ok(())
}

fn validate_download_name(value: &str) -> Result<(), ObjectStoreError> {
    if value.is_empty()
        || value.len() > 255
        || !value
            .bytes()
            .all(|byte| byte == b' ' || byte.is_ascii_graphic())
        || value
            .chars()
            .any(|character| matches!(character, '"' | '\\' | '/' | ';'))
    {
        return Err(ObjectStoreError::Store(
            "download filename is invalid".into(),
        ));
    }
    Ok(())
}

fn provider_key(key_prefix: &str, key: &ObjectKey) -> String {
    if key_prefix.is_empty() {
        key.as_str().to_owned()
    } else {
        format!("{key_prefix}/{}", key.as_str())
    }
}

fn effective_capability_expiry(
    now: DateTime<Utc>,
    expires_in: TimeDelta,
    credentials: &Credentials,
) -> Result<DateTime<Utc>, ObjectUploadError> {
    let requested_expiry = now
        .checked_add_signed(expires_in)
        .ok_or(ObjectUploadError::InvalidExpiry)?;
    let Some(credential_expiry) = credentials.expiry() else {
        return Ok(requested_expiry);
    };
    let credential_expiry = system_time_to_utc(credential_expiry)?;
    let safe_credential_expiry = credential_expiry
        .checked_sub_signed(CREDENTIAL_EXPIRY_SAFETY_SKEW)
        .ok_or(ObjectUploadError::InvalidCredentialExpiry)?;
    if safe_credential_expiry <= now {
        return Err(ObjectUploadError::CredentialLifetimeTooShort);
    }
    Ok(requested_expiry.min(safe_credential_expiry))
}

fn system_time_to_utc(value: SystemTime) -> Result<DateTime<Utc>, ObjectUploadError> {
    let since_epoch = value
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ObjectUploadError::InvalidCredentialExpiry)?;
    let seconds = i64::try_from(since_epoch.as_secs())
        .map_err(|_| ObjectUploadError::InvalidCredentialExpiry)?;
    DateTime::from_timestamp(seconds, since_epoch.subsec_nanos())
        .ok_or(ObjectUploadError::InvalidCredentialExpiry)
}

fn upload_error_to_store(error: ObjectUploadError) -> ObjectStoreError {
    match error {
        ObjectUploadError::ObjectStore(error) => error,
        error => ObjectStoreError::Store(error.to_string()),
    }
}

fn encode_attributes(attributes: &BTreeMap<String, String>) -> Result<String, ObjectStoreError> {
    if attributes.len() > 32
        || attributes.iter().any(|(key, value)| {
            key.trim().is_empty()
                || key.len() > 128
                || value.len() > 1024
                || key.chars().any(char::is_control)
                || value.chars().any(char::is_control)
        })
    {
        return Err(ObjectStoreError::Store(
            "object attributes exceed the S3 metadata boundary".into(),
        ));
    }
    let encoded = STANDARD.encode(
        serde_json::to_vec(attributes)
            .map_err(|error| ObjectStoreError::Store(error.to_string()))?,
    );
    if encoded.len() > 1536 {
        return Err(ObjectStoreError::Store(
            "object attributes exceed the S3 metadata boundary".into(),
        ));
    }
    Ok(encoded)
}

fn sha256_base64(value: &str) -> Result<String, ObjectStoreError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ObjectStoreError::Store(
            "SHA-256 checksum must be exactly 64 hexadecimal characters".into(),
        ));
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
    }
    Ok(STANDARD.encode(bytes))
}

fn hex_nibble(value: u8) -> Result<u8, ObjectStoreError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(ObjectStoreError::Store(
            "SHA-256 checksum contains a non-hexadecimal character".into(),
        )),
    }
}

fn decode_metadata(
    metadata: Option<&HashMap<String, String>>,
    content_type: Option<&str>,
    content_length: Option<i64>,
    last_modified: Option<&aws_sdk_s3::primitives::DateTime>,
    computed_sha256: &str,
) -> Result<ObjectMetadata, ObjectStoreError> {
    let metadata = metadata.cloned().unwrap_or_default();
    if metadata.get(META_SHA256).is_some_and(|value| {
        value.len() != 64
            || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
            || !value.eq_ignore_ascii_case(computed_sha256)
    }) {
        return Err(ObjectStoreError::Store(
            "S3 object checksum metadata does not match the downloaded body".into(),
        ));
    }
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
    let attributes = metadata
        .get(META_ATTRIBUTES)
        .ok_or_else(|| ObjectStoreError::Store("S3 object attributes metadata is missing".into()))
        .and_then(|value| {
            STANDARD
                .decode(value)
                .map_err(|error| ObjectStoreError::Store(error.to_string()))
        })
        .and_then(|bytes| {
            serde_json::from_slice(&bytes)
                .map_err(|error| ObjectStoreError::Store(error.to_string()))
        })?;
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
    Ok(ObjectMetadata {
        content_type,
        size_bytes,
        sha256: computed_sha256.to_owned(),
        created_at,
        attributes,
    })
}

fn decode_read_head(
    key: &ObjectKey,
    metadata: Option<&HashMap<String, String>>,
    content_type: Option<&str>,
    content_length: Option<i64>,
    entity_tag: Option<&str>,
    version_id: Option<&str>,
    last_modified: Option<&aws_sdk_s3::primitives::DateTime>,
) -> Result<ObjectReadHead, ObjectStoreError> {
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
    let entity_tag = entity_tag
        .filter(|value| !value.is_empty() && !value.starts_with("W/"))
        .ok_or_else(|| ObjectStoreError::Store("S3 object has no strong entity tag".into()))?
        .to_owned();
    let last_modified = last_modified
        .and_then(|value| DateTime::from_timestamp(value.secs(), value.subsec_nanos()))
        .ok_or_else(|| ObjectStoreError::Store("S3 last-modified time is unavailable".into()))?;
    let attributes = metadata
        .and_then(|values| values.get(META_ATTRIBUTES))
        .map(|value| {
            STANDARD
                .decode(value)
                .map_err(|error| ObjectStoreError::Store(error.to_string()))
                .and_then(|bytes| {
                    serde_json::from_slice(&bytes)
                        .map_err(|error| ObjectStoreError::Store(error.to_string()))
                })
        })
        .transpose()?
        .unwrap_or_default();
    Ok(ObjectReadHead {
        key: key.clone(),
        content_type,
        size_bytes,
        entity_tag,
        version_id: version_id.map(str::to_owned),
        last_modified,
        attributes,
    })
}

fn post_signature(
    secret: &str,
    date: &str,
    region: &str,
    encoded_policy: &[u8],
) -> Result<String, ObjectStoreError> {
    let date_key = hmac(format!("AWS4{secret}").as_bytes(), date.as_bytes())?;
    let region_key = hmac(&date_key, region.as_bytes())?;
    let service_key = hmac(&region_key, b"s3")?;
    let signing_key = hmac(&service_key, b"aws4_request")?;
    Ok(hex(&hmac(&signing_key, encoded_policy)?))
}

fn hmac(key: &[u8], value: &[u8]) -> Result<Vec<u8>, ObjectStoreError> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|_| ObjectStoreError::Store("S3 signing key is invalid".into()))?;
    mac.update(value);
    Ok(mac.finalize().into_bytes().to_vec())
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
    use serde_json::Value;
    use std::collections::BTreeSet;

    const ACCESS_KEY: &str = "AKIDEXAMPLE";
    const SECRET_KEY: &str = "fixed-test-secret-never-log";
    const SESSION_TOKEN: &str = "fixed-session-token-never-log";

    fn fixed_now() -> DateTime<Utc> {
        "2026-08-09T01:02:03Z".parse().unwrap()
    }

    fn as_system_time(value: DateTime<Utc>) -> SystemTime {
        let seconds = u64::try_from(value.timestamp()).unwrap();
        UNIX_EPOCH
            + std::time::Duration::from_secs(seconds)
            + std::time::Duration::from_nanos(u64::from(value.timestamp_subsec_nanos()))
    }

    fn credentials(session_token: Option<&str>, expiry: Option<DateTime<Utc>>) -> Credentials {
        Credentials::new(
            ACCESS_KEY,
            SECRET_KEY,
            session_token.map(str::to_owned),
            expiry.map(as_system_time),
            "minco-fixed-policy-test",
        )
    }

    fn adapter() -> S3ObjectAdapter {
        adapter_with_credentials(credentials(None, None))
    }

    fn adapter_with_credentials(value: Credentials) -> S3ObjectAdapter {
        let credentials = SharedCredentialsProvider::new(value);
        let config = aws_sdk_s3::Config::builder()
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .region(aws_sdk_s3::config::Region::new("ap-southeast-2"))
            .credentials_provider(credentials.clone())
            .build();
        S3ObjectAdapter::new(
            aws_sdk_s3::Client::from_conf(config),
            credentials,
            "minco-objects",
            "tenant-uploads",
            "ap-southeast-2",
            None,
        )
        .unwrap()
    }

    fn fixed_post() -> S3PostObject {
        let digest = Sha256::digest(b"verified upload");
        S3PostObject {
            key: ObjectKey::parse("images/018f6f4a-8d29-7a31-a8dc-123456789abc").unwrap(),
            content_type: "image/png".into(),
            minimum_size_bytes: 15,
            maximum_size_bytes: 15,
            expires_in: TimeDelta::minutes(10),
            attributes: BTreeMap::from([
                (
                    "minco.upload_id".into(),
                    "018f6f4a-8d29-7a31-a8dc-123456789abc".into(),
                ),
                ("tenant".into(), "acme".into()),
            ]),
            sha256: Some(hex(&digest)),
        }
    }

    fn condition_fields(policy: &Value) -> BTreeSet<String> {
        policy["conditions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|condition| {
                if let Some(object) = condition.as_object() {
                    object.keys().next().cloned()
                } else {
                    condition.as_array().and_then(|values| {
                        values
                            .get(1)
                            .and_then(Value::as_str)
                            .map(|field| field.trim_start_matches('$').to_owned())
                    })
                }
            })
            .collect()
    }

    #[tokio::test]
    async fn managed_post_policy_is_a_complete_deterministic_contract() {
        let now = fixed_now();
        let credential_expiry = now + TimeDelta::minutes(5);
        let addressing = S3Addressing::new("ap-southeast-2", None).unwrap();
        let request = fixed_post();
        let expected_checksum = sha256_base64(request.sha256.as_deref().unwrap()).unwrap();
        let expected_attributes = encode_attributes(&request.attributes).unwrap();
        let signed = sign_post_at(
            "minco-objects",
            "tenant-uploads",
            &addressing,
            request,
            &credentials(Some(SESSION_TOKEN), Some(credential_expiry)),
            now,
        )
        .await
        .unwrap();

        assert_eq!(
            signed.url,
            "https://minco-objects.s3.ap-southeast-2.amazonaws.com"
        );
        assert_eq!(signed.expires_at, now + TimeDelta::minutes(4));
        let policy_bytes = STANDARD.decode(&signed.form_fields["policy"]).unwrap();
        let policy: Value = serde_json::from_slice(&policy_bytes).unwrap();
        assert_eq!(policy["expiration"], "2026-08-09T01:06:03.000Z");
        let conditions = policy["conditions"].as_array().unwrap();
        for expected in [
            serde_json::json!({"bucket": "minco-objects"}),
            serde_json::json!({"key": "tenant-uploads/images/018f6f4a-8d29-7a31-a8dc-123456789abc"}),
            serde_json::json!({"Content-Type": "image/png"}),
            serde_json::json!(["content-length-range", 15, 15]),
            serde_json::json!({"x-amz-checksum-algorithm": "SHA256"}),
            serde_json::json!({"x-amz-checksum-sha256": expected_checksum.clone()}),
            serde_json::json!({"x-amz-server-side-encryption": "AES256"}),
            serde_json::json!({"x-amz-meta-minco-attributes": expected_attributes.clone()}),
            serde_json::json!({"x-amz-meta-minco-created-at": "2026-08-09T01:02:03.000Z"}),
            serde_json::json!({"x-amz-security-token": SESSION_TOKEN}),
        ] {
            assert!(
                conditions.contains(&expected),
                "missing condition {expected}"
            );
        }

        let covered = condition_fields(&policy);
        for field in signed
            .form_fields
            .keys()
            .filter(|field| !matches!(field.as_str(), "policy" | "x-amz-signature" | "file"))
        {
            assert!(
                covered.contains(field),
                "returned field lacks condition: {field}"
            );
        }
        assert_eq!(
            signed.form_fields["x-amz-checksum-sha256"],
            expected_checksum
        );
        assert_eq!(signed.form_fields["x-amz-security-token"], SESSION_TOKEN);
        assert_eq!(
            signed.form_fields["x-amz-signature"],
            "6c21f38e5a98818daaa6b0029796ef379c24e7c6e5a5a7b3e66bf6a28ae476ce"
        );

        let serialized = serde_json::to_string(&signed).unwrap();
        let debug = format!("{signed:?}");
        for output in [
            signed.url.as_str(),
            std::str::from_utf8(&policy_bytes).unwrap(),
            serialized.as_str(),
            debug.as_str(),
        ] {
            assert!(!output.contains(SECRET_KEY));
        }
        assert!(debug.contains("[REDACTED PRESIGNED URL]"));
        assert!(!debug.contains(SESSION_TOKEN));
    }

    #[tokio::test]
    async fn generated_endpoint_rules_cover_partitions_and_safe_bucket_forms() {
        let commercial = S3Addressing::new("ap-southeast-2", None).unwrap();
        assert_eq!(
            commercial
                .resolve_bucket_endpoint("minco-objects")
                .await
                .unwrap(),
            "https://minco-objects.s3.ap-southeast-2.amazonaws.com"
        );
        assert_eq!(
            commercial
                .clone()
                .for_bucket("uploads.example.com")
                .resolve_bucket_endpoint("uploads.example.com")
                .await
                .unwrap(),
            "https://s3.ap-southeast-2.amazonaws.com/uploads.example.com"
        );
        assert_eq!(
            S3Addressing::new("cn-north-1", None)
                .unwrap()
                .resolve_bucket_endpoint("minco-objects")
                .await
                .unwrap(),
            "https://minco-objects.s3.cn-north-1.amazonaws.com.cn"
        );
        assert_eq!(
            S3Addressing::new("ap-southeast-2", Some("http://127.0.0.1:4566".into()))
                .unwrap()
                .resolve_bucket_endpoint("minco-objects")
                .await
                .unwrap(),
            "http://127.0.0.1:4566/minco-objects"
        );
    }

    #[tokio::test]
    async fn credential_expiry_bounds_capabilities_and_rejects_unsafe_lifetimes() {
        let now = fixed_now();
        let addressing = S3Addressing::new("ap-southeast-2", None).unwrap();

        let static_signed = sign_post_at(
            "minco-objects",
            "tenant-uploads",
            &addressing,
            fixed_post(),
            &credentials(None, None),
            now,
        )
        .await
        .unwrap();
        assert_eq!(static_signed.expires_at, now + TimeDelta::minutes(10));

        let temporary_signed = sign_post_at(
            "minco-objects",
            "tenant-uploads",
            &addressing,
            fixed_post(),
            &credentials(None, Some(now + TimeDelta::minutes(5))),
            now,
        )
        .await
        .unwrap();
        assert_eq!(temporary_signed.expires_at, now + TimeDelta::minutes(4));

        let too_soon = sign_post_at(
            "minco-objects",
            "tenant-uploads",
            &addressing,
            fixed_post(),
            &credentials(Some(SESSION_TOKEN), Some(now + TimeDelta::seconds(30))),
            now,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            too_soon,
            ObjectUploadError::CredentialLifetimeTooShort
        ));
        assert!(!too_soon.to_string().contains(SECRET_KEY));

        let pre_epoch = Credentials::new(
            ACCESS_KEY,
            SECRET_KEY,
            Some(SESSION_TOKEN.into()),
            UNIX_EPOCH.checked_sub(std::time::Duration::from_secs(1)),
            "minco-invalid-expiry-test",
        );
        assert!(matches!(
            sign_post_at(
                "minco-objects",
                "tenant-uploads",
                &addressing,
                fixed_post(),
                &pre_epoch,
                now,
            )
            .await,
            Err(ObjectUploadError::InvalidCredentialExpiry)
        ));
    }

    #[tokio::test]
    async fn resumable_presigned_requests_bind_range_size_and_part_checksum() {
        let adapter = adapter();
        let key = ObjectKey::parse("documents/revision-2").unwrap();
        let download = ObjectDownloadSigner::sign_download(
            &adapter,
            SignObjectDownload {
                key: key.clone(),
                range: Some(minco_plugin_object_storage::ObjectByteRange::from(16)),
                if_match: "\"strong-etag\"".into(),
                version_id: Some("version-2".into()),
                download_file_name: Some("report.pdf".into()),
                cache_control: "private, max-age=600, immutable".into(),
                expires_in: TimeDelta::minutes(10),
            },
        )
        .await
        .unwrap();
        assert_eq!(download.method, PresignedMethod::Get);
        assert_eq!(
            download.headers.get("range").map(String::as_str),
            Some("bytes=16-")
        );
        assert_eq!(
            download.headers.get("if-match").map(String::as_str),
            Some("\"strong-etag\"")
        );
        assert!(download.url.contains("versionId=version-2"));
        assert!(format!("{download:?}").contains("[REDACTED PRESIGNED URL]"));

        let checksum = "ab".repeat(32);
        let part = MultipartObjectSigner::sign_multipart_part(
            &adapter,
            SignMultipartPart {
                key,
                upload_id: ProviderMultipartUploadId::parse("provider-upload-secret").unwrap(),
                part_number: 2,
                size_bytes: 16 * 1024 * 1024,
                sha256: checksum.clone(),
                expires_in: TimeDelta::minutes(10),
            },
        )
        .await
        .unwrap();
        assert_eq!(part.method, PresignedMethod::Put);
        assert_eq!(
            part.headers.get("content-length").map(String::as_str),
            Some("16777216")
        );
        assert_eq!(
            part.headers
                .get("x-amz-checksum-sha256")
                .map(String::as_str),
            Some(sha256_base64(&checksum).unwrap().as_str())
        );
        let debug = format!("{part:?}");
        assert!(!debug.contains("provider-upload-secret"));
        assert!(debug.contains("[REDACTED PRESIGNED URL]"));
    }

    #[tokio::test]
    async fn sdk_presigning_never_outlives_temporary_credentials() {
        let credential_expiry = Utc::now() + TimeDelta::minutes(5);
        let adapter = adapter_with_credentials(credentials(None, Some(credential_expiry)));
        let signed = ObjectDownloadSigner::sign_download(
            &adapter,
            SignObjectDownload {
                key: ObjectKey::parse("documents/revision-2").unwrap(),
                range: None,
                if_match: "\"strong-etag\"".into(),
                version_id: None,
                download_file_name: Some("report.pdf".into()),
                cache_control: "private, no-store".into(),
                expires_in: TimeDelta::minutes(10),
            },
        )
        .await
        .unwrap();
        assert!(signed.expires_at <= credential_expiry - CREDENTIAL_EXPIRY_SAFETY_SKEW);
    }

    #[test]
    fn storage_boundaries_reject_ambiguous_provider_values() {
        assert!(validate_bucket("minco-objects").is_ok());
        assert!(validate_bucket("amzn-s3-demo-private").is_err());
        assert!(validate_bucket("amzn_s3_demo_private").is_err());
        assert!(validate_bucket("Minco_Objects").is_err());
        assert!(validate_bucket("192.0.2.1").is_err());
        assert!(validate_bucket("xn--minco").is_err());
        assert_eq!(
            normalize_prefix("/feedback/uploads/").unwrap(),
            "feedback/uploads"
        );
        assert_eq!(normalize_prefix("").unwrap(), "");
        assert!(normalize_prefix("feedback/../private").is_err());
        assert!(valid_endpoint_override("http://127.0.0.1:4566"));
        assert!(valid_endpoint_override(
            "https://s3.ap-southeast-2.amazonaws.com"
        ));
        assert!(!valid_endpoint_override("http://example.com"));
        assert!(!valid_endpoint_override("https://example.com/prefix"));
        for file_name in ["report.pdf", "quarterly report-1_2.pdf"] {
            assert!(validate_download_name(file_name).is_ok(), "{file_name}");
        }
        for file_name in [
            "report\".pdf",
            "report\\.pdf",
            "folder/report.pdf",
            "report;inline.pdf",
            "report\n.pdf",
            "résumé.pdf",
        ] {
            assert!(validate_download_name(file_name).is_err(), "{file_name}");
        }
        assert!(validate_post_size(3, 3).is_ok());
        assert!(matches!(
            validate_post_size(0, 0),
            Err(ObjectStoreError::InvalidMaximumSize)
        ));
        assert!(matches!(
            validate_post_size(1, MAX_SINGLE_POST_SIZE_BYTES + 1),
            Err(ObjectStoreError::ObjectTooLarge)
        ));
        assert_eq!(
            sha256_base64(&"00".repeat(32)).unwrap(),
            STANDARD.encode([0_u8; 32])
        );
        assert!(sha256_base64("invalid").is_err());
    }
}
