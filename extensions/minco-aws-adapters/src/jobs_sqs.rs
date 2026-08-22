//! SQS job dispatch: publishes serialized job envelopes to a selected queue.
//!
//! The queue message is delivery, never authoritative state. Queue-target
//! and message-body validation are shared with the event publisher so one
//! set of provider rules governs both transports.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use minco_plugin_jobs::{JobDelivery, JobDispatcher, JobEnvelope, JobError};

/// Per-message delays (`DelaySeconds`) are capped at fifteen minutes and are
/// unsupported on FIFO queues; longer waits belong to durable publication.
pub const SQS_MAX_DELAY_SECONDS: i64 = 900;

#[derive(Debug, Clone)]
pub struct SqsJobDispatcher {
    client: aws_sdk_sqs::Client,
    queue_url: String,
    include_message_group: bool,
    fifo: bool,
}

impl SqsJobDispatcher {
    /// Create a dispatcher for one queue. The URL must be an exact HTTPS (or
    /// explicit loopback) SQS target whose queue name matches the FIFO mode,
    /// mirroring the event publisher's validation.
    pub fn new(
        client: aws_sdk_sqs::Client,
        queue_url: impl Into<String>,
        fifo: bool,
    ) -> Result<Self, JobError> {
        let queue_url = queue_url.into();
        let uri = crate::validated_service_uri(&queue_url);
        let queue_name = uri
            .as_ref()
            .and_then(|uri| uri.path().rsplit('/').next())
            .filter(|name| !name.is_empty());
        let mismatch = queue_name.is_none_or(|name| name.strip_suffix(".fifo").is_some() != fifo);
        if mismatch {
            return Err(JobError::InvalidJob(
                "SQS queue URL is invalid or does not match FIFO mode".into(),
            ));
        }
        Ok(Self {
            client,
            queue_url,
            include_message_group: fifo,
            fifo,
        })
    }

    /// Adds `MessageGroupId` to standard queues as an opt-in fair-queue
    /// boundary. FIFO queues always include it.
    #[must_use]
    pub const fn with_fair_queue_groups(mut self, enabled: bool) -> Self {
        self.include_message_group = self.fifo || enabled;
        self
    }

    fn validate_target(envelope: &JobEnvelope) -> Result<(), JobError> {
        let body = String::from_utf8(envelope.to_json_bytes()?)
            .map_err(|_| JobError::InvalidTransportMessage("job envelope is not UTF-8".into()))?;
        if body.is_empty()
            || body.len() > super::sqs::SQS_MAX_MESSAGE_BYTES
            || !body.chars().all(super::sqs::is_sqs_character)
        {
            return Err(JobError::InvalidTransportMessage(
                "serialized job envelope exceeds SQS limits or contains unsupported characters"
                    .into(),
            ));
        }
        Ok(())
    }
}

/// Compute the per-message delay for a queued dispatch.
///
/// Durable dispatch never runs early, so only explicitly delayed queued
/// submissions reach a positive delay here. Delays beyond the provider
/// range, or any delay on a FIFO queue, fail before provider contact.
pub fn delayed_dispatch_seconds(
    envelope: &JobEnvelope,
    now: DateTime<Utc>,
    fifo: bool,
) -> Result<Option<i64>, JobError> {
    let seconds = (envelope.available_at - now).num_seconds();
    if seconds <= 0 {
        return Ok(None);
    }
    if seconds > SQS_MAX_DELAY_SECONDS {
        return Err(JobError::InvalidJob(format!(
            "queued delay of {seconds}s exceeds the SQS maximum of {SQS_MAX_DELAY_SECONDS}s; use durable dispatch"
        )));
    }
    if fifo {
        return Err(JobError::InvalidJob(
            "FIFO queues do not support per-message delays; use durable dispatch".into(),
        ));
    }
    Ok(Some(seconds))
}

/// Deterministic FIFO grouping for a job: the partition, then the overlap
/// key, then the correlation identity — bounded to the provider limit.
#[must_use]
pub fn job_message_group(envelope: &JobEnvelope) -> String {
    let candidate = envelope
        .partition
        .clone()
        .or_else(|| envelope.overlap_key.clone())
        .unwrap_or_else(|| envelope.correlation_id.to_string());
    if candidate.len() <= 128 && candidate.is_ascii() {
        candidate
    } else {
        envelope.correlation_id.to_string()
    }
}

