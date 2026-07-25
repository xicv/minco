use async_trait::async_trait;
use aws_credential_types::provider::{ProvideCredentials, SharedCredentialsProvider};
use aws_sdk_s3::{
    presigning::PresigningConfig, primitives::ByteStream, types::ServerSideEncryption,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use hmac::{Hmac, Mac};
use minco_plugin_object_storage::{
    ObjectAccessSigner, ObjectKey, ObjectMetadata, ObjectStore, ObjectStoreError, PresignGetObject,
    PresignPutObject, PresignedMethod, PresignedObjectRequest, PutObject, StoredObject,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

type HmacSha256 = Hmac<Sha256>;

const META_SHA256: &str = "minco-sha256";
const META_CREATED_AT: &str = "minco-created-at";
const META_ATTRIBUTES: &str = "minco-attributes";

#[derive(Clone)]
pub struct S3ObjectAdapter {
    client: aws_sdk_s3::Client,
    credentials: SharedCredentialsProvider,
    bucket: String,
    key_prefix: String,
    region: String,
    endpoint_override: Option<String>,
}

impl std::fmt::Debug for S3ObjectAdapter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("S3ObjectAdapter")
            .field("bucket", &self.bucket)
            .field("key_prefix", &self.key_prefix)
            .field("region", &self.region)
            .field("endpoint_override", &self.endpoint_override)
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
        let key_prefix = normalize_prefix(&key_prefix.into())?;
        let region = region.into();
        validate_bucket(&bucket)?;
        if region.trim().is_empty()
            || region.len() > 64
            || !region
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(ObjectStoreError::Store("AWS region is invalid".into()));
        }
        if endpoint_override
            .as_deref()
            .is_some_and(|endpoint| !valid_endpoint_override(endpoint))
        {
            return Err(ObjectStoreError::Store(
                "S3 endpoint override is invalid".into(),
            ));
        }
        Ok(Self {
            client,
            credentials,
            bucket,
            key_prefix,
            region,
            endpoint_override: endpoint_override
                .map(|endpoint| endpoint.trim_end_matches('/').to_owned()),
        })
    }

    fn provider_key(&self, key: &ObjectKey) -> String {
        if self.key_prefix.is_empty() {
            key.as_str().to_owned()
        } else {
            format!("{}/{}", self.key_prefix, key.as_str())
        }
    }

    fn post_url(&self) -> String {
        match &self.endpoint_override {
            Some(endpoint) => format!("{endpoint}/{}", self.bucket),
            None => format!("https://{}.s3.{}.amazonaws.com", self.bucket, self.region),
        }
    }
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
        validate_expiry(request.expires_in)?;
        validate_content_type(&request.content_type)?;
        if request.maximum_size_bytes == 0 {
            return Err(ObjectStoreError::InvalidMaximumSize);
        }
        let credentials = self
            .credentials
            .provide_credentials()
            .await
            .map_err(|error| {
                ObjectStoreError::Store(format!("AWS credentials are unavailable: {error}"))
            })?;
        let now = Utc::now();
        let expires_at = now + request.expires_in;
        let date = now.format("%Y%m%d").to_string();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let scope = format!("{date}/{}/s3/aws4_request", self.region);
        let credential = format!("{}/{}", credentials.access_key_id(), scope);
        let attributes = encode_attributes(&request.attributes)?;
        let key = self.provider_key(&request.key);
        let created_at = now.to_rfc3339_opts(SecondsFormat::Millis, true);
        let mut conditions = vec![
            serde_json::json!({"bucket": self.bucket}),
            serde_json::json!({"key": key}),
            serde_json::json!({"Content-Type": request.content_type}),
            serde_json::json!(["content-length-range", 0, request.maximum_size_bytes]),
            serde_json::json!({"x-amz-algorithm": "AWS4-HMAC-SHA256"}),
            serde_json::json!({"x-amz-credential": credential}),
            serde_json::json!({"x-amz-date": amz_date}),
            serde_json::json!({"x-amz-server-side-encryption": "AES256"}),
            serde_json::json!({"x-amz-meta-minco-attributes": attributes}),
            serde_json::json!({"x-amz-meta-minco-created-at": created_at}),
        ];
        if let Some(token) = credentials.session_token() {
            conditions.push(serde_json::json!({"x-amz-security-token": token}));
        }
        let policy = serde_json::json!({
            "expiration": expires_at.to_rfc3339_opts(SecondsFormat::Millis, true),
            "conditions": conditions
        });
        let encoded_policy = STANDARD.encode(
            serde_json::to_vec(&policy)
                .map_err(|error| ObjectStoreError::Store(error.to_string()))?,
        );
        let signature = post_signature(
            credentials.secret_access_key(),
            &date,
            &self.region,
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
        if let Some(token) = credentials.session_token() {
            form_fields.insert("x-amz-security-token".into(), token.into());
        }
        Ok(PresignedObjectRequest {
            method: PresignedMethod::Post,
            url: self.post_url(),
            headers: BTreeMap::new(),
            form_fields,
            expires_at,
        })
    }

    async fn sign_get(
        &self,
        request: PresignGetObject,
    ) -> Result<PresignedObjectRequest, ObjectStoreError> {
        validate_expiry(request.expires_in)?;
        let seconds = request
            .expires_in
            .num_seconds()
            .try_into()
            .map_err(|_| ObjectStoreError::InvalidExpiry)?;
        let config = PresigningConfig::expires_in(std::time::Duration::from_secs(seconds))
            .map_err(|error| ObjectStoreError::Store(error.to_string()))?;
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
            expires_at: Utc::now() + request.expires_in,
        })
    }
}

fn valid_endpoint_override(endpoint: &str) -> bool {
    crate::validated_service_uri(endpoint).is_some_and(|uri| uri.path() == "/")
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
        && !bucket.starts_with("amzn_s3_demo_")
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

fn validate_download_name(value: &str) -> Result<(), ObjectStoreError> {
    if value.is_empty()
        || value.len() > 255
        || value.chars().any(char::is_control)
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

    #[test]
    fn post_policy_signature_matches_the_aws_sigv4_example_shape() {
        let signature = post_signature("secret", "20260725", "ap-southeast-2", b"policy").unwrap();
        assert_eq!(signature.len(), 64);
        assert!(signature.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn storage_boundaries_reject_ambiguous_provider_values() {
        assert!(validate_bucket("minco-objects").is_ok());
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
        assert!(validate_download_name("report.pdf").is_ok());
        assert!(validate_download_name("report\".pdf").is_err());
    }
}
