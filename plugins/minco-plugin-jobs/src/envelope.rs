//! Bounded, versioned job envelopes.
//!
//! The envelope is the only representation that crosses a queue. It carries
//! operational metadata plus the typed job payload, is closed to unknown
//! fields so unsupported producers fail deterministically, and never exposes
//! payload or metadata values through `Debug`.

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::JobError;
use crate::policy::RetryPolicy;

/// Current envelope wire schema. Envelopes carrying any other schema version
/// are rejected before decode.
pub const JOB_ENVELOPE_SCHEMA_VERSION: u16 = 1;

/// Maximum logical job name length in bytes.
pub const MAX_JOB_NAME_BYTES: usize = 128;
/// Maximum worker-profile identifier length in bytes.
pub const MAX_WORKER_PROFILE_BYTES: usize = 64;
/// Maximum serialized payload bytes. Large content belongs in object storage.
pub const MAX_JOB_PAYLOAD_BYTES: usize = 131_072;
/// Maximum complete serialized envelope bytes, kept below provider ceilings.
pub const MAX_JOB_ENVELOPE_BYTES: usize = 262_144;
/// Maximum metadata entries per envelope.
pub const MAX_JOB_METADATA_ENTRIES: usize = 16;
/// Maximum metadata name length in bytes.
pub const MAX_JOB_METADATA_NAME_BYTES: usize = 64;
/// Maximum metadata value length in bytes.
pub const MAX_JOB_METADATA_VALUE_BYTES: usize = 256;
/// Maximum dedupe/overlap key length in bytes.
pub const MAX_JOB_KEY_BYTES: usize = 128;
/// Maximum partition reference length in bytes.
pub const MAX_JOB_PARTITION_BYTES: usize = 128;
/// Hard ceiling on attempts configured by a retry policy.
pub const MAX_JOB_ATTEMPTS: u32 = 100;

const FORBIDDEN_METADATA_NAME_FRAGMENTS: [&str; 8] = [
    "authorization",
    "cookie",
    "credential",
    "password",
    "secret",
    "token",
    "api-key",
    "api_key",
];

/// A durable typed work command envelope.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobEnvelope {
    pub schema_version: u16,
    pub job_id: Uuid,
    pub job_name: String,
    pub job_version: u16,
    pub payload: serde_json::Value,
    pub worker_profile: String,
    pub created_at: DateTime<Utc>,
    /// Earliest time this job may be delivered or executed.
    pub available_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<DateTime<Utc>>,
    /// Attempt number this delivery refers to; the durable record is
    /// authoritative for the current attempt count.
    pub attempt: u32,
    pub maximum_attempts: u32,
    pub correlation_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedupe_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlap_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
}

impl std::fmt::Debug for JobEnvelope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobEnvelope")
            .field("schema_version", &self.schema_version)
            .field("job_id", &self.job_id)
            .field("job_name", &self.job_name)
            .field("job_version", &self.job_version)
            .field("worker_profile", &self.worker_profile)
            .field("created_at", &self.created_at)
            .field("available_at", &self.available_at)
            .field("deadline", &self.deadline)
            .field("attempt", &self.attempt)
            .field("maximum_attempts", &self.maximum_attempts)
            .field("correlation_id", &self.correlation_id)
            .field("causation_id", &self.causation_id)
            .field(
                "payload_bytes",
                &serde_json::to_string(&self.payload).map_or(0, |encoded| encoded.len()),
            )
            .field("metadata_names", &self.metadata.keys().collect::<Vec<_>>())
            .field("has_dedupe_key", &self.dedupe_key.is_some())
            .field("has_overlap_key", &self.overlap_key.is_some())
            .field("partition", &self.partition)
            .field("retry", &self.retry)
            .finish_non_exhaustive()
    }
}

impl JobEnvelope {
    /// Build an envelope for a typed job payload, minting a fresh `UUIDv7` job
    /// identity. The job name, version and payload limits are validated
    /// before the envelope exists.
    pub fn for_job<J>(
        payload: &J,
        worker_profile: &str,
        correlation_id: Uuid,
    ) -> Result<Self, JobError>
    where
        J: crate::Job,
    {
        let payload = serde_json::to_value(payload).map_err(|error| {
            JobError::InvalidJob(format!("payload serialization failed: {error}"))
        })?;
        Self::for_parts(J::NAME, J::VERSION, payload, worker_profile, correlation_id)
    }