#[async_trait]
impl JobDispatcher for SqsJobDispatcher {
    async fn dispatch(&self, delivery: &JobDelivery, now: DateTime<Utc>) -> Result<(), JobError> {
        let envelope = &delivery.envelope;
        Self::validate_target(envelope)?;
        let delay = delayed_dispatch_seconds(envelope, now, self.fifo)?;
        let mut request = self
            .client
            .send_message()
            .queue_url(&self.queue_url)
            .message_body(String::from_utf8_lossy(&envelope.to_json_bytes()?).into_owned());
        if let Some(seconds) = delay {
            request = request.delay_seconds(i32::try_from(seconds).unwrap_or(0));
        }
        if self.include_message_group {
            request = request.message_group_id(job_message_group(envelope));
        }
        if self.fifo {
            // The publication identity — never the job identity — is the
            // FIFO deduplication identity: an ambiguous resend of one send
            // is suppressed by the provider, while a new retry generation
            // of the same job is not.
            request = request.message_deduplication_id(delivery.publication_id.to_string());
        }
        request.send().await.map_err(|error| {
            JobError::Infrastructure(format!("SQS SendMessage failed: {error}"))
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;
    use minco_plugin_jobs::JobOptions;

    fn inert_client() -> aws_sdk_sqs::Client {
        aws_sdk_sqs::Client::from_conf(
            aws_sdk_sqs::Config::builder()
                .behavior_version_latest()
                .build(),
        )
    }

    fn envelope() -> JobEnvelope {
        JobEnvelope::for_parts(
            "orders.send-confirmation",
            1,
            serde_json::json!({ "order_id": "o-1" }),
            "orders-notifications",
            uuid::Uuid::now_v7(),
        )
        .expect("valid envelope")
    }

    #[test]
    fn queue_url_validation_rejects_ambiguous_or_insecure_targets() {
        let client = inert_client();
        assert!(
            SqsJobDispatcher::new(
                client.clone(),
                "https://sqs.ap-southeast-2.amazonaws.com/123456789012/minco-jobs",
                false
            )
            .is_ok()
        );
        assert!(
            SqsJobDispatcher::new(
                client.clone(),
                "https://sqs.ap-southeast-2.amazonaws.com/123456789012/minco-jobs.fifo",
                true
            )
            .is_ok()
        );
        for invalid in [
            "minco-jobs",
            "https://user@sqs.example.com/123456789012/minco-jobs",
            "https://sqs.example.com/123456789012/minco-jobs?x=1",
            "http://sqs.example.com/123456789012/minco-jobs",
        ] {
            assert!(
                SqsJobDispatcher::new(client.clone(), invalid, false).is_err(),
                "URL must be rejected: {invalid}"
            );
        }
        assert!(
            SqsJobDispatcher::new(
                client,
                "https://sqs.ap-southeast-2.amazonaws.com/123456789012/minco-jobs",
                true
            )
            .is_err(),
            "FIFO mode must require a .fifo queue name"
        );
    }

    #[test]
    fn standard_delayed_dispatch_respects_the_provider_range() {
        let now = Utc::now();
        let immediate = envelope();
        assert_eq!(
            delayed_dispatch_seconds(&immediate, now, false).expect("immediate"),
            None
        );
        let bounded =
            envelope().with(JobOptions::default().with_available_after(TimeDelta::seconds(300)));
        assert_eq!(
            delayed_dispatch_seconds(&bounded, now, false).expect("within range"),
            Some(300)
        );
        let over =
            envelope().with(JobOptions::default().with_available_after(TimeDelta::seconds(901)));
        let error = delayed_dispatch_seconds(&over, now, false).expect_err("beyond range");
        assert!(matches!(error, JobError::InvalidJob(_)), "{error:?}");
    }

    #[test]
    fn fifo_invalid_delay_fails_before_provider_contact() {
        let now = Utc::now();
        let delayed =
            envelope().with(JobOptions::default().with_available_after(TimeDelta::seconds(10)));
        let error = delayed_dispatch_seconds(&delayed, now, true).expect_err("fifo rejects delay");
        assert!(matches!(error, JobError::InvalidJob(_)), "{error:?}");
        let immediate = envelope();
        assert_eq!(
            delayed_dispatch_seconds(&immediate, now, true).expect("fifo immediate"),
            None
        );
    }

    #[test]
    fn message_groups_and_dedup_identity_are_deterministic() {
        let base = envelope();
        assert_eq!(job_message_group(&base), base.correlation_id.to_string());
        let partitioned = envelope().with(JobOptions::default().with_partition("tenant-a"));
        assert_eq!(job_message_group(&partitioned), "tenant-a");
        let overlapped =
            envelope().with(JobOptions::default().with_overlap_key("orders.confirm:o-1"));
        assert_eq!(job_message_group(&overlapped), "orders.confirm:o-1");
        let long = envelope().with(JobOptions::default().with_partition("p".repeat(200)));
        assert_eq!(
            job_message_group(&long),
            long.correlation_id.to_string(),
            "oversized groups fall back to the correlation identity"
        );
    }

    #[tokio::test]
    async fn provider_error_carries_no_payload() {
        let dispatcher = SqsJobDispatcher::new(
            inert_client(),
            "https://sqs.ap-southeast-2.amazonaws.com/123456789012/minco-jobs",
            false,
        )
        .expect("dispatcher");
        let envelope = envelope();
        let delivery = minco_plugin_jobs::JobDelivery {
            envelope,
            publication_id: uuid::Uuid::now_v7(),
        };
        let error = dispatcher
            .dispatch(&delivery, Utc::now())
            .await
            .expect_err("inert client cannot send");
        let rendered = format!("{error}");
        assert!(
            !rendered.contains("order-note") && !rendered.contains("o-1"),
            "errors must not leak payloads"
        );
    }
}
