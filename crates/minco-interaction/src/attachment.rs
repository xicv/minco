use chrono::{DateTime, Utc};
use minco_plugin_object_storage::{
    IssueObjectUpload, IssuedObjectUpload, ObjectKey, ObjectStoreError, ObjectStoreService,
    ObjectUploadError, ObjectUploadService, PendingObjectUpload, PutObject, VerifiedObjectUpload,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    Screenshot,
    Audio,
    File,
}

#[derive(Clone, PartialEq, Eq)]
pub struct AttachmentUpload {
    pub kind: AttachmentKind,
    pub file_name: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

impl fmt::Debug for AttachmentUpload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AttachmentUpload")
            .field("kind", &self.kind)
            .field("file_name", &"[REDACTED]")
            .field("content_type", &self.content_type)
            .field("size_bytes", &self.bytes.len())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentMetadata {
    pub id: Uuid,
    pub kind: AttachmentKind,
    pub object_key: ObjectKey,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub created_at: DateTime<Utc>,
}

impl fmt::Debug for AttachmentMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AttachmentMetadata")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("object_key", &"[REDACTED]")
            .field("file_name", &"[REDACTED]")
            .field("content_type", &self.content_type)
            .field("size_bytes", &self.size_bytes)
            .field("sha256", &self.sha256)
            .field("created_at", &self.created_at)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentLimits {
    pub count: usize,
    pub screenshot_bytes: u64,
    pub audio_bytes: u64,
    pub file_bytes: u64,
    pub aggregate_bytes: u64,
}

impl AttachmentLimits {
    #[must_use]
    pub const fn maximum_for(self, kind: AttachmentKind) -> u64 {
        match kind {
            AttachmentKind::Screenshot => self.screenshot_bytes,
            AttachmentKind::Audio => self.audio_bytes,
            AttachmentKind::File => self.file_bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentPolicy {
    limits: AttachmentLimits,
    allowed_content_types: BTreeMap<AttachmentKind, BTreeSet<String>>,
}

impl AttachmentPolicy {
    pub fn new<I, S>(limits: AttachmentLimits, content_types: I) -> Result<Self, AttachmentError>
    where
        I: IntoIterator<Item = (AttachmentKind, S)>,
        S: AsRef<str>,
    {
        if limits.count > 64
            || limits.aggregate_bytes == 0
            || [
                limits.screenshot_bytes,
                limits.audio_bytes,
                limits.file_bytes,
            ]
            .contains(&0)
        {
            return Err(AttachmentError::InvalidLimits);
        }
        let mut allowed_content_types = BTreeMap::<AttachmentKind, BTreeSet<String>>::new();
        for (kind, value) in content_types {
            let value = normalize_content_type(value.as_ref())?;
            allowed_content_types.entry(kind).or_default().insert(value);
        }
        if [
            AttachmentKind::Screenshot,
            AttachmentKind::Audio,
            AttachmentKind::File,
        ]
        .iter()
        .any(|kind| {
            allowed_content_types
                .get(kind)
                .is_none_or(BTreeSet::is_empty)
        }) {
            return Err(AttachmentError::EmptyContentTypeAllowlist);
        }
        Ok(Self {
            limits,
            allowed_content_types,
        })
    }

    #[must_use]
    pub const fn limits(&self) -> AttachmentLimits {
        self.limits
    }

    pub fn validate_upload(
        &self,
        upload: &AttachmentUpload,
    ) -> Result<ValidatedAttachment, AttachmentError> {
        let size_bytes = u64::try_from(upload.bytes.len())
            .map_err(|_| AttachmentError::AggregateSizeOverflow)?;
        if size_bytes == 0 {
            return Err(AttachmentError::EmptyAttachment);
        }
        let maximum = self.limits.maximum_for(upload.kind);
        if size_bytes > maximum {
            return Err(AttachmentError::AttachmentTooLarge {
                kind: upload.kind,
                actual: size_bytes,
                maximum,
            });
        }
        let content_type = normalize_content_type(&upload.content_type)?;
        if !self
            .allowed_content_types
            .get(&upload.kind)
            .is_some_and(|allowed| allowed.contains(&content_type))
        {
            return Err(AttachmentError::UnsupportedContentType {
                kind: upload.kind,
                content_type,
            });
        }
        Ok(ValidatedAttachment {
            kind: upload.kind,
            file_name: safe_presentation_file_name(&upload.file_name)?,
            content_type,
            size_bytes,
        })
    }

    pub fn validate_batch(
        &self,
        uploads: &[AttachmentUpload],
    ) -> Result<Vec<ValidatedAttachment>, AttachmentError> {
        if uploads.len() > self.limits.count {
            return Err(AttachmentError::TooManyAttachments {
                actual: uploads.len(),
                maximum: self.limits.count,
            });
        }
        let mut aggregate = 0_u64;
        let mut validated = Vec::with_capacity(uploads.len());
        for upload in uploads {
            let item = self.validate_upload(upload)?;
            aggregate = aggregate
                .checked_add(item.size_bytes)
                .ok_or(AttachmentError::AggregateSizeOverflow)?;
            if aggregate > self.limits.aggregate_bytes {
                return Err(AttachmentError::AggregateTooLarge {
                    actual: aggregate,
                    maximum: self.limits.aggregate_bytes,
                });
            }
            validated.push(item);
        }
        Ok(validated)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedAttachment {
    pub kind: AttachmentKind,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct AttachmentService {
    objects: ObjectStoreService,
    policy: AttachmentPolicy,
}

impl AttachmentService {
    pub const fn new(objects: ObjectStoreService, policy: AttachmentPolicy) -> Self {
        Self { objects, policy }
    }

    pub async fn store_small(
        &self,
        namespace: &str,
        owner_id: &str,
        upload: &AttachmentUpload,
        mut attributes: BTreeMap<String, String>,
    ) -> Result<AttachmentMetadata, AttachmentError> {
        let validated = self.policy.validate_upload(upload)?;
        validate_key_segment(namespace)?;
        validate_key_segment(owner_id)?;
        let id = Uuid::now_v7();
        let object_key = ObjectKey::parse(format!("{namespace}/{owner_id}/{id}"))?;
        attributes
            .entry("attachment_id".into())
            .or_insert_with(|| id.to_string());
        let metadata = self
            .objects
            .put(PutObject {
                key: object_key.clone(),
                bytes: upload.bytes.clone(),
                content_type: validated.content_type.clone(),
                attributes,
            })
            .await?;
        Ok(AttachmentMetadata {
            id,
            kind: validated.kind,
            object_key,
            file_name: validated.file_name,
            content_type: validated.content_type,
            size_bytes: metadata.size_bytes,
            sha256: metadata.sha256,
            created_at: metadata.created_at,
        })
    }

    /// Delegates capability issuance to the existing object-storage service.
    pub async fn issue_direct(
        uploads: &ObjectUploadService,
        request: IssueObjectUpload,
    ) -> Result<IssuedObjectUpload, ObjectUploadError> {
        uploads.issue(request).await
    }

    /// Delegates provider metadata verification to the existing service.
    pub async fn verify_direct(
        uploads: &ObjectUploadService,
        pending: &PendingObjectUpload,
    ) -> Result<VerifiedObjectUpload, ObjectUploadError> {
        uploads.verify(pending).await
    }
}

pub fn safe_presentation_file_name(value: &str) -> Result<String, AttachmentError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(AttachmentError::InvalidFileName);
    }
    let safe = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .take(160)
        .collect::<String>();
    if safe.trim_matches(['-', '.']).is_empty() {
        Err(AttachmentError::InvalidFileName)
    } else {
        Ok(safe)
    }
}

fn normalize_content_type(value: &str) -> Result<String, AttachmentError> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty()
        || value.len() > 160
        || value.contains(';')
        || value.chars().any(char::is_control)
        || !value.contains('/')
    {
        return Err(AttachmentError::InvalidContentType);
    }
    Ok(value)
}

fn validate_key_segment(value: &str) -> Result<(), AttachmentError> {
    if value.is_empty()
        || value.len() > 200
        || value.contains('/')
        || matches!(value, "." | "..")
        || value.chars().any(char::is_control)
    {
        Err(AttachmentError::InvalidObjectScope)
    } else {
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AttachmentError {
    #[error("attachment limits are invalid")]
    InvalidLimits,
    #[error("every attachment kind requires an exact non-empty content-type allowlist")]
    EmptyContentTypeAllowlist,
    #[error("attachment is empty")]
    EmptyAttachment,
    #[error("attachment file name is invalid")]
    InvalidFileName,
    #[error("attachment content type is invalid")]
    InvalidContentType,
    #[error("content type {content_type:?} is not allowed for {kind:?}")]
    UnsupportedContentType {
        kind: AttachmentKind,
        content_type: String,
    },
    #[error("attachment is {actual} bytes; maximum for {kind:?} is {maximum}")]
    AttachmentTooLarge {
        kind: AttachmentKind,
        actual: u64,
        maximum: u64,
    },
    #[error("attachment count is {actual}; maximum is {maximum}")]
    TooManyAttachments { actual: usize, maximum: usize },
    #[error("aggregate attachment size overflowed")]
    AggregateSizeOverflow,
    #[error("aggregate attachment size is {actual}; maximum is {maximum}")]
    AggregateTooLarge { actual: u64, maximum: u64 },
    #[error("attachment object scope is invalid")]
    InvalidObjectScope,
    #[error(transparent)]
    ObjectStore(#[from] ObjectStoreError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> AttachmentPolicy {
        AttachmentPolicy::new(
            AttachmentLimits {
                count: 2,
                screenshot_bytes: 10,
                audio_bytes: 20,
                file_bytes: 30,
                aggregate_bytes: 30,
            },
            [
                (AttachmentKind::Screenshot, "image/png"),
                (AttachmentKind::Audio, "audio/webm"),
                (AttachmentKind::File, "application/pdf"),
            ],
        )
        .unwrap()
    }

    #[test]
    fn validates_exact_types_sizes_and_aggregate() {
        let upload = AttachmentUpload {
            kind: AttachmentKind::Screenshot,
            file_name: "screen shot.png".into(),
            content_type: "IMAGE/PNG".into(),
            bytes: vec![1; 10],
        };
        assert_eq!(
            policy().validate_upload(&upload).unwrap().file_name,
            "screen-shot.png"
        );
        let wrong = AttachmentUpload {
            content_type: "image/jpeg".into(),
            ..upload.clone()
        };
        assert!(matches!(
            policy().validate_upload(&wrong),
            Err(AttachmentError::UnsupportedContentType { .. })
        ));
        let aggregate = [
            upload,
            AttachmentUpload {
                kind: AttachmentKind::Audio,
                file_name: "voice.webm".into(),
                content_type: "audio/webm".into(),
                bytes: vec![1; 21],
            },
        ];
        assert!(matches!(
            policy().validate_batch(&aggregate),
            Err(AttachmentError::AttachmentTooLarge { .. })
        ));
        let debug = format!("{:?}", aggregate[0]);
        assert!(!debug.contains("screen shot.png"));
        assert!(!debug.contains("[1, 1"));
    }
}