    /// Build an envelope from explicit parts. Useful for adapters decoding
    /// scheduled trigger forms before a typed payload exists.
    pub fn for_parts(
        job_name: impl Into<String>,
        job_version: u16,
        payload: serde_json::Value,
        worker_profile: impl Into<String>,
        correlation_id: Uuid,
    ) -> Result<Self, JobError> {
        let now = Utc::now();
        let envelope = Self {
            schema_version: JOB_ENVELOPE_SCHEMA_VERSION,
            job_id: Uuid::now_v7(),
            job_name: job_name.into(),
            job_version,
            payload,
            worker_profile: worker_profile.into(),
            created_at: now,
            available_at: now,
            deadline: None,
            attempt: 1,
            maximum_attempts: RetryPolicy::default().maximum_attempts,
            correlation_id,
            causation_id: None,
            metadata: BTreeMap::new(),
            dedupe_key: None,
            overlap_key: None,
            partition: None,
            retry: None,
        };
        envelope.validate().map_err(|_| {
            JobError::InvalidJob("job envelope does not satisfy its own limits".into())
        })?;
        Ok(envelope)
    }

    #[must_use]
    pub fn with(mut self, options: JobOptions) -> Self {
        if let Some(deadline) = options.deadline {
            self.deadline = Some(deadline);
        }
        if let Some(at) = options.available_at {
            self.available_at = at;
        }
        if let Some(causation_id) = options.causation_id {
            self.causation_id = Some(causation_id);
        }
        if let Some(partition) = options.partition {
            self.partition = Some(partition);
        }
        if let Some(key) = options.dedupe_key {
            self.dedupe_key = Some(key);
        }
        if let Some(key) = options.overlap_key {
            self.overlap_key = Some(key);
        }
        if let Some(policy) = options.retry {
            self.maximum_attempts = policy.maximum_attempts;
            self.retry = Some(policy);
        }
        for (name, value) in options.metadata {
            self.metadata.insert(name, value);
        }
        self
    }

    /// Validate every string, count and byte limit. Returns the serialized
    /// envelope size on success so callers can enforce the transport bound.
    pub fn validate(&self) -> Result<usize, JobError> {
        if self.schema_version != JOB_ENVELOPE_SCHEMA_VERSION {
            return Err(JobError::UnsupportedEnvelopeSchema(self.schema_version));
        }
        validate_job_name(&self.job_name)?;
        if self.job_version == 0 {
            return Err(JobError::InvalidJob(
                "job version must be at least 1".into(),
            ));
        }
        validate_worker_profile(&self.worker_profile)?;
        let payload_bytes = serde_json::to_vec(&self.payload)
            .map_err(|error| {
                JobError::InvalidJob(format!("payload serialization failed: {error}"))
            })?
            .len();
        if payload_bytes == 0 || payload_bytes > MAX_JOB_PAYLOAD_BYTES {
            return Err(JobError::PayloadTooLarge {
                bytes: payload_bytes,
                maximum: MAX_JOB_PAYLOAD_BYTES,
            });
        }
        if self.maximum_attempts == 0 || self.maximum_attempts > MAX_JOB_ATTEMPTS {
            return Err(JobError::InvalidJob(format!(
                "maximum attempts must be between 1 and {MAX_JOB_ATTEMPTS}"
            )));
        }
        if self.attempt == 0 || self.attempt > self.maximum_attempts {
            return Err(JobError::InvalidJob(
                "attempt must be between 1 and maximum attempts".into(),
            ));
        }
        if let Some(retry) = &self.retry {
            retry.validate()?;
            if retry.maximum_attempts != self.maximum_attempts {
                return Err(JobError::InvalidJob(
                    "retry policy attempts must match envelope maximum attempts".into(),
                ));
            }
        }
        if self.available_at < self.created_at {
            return Err(JobError::InvalidJob(
                "available_at precedes created_at".into(),
            ));
        }
        if self
            .deadline
            .is_some_and(|deadline| deadline <= self.created_at)
        {
            return Err(JobError::InvalidJob(
                "deadline must be after created_at".into(),
            ));
        }
        if self.metadata.len() > MAX_JOB_METADATA_ENTRIES {
            return Err(JobError::InvalidJob(format!(
                "metadata exceeds {MAX_JOB_METADATA_ENTRIES} entries"
            )));
        }
        for (name, value) in &self.metadata {
            validate_metadata_pair(name, value)?;
        }
        if let Some(key) = self.dedupe_key.as_deref() {
            validate_key("dedupe_key", key)?;
        }
        if let Some(key) = self.overlap_key.as_deref() {
            validate_key("overlap_key", key)?;
        }
        if self.partition.as_deref().is_some_and(|partition| {
            partition.is_empty() || partition.len() > MAX_JOB_PARTITION_BYTES
        }) {
            return Err(JobError::InvalidJob(format!(
                "partition must be 1..={MAX_JOB_PARTITION_BYTES} bytes"
            )));
        }
        let encoded = self.to_json_bytes()?;
        if encoded.len() > MAX_JOB_ENVELOPE_BYTES {
            return Err(JobError::EnvelopeTooLarge(encoded.len()));
        }
        Ok(encoded.len())
    }

