use async_trait::async_trait;
use minco_plugin_events::{DomainEvent, EventError, EventPublisher};

pub(crate) const SQS_MAX_MESSAGE_BYTES: usize = 1_048_576;

#[derive(Debug, Clone)]
pub struct SqsEventPublisher {
    client: aws_sdk_sqs::Client,
    queue_url: String,
    include_message_group: bool,
    fifo: bool,
}

impl SqsEventPublisher {
    pub fn new(
        client: aws_sdk_sqs::Client,
        queue_url: impl Into<String>,
        fifo: bool,
    ) -> Result<Self, EventError> {
        let queue_url = queue_url.into();
        let uri = crate::validated_service_uri(&queue_url);
        let queue_name = uri
            .as_ref()
            .and_then(|uri| uri.path().rsplit('/').next())
            .filter(|name| !name.is_empty());
        if queue_name.is_none_or(|name| name.strip_suffix(".fifo").is_some() != fifo) {
            return Err(EventError::Infrastructure(
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

    /// Adds `MessageGroupId` to standard queues as an opt-in fair-queue tenant
    /// boundary. FIFO queues always include it.
    #[must_use]
    pub const fn with_fair_queue_groups(mut self, enabled: bool) -> Self {
        self.include_message_group = self.fifo || enabled;
        self
    }
}

#[async_trait]
impl EventPublisher for SqsEventPublisher {
    async fn publish(&self, event: &DomainEvent) -> Result<(), EventError> {
        validate_event(event)?;
        let body = serde_json::to_string(event)
            .map_err(|error| EventError::Infrastructure(error.to_string()))?;
        validate_message_body(&body)?;

        let group = message_group(event);
        let mut request = self
            .client
            .send_message()
            .queue_url(&self.queue_url)
            .message_body(body);
        if self.include_message_group {
            request = request.message_group_id(group);
        }
        if self.fifo {
            request = request.message_deduplication_id(event.id.to_string());
        }
        request.send().await.map_err(|error| {
            EventError::Infrastructure(format!("SQS SendMessage failed: {error}"))
        })?;
        Ok(())
    }
}

fn validate_event(event: &DomainEvent) -> Result<(), EventError> {
    if event.event_type.trim().is_empty()
        || event.aggregate_type.trim().is_empty()
        || event.aggregate_id.trim().is_empty()
    {
        return Err(EventError::InvalidEvent);
    }
    Ok(())
}

pub(crate) fn validate_message_body(body: &str) -> Result<(), EventError> {
    if body.is_empty() || body.len() > SQS_MAX_MESSAGE_BYTES || !body.chars().all(is_sqs_character)
    {
        return Err(EventError::Infrastructure(
            "serialized event exceeds SQS limits or contains unsupported characters".into(),
        ));
    }
    Ok(())
}

pub(crate) const fn is_sqs_character(character: char) -> bool {
    matches!(character, '\u{9}' | '\u{A}' | '\u{D}')
        || matches!(
            character as u32,
            0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x0010_FFFF
        )
}

fn message_group(event: &DomainEvent) -> String {
    let value = format!("{}:{}", event.aggregate_type, event.aggregate_id);
    if value.len() <= 128 && value.is_ascii() {
        value
    } else {
        event.correlation_id.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::BTreeMap;
    use uuid::Uuid;

    fn client() -> aws_sdk_sqs::Client {
        aws_sdk_sqs::Client::from_conf(
            aws_sdk_sqs::Config::builder()
                .behavior_version_latest()
                .build(),
        )
    }

    fn event() -> DomainEvent {
        DomainEvent {
            id: Uuid::now_v7(),
            event_type: "order.placed".into(),
            aggregate_type: "order".into(),
            aggregate_id: "order-1".into(),
            correlation_id: Uuid::now_v7(),
            occurred_at: Utc::now(),
            payload: serde_json::json!({"total": 10}),
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn queue_limits_and_groups_are_deterministic() {
        let event = event();
        assert_eq!(message_group(&event), "order:order-1");
        assert!(validate_message_body(&serde_json::to_string(&event).unwrap()).is_ok());
        assert!(validate_message_body(&"x".repeat(SQS_MAX_MESSAGE_BYTES + 1)).is_err());
        assert!(validate_message_body("\u{ffff}").is_err());
    }

    #[test]
    fn queue_url_validation_rejects_ambiguous_or_insecure_targets() {
        assert!(
            SqsEventPublisher::new(
                client(),
                "https://sqs.ap-southeast-2.amazonaws.com/123456789012/minco-events",
                false,
            )
            .is_ok()
        );
        for invalid in [
            "queue-name",
            "https://user@sqs.ap-southeast-2.amazonaws.com/queue",
            "https://sqs.ap-southeast-2.amazonaws.com/queue?token=value",
            "http://sqs.ap-southeast-2.amazonaws.com/queue",
            "https://sqs.ap-southeast-2.amazonaws.com/123456789012/minco-events.fifo",
        ] {
            assert!(
                SqsEventPublisher::new(client(), invalid, false,).is_err(),
                "{invalid}"
            );
        }
        assert!(
            SqsEventPublisher::new(
                client(),
                "https://sqs.ap-southeast-2.amazonaws.com/123456789012/minco-events.fifo",
                true,
            )
            .is_ok()
        );
        assert!(
            SqsEventPublisher::new(
                client(),
                "https://sqs.ap-southeast-2.amazonaws.com/123456789012/minco-events",
                true,
            )
            .is_err()
        );
    }
}