    /// Serialize to the exact JSON bytes placed on a transport.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, JobError> {
        serde_json::to_vec(self).map_err(|error| {
            JobError::Infrastructure(format!("job envelope serialization failed: {error}"))
        })
    }

    /// Decode closed-shape JSON bytes into an envelope. Unknown fields and
    /// unsupported schema versions fail deterministically.
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, JobError> {
        let envelope: Self = serde_json::from_slice(bytes).map_err(|error| {
            JobError::InvalidTransportMessage(format!("job envelope decode failed: {error}"))
        })?;
        envelope.validate()?;
        Ok(envelope)
    }

    /// The retry policy in force, defaulting to a bounded exponential policy.
    #[must_use]
    pub fn effective_retry(&self) -> RetryPolicy {
        self.retry.clone().unwrap_or_default()
    }
}

/// Optional submission-time configuration applied through
/// [`JobEnvelope::with`].
#[derive(Debug, Clone, Default)]
pub struct JobOptions {
    pub deadline: Option<DateTime<Utc>>,
    pub available_at: Option<DateTime<Utc>>,
    pub causation_id: Option<Uuid>,
    pub partition: Option<String>,
    pub dedupe_key: Option<String>,
    pub overlap_key: Option<String>,
    pub retry: Option<RetryPolicy>,
    pub metadata: BTreeMap<String, String>,
}

impl JobOptions {
    #[must_use]
    pub const fn with_deadline(mut self, deadline: DateTime<Utc>) -> Self {
        self.deadline = Some(deadline);
        self
    }

    #[must_use]
    pub fn with_available_after(mut self, delay: TimeDelta) -> Self {
        self.available_at = Some(Utc::now() + delay);
        self
    }

    #[must_use]
    pub const fn with_causation(mut self, causation_id: Uuid) -> Self {
        self.causation_id = Some(causation_id);
        self
    }

    #[must_use]
    pub fn with_partition(mut self, partition: impl Into<String>) -> Self {
        self.partition = Some(partition.into());
        self
    }

    #[must_use]
    pub fn with_dedupe_key(mut self, key: impl Into<String>) -> Self {
        self.dedupe_key = Some(key.into());
        self
    }

    #[must_use]
    pub fn with_overlap_key(mut self, key: impl Into<String>) -> Self {
        self.overlap_key = Some(key.into());
        self
    }

    #[must_use]
    pub const fn with_retry(mut self, policy: RetryPolicy) -> Self {
        self.retry = Some(policy);
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(name.into(), value.into());
        self
    }
}

/// Validate a stable logical job name.
pub fn validate_job_name(name: &str) -> Result<(), JobError> {
    if name.is_empty() || name.len() > MAX_JOB_NAME_BYTES {
        return Err(JobError::InvalidJob(format!(
            "job name must be 1..={MAX_JOB_NAME_BYTES} bytes"
        )));
    }
    if !name.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'.'
    }) {
        return Err(JobError::InvalidJob(
            "job name may contain only lowercase letters, digits, hyphens and dots".into(),
        ));
    }
    Ok(())
}

/// Validate a stable worker-profile identifier.
pub fn validate_worker_profile(profile: &str) -> Result<(), JobError> {
    if profile.is_empty() || profile.len() > MAX_WORKER_PROFILE_BYTES {
        return Err(JobError::InvalidJob(format!(
            "worker profile must be 1..={MAX_WORKER_PROFILE_BYTES} bytes"
        )));
    }
    if !profile
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(JobError::InvalidJob(
            "worker profile may contain only lowercase letters, digits and hyphens".into(),
        ));
    }
    Ok(())
}

fn validate_metadata_pair(name: &str, value: &str) -> Result<(), JobError> {
    if name.is_empty()
        || name.len() > MAX_JOB_METADATA_NAME_BYTES
        || FORBIDDEN_METADATA_NAME_FRAGMENTS
            .iter()
            .any(|fragment| name.to_ascii_lowercase().contains(fragment))
    {
        return Err(JobError::InvalidJob(format!(
            "metadata name must be 1..={MAX_JOB_METADATA_NAME_BYTES} bytes and not resemble a credential"
        )));
    }
    if !name.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'.'
    }) {
        return Err(JobError::InvalidJob(
            "metadata name may contain only lowercase letters, digits, hyphens and dots".into(),
        ));
    }
    if value.len() > MAX_JOB_METADATA_VALUE_BYTES {
        return Err(JobError::InvalidJob(format!(
            "metadata value must be at most {MAX_JOB_METADATA_VALUE_BYTES} bytes"
        )));
    }
    if value.contains(|character: char| character.is_control()) {
        return Err(JobError::InvalidJob(
            "metadata value contains control characters".into(),
        ));
    }
    Ok(())
}

fn validate_key(field: &str, key: &str) -> Result<(), JobError> {
    if key.is_empty() || key.len() > MAX_JOB_KEY_BYTES {
        return Err(JobError::InvalidJob(format!(
            "{field} must be 1..={MAX_JOB_KEY_BYTES} bytes"
        )));
    }
    if key.contains(|character: char| character.is_control()) {
        return Err(JobError::InvalidJob(format!(
            "{field} contains control characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_envelope_serializes_and_decodes_exactly_once() {
        let envelope = envelope();
        let bytes = envelope.to_json_bytes().expect("serialize envelope");
        let decoded = JobEnvelope::from_json_bytes(&bytes).expect("decode envelope");
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn debug_excludes_payload_and_metadata_values() {
        let envelope =
            envelope().with(JobOptions::default().with_metadata("source", "payload-secret"));
        let text = format!("{envelope:?}");
        assert!(
            !text.contains("payload-secret"),
            "debug leaked a metadata value"
        );
        assert!(!text.contains("\"order-note\""), "debug leaked the payload");
        assert!(
            text.contains("payload_bytes"),
            "debug should show bounded sizes"
        );
        assert!(
            text.contains("metadata_names"),
            "debug should show metadata names only"
        );
    }

    #[test]
    fn unknown_envelope_fields_are_rejected() {
        let mut value = serde_json::to_value(envelope()).expect("serialize");
        let object = value.as_object_mut().expect("object");
        object.insert("surprise".into(), serde_json::Value::Bool(true));
        let bytes = serde_json::to_vec(&value).expect("re-serialize");
        let error = JobEnvelope::from_json_bytes(&bytes)
            .expect_err("closed shape must reject unknown fields");
        assert!(matches!(error, JobError::InvalidTransportMessage(_)));
    }

    #[test]
    fn unsupported_schema_version_fails_deterministically() {
        let mut envelope = envelope();
        envelope.schema_version = 2;
        let error = envelope.validate().expect_err("schema 2 is unsupported");
        assert!(matches!(error, JobError::UnsupportedEnvelopeSchema(2)));
    }

    #[test]
    fn payload_limit_boundary_is_exact() {
        let mut envelope = envelope();
        envelope.payload = serde_json::Value::String("x".repeat(MAX_JOB_PAYLOAD_BYTES - 2));
        envelope.validate().expect("exactly at the limit passes");
        envelope.payload = serde_json::Value::String("x".repeat(MAX_JOB_PAYLOAD_BYTES - 1));
        let error = envelope.validate().expect_err("one byte over fails");
        assert!(matches!(error, JobError::PayloadTooLarge { .. }));
    }

    #[test]
    fn metadata_limits_and_credential_names_are_rejected() {
        let mut envelope = envelope();
        envelope
            .metadata
            .insert("authorization".into(), "Bearer x".into());
        assert!(envelope.validate().is_err());
        envelope.metadata.clear();
        envelope.metadata.insert(
            "source".into(),
            "v".repeat(MAX_JOB_METADATA_VALUE_BYTES + 1),
        );
        assert!(envelope.validate().is_err());
        envelope.metadata.clear();
        envelope.metadata.insert(
            format!("n{}", "-k".repeat(MAX_JOB_METADATA_NAME_BYTES)),
            "v".into(),
        );
        assert!(envelope.validate().is_err());
    }

    #[test]
    fn invalid_job_names_fail() {
        assert!(validate_job_name("").is_err());
        assert!(validate_job_name("Send_Order_Email").is_err());
        assert!(validate_job_name(&"j".repeat(MAX_JOB_NAME_BYTES + 1)).is_err());
        assert!(validate_job_name("orders.send-confirmation").is_ok());
    }

    #[test]
    fn maximal_legal_envelope_stays_below_the_transport_bound() {
        let mut envelope = envelope();
        envelope.payload = serde_json::Value::String("x".repeat(MAX_JOB_PAYLOAD_BYTES - 2));
        for index in 0..MAX_JOB_METADATA_ENTRIES {
            envelope.metadata.insert(
                format!("pad-{index}"),
                "p".repeat(MAX_JOB_METADATA_VALUE_BYTES),
            );
        }
        let size = envelope
            .validate()
            .expect("every component is at its legal maximum");
        assert!(
            size <= MAX_JOB_ENVELOPE_BYTES,
            "a maximal legal envelope must always fit the transport bound: {size}"
        );
    }

    fn envelope() -> JobEnvelope {
        JobEnvelope::for_parts(
            "orders.send-confirmation",
            1,
            serde_json::json!({ "note": "order-note", "order_id": "o-1" }),
            "orders-notifications",
            Uuid::now_v7(),
        )
        .expect("valid envelope")
    }
}
