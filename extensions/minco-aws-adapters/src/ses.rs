use async_trait::async_trait;
use aws_sdk_sesv2::types::{Body, Content, Destination, EmailContent, Message};
use minco_plugin_notifications::{
    Notification, NotificationChannel, NotificationError, NotificationSink,
};

#[derive(Debug, Clone)]
pub struct SesNotificationSink {
    client: aws_sdk_sesv2::Client,
    from_address: String,
    from_identity_arn: Option<String>,
}

impl SesNotificationSink {
    pub fn new(
        client: aws_sdk_sesv2::Client,
        from_address: impl Into<String>,
        from_identity_arn: Option<String>,
    ) -> Result<Self, NotificationError> {
        let from_address = from_address.into();
        validate_email(&from_address)?;
        if from_identity_arn
            .as_deref()
            .is_some_and(|arn| !arn.starts_with("arn:") || arn.chars().any(char::is_control))
        {
            return Err(NotificationError::Delivery(
                "SES identity ARN is invalid".into(),
            ));
        }
        Ok(Self {
            client,
            from_address,
            from_identity_arn,
        })
    }
}

#[async_trait]
impl NotificationSink for SesNotificationSink {
    async fn send(&self, notification: Notification) -> Result<(), NotificationError> {
        if notification.channel != NotificationChannel::Email {
            return Err(NotificationError::Delivery(
                "SES adapter accepts only email notifications".into(),
            ));
        }
        validate_notification(&notification)?;

        let body_text = email_body(&notification);
        if body_text.len() > 1_000_000 || body_text.chars().any(|character| character == '\0') {
            return Err(NotificationError::Delivery(
                "rendered email body exceeds the Minco delivery boundary".into(),
            ));
        }
        let subject = Content::builder()
            .data(notification.title)
            .charset("UTF-8")
            .build()
            .map_err(|error| NotificationError::Delivery(error.to_string()))?;
        let body = Content::builder()
            .data(body_text)
            .charset("UTF-8")
            .build()
            .map_err(|error| NotificationError::Delivery(error.to_string()))?;
        let message = Message::builder()
            .subject(subject)
            .body(Body::builder().text(body).build())
            .build();
        let mut request = self
            .client
            .send_email()
            .from_email_address(&self.from_address)
            .destination(
                Destination::builder()
                    .to_addresses(notification.recipient)
                    .build(),
            )
            .content(EmailContent::builder().simple(message).build());
        if let Some(identity_arn) = &self.from_identity_arn {
            request = request.from_email_address_identity_arn(identity_arn);
        }
        request.send().await.map_err(|error| {
            NotificationError::Delivery(format!("SES SendEmail failed: {error}"))
        })?;
        Ok(())
    }
}

fn validate_notification(notification: &Notification) -> Result<(), NotificationError> {
    validate_email(&notification.recipient)?;
    if notification.title.trim().is_empty()
        || notification.title.len() > 200
        || notification.title.chars().any(char::is_control)
        || notification.body.len() > 1_000_000
        || notification
            .link
            .as_deref()
            .is_some_and(|link| link.len() > 2048 || link.chars().any(char::is_control))
    {
        return Err(NotificationError::Delivery(
            "email subject or body exceeds the Minco delivery boundary".into(),
        ));
    }
    Ok(())
}

fn validate_email(value: &str) -> Result<(), NotificationError> {
    let Some((local, domain)) = value.split_once('@') else {
        return Err(NotificationError::InvalidRecipient);
    };
    if local.is_empty()
        || domain.is_empty()
        || domain.starts_with('.')
        || domain.ends_with('.')
        || value.matches('@').count() != 1
        || value.len() > 320
        || !value.is_ascii()
        || value
            .chars()
            .any(|character| character.is_control() || character.is_ascii_whitespace())
        || domain.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err(NotificationError::InvalidRecipient);
    }
    Ok(())
}

fn email_body(notification: &Notification) -> String {
    match notification.link.as_deref() {
        Some(link) => format!("{}\n\n{}", notification.body, link),
        None => notification.body.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_rejects_header_injection_and_non_email_channels() {
        assert!(validate_email("person@example.com").is_ok());
        assert!(validate_email("person@example.com\nBcc: attacker@example.com").is_err());
        let notification = Notification::new(
            "topic",
            NotificationChannel::Webhook,
            "person@example.com",
            "Title",
            "Body",
        );
        assert_ne!(notification.channel, NotificationChannel::Email);
    }
}

use aws_sdk_sesv2::{
    config::Config as SesConfig,
    operation::send_email::SendEmailError,
    primitives::Blob,
    types::{MessageTag, RawMessage},
};
use minco_plugin_notifications::{
    MailAddress, MailDeliveryEvent, MailDeliveryEventKind, MailError, MailErrorKind, MailMessage,
    MailReceipt, MailTransport, deterministic_mail_event_id, render_mime,
};
use std::{collections::BTreeMap, time::Duration};
use uuid::Uuid;

const SES_TRANSPORT_NAME: &str = "aws.ses";

#[derive(Debug, Clone)]
pub struct SesMailTransportConfig {
    pub from: MailAddress,
    pub from_identity_arn: Option<String>,
    pub configuration_set: Option<String>,
    pub endpoint_id: Option<String>,
    pub tenant_name: Option<String>,
    pub default_tags: BTreeMap<String, String>,
    pub operation_timeout: Duration,
}

impl SesMailTransportConfig {
    pub fn new(from: MailAddress) -> Result<Self, MailError> {
        from.validate()?;
        Ok(Self {
            from,
            from_identity_arn: None,
            configuration_set: None,
            endpoint_id: None,
            tenant_name: None,
            default_tags: BTreeMap::new(),
            operation_timeout: Duration::from_secs(10),
        })
    }

    pub fn validate(&self) -> Result<(), MailError> {
        self.from.validate()?;
        if self.operation_timeout.is_zero()
            || self.from_identity_arn.as_deref().is_some_and(|value| {
                !value.starts_with("arn:")
                    || value.len() > 2_048
                    || value.chars().any(char::is_control)
            })
            || self
                .configuration_set
                .as_deref()
                .is_some_and(|value| !valid_provider_identifier(value, 64))
            || self
                .endpoint_id
                .as_deref()
                .is_some_and(|value| !valid_provider_identifier(value, 64))
            || self
                .tenant_name
                .as_deref()
                .is_some_and(|value| !valid_provider_identifier(value, 64))
            || self.default_tags.len() > 48
            || self.default_tags.iter().any(|(name, value)| {
                matches!(name.as_str(), "minco_message_id" | "minco_topic")
                    || !valid_ses_tag(name)
                    || !valid_ses_tag(value)
            })
        {
            return Err(MailError::new(
                MailErrorKind::Configuration,
                SES_TRANSPORT_NAME,
                "SES mail transport configuration is invalid",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SesMailTransport {
    client: aws_sdk_sesv2::Client,
    config: SesMailTransportConfig,
}

impl SesMailTransport {
    pub fn new(
        client: aws_sdk_sesv2::Client,
        config: SesMailTransportConfig,
    ) -> Result<Self, MailError> {
        config.validate()?;
        Ok(Self { client, config })
    }

    pub fn from_sdk_config(
        sdk_config: &aws_config::SdkConfig,
        config: SesMailTransportConfig,
    ) -> Result<Self, MailError> {
        config.validate()?;
        let timeout_config = aws_config::timeout::TimeoutConfig::builder()
            .operation_timeout(config.operation_timeout)
            .operation_attempt_timeout(config.operation_timeout)
            .build();
        let service_config = aws_sdk_sesv2::config::Builder::from(sdk_config)
            .retry_config(aws_config::retry::RetryConfig::standard().with_max_attempts(1))
            .timeout_config(timeout_config)
            .build();
        Self::new(aws_sdk_sesv2::Client::from_conf(service_config), config)
    }
}

#[async_trait]
impl MailTransport for SesMailTransport {
    fn name(&self) -> &str {
        SES_TRANSPORT_NAME
    }

    async fn send(
        &self,
        message: &MailMessage,
        attempt: u32,
    ) -> Result<MailReceipt, MailError> {
        message.validate()?;
        let raw = render_mime(message, &self.config.from)?;
        let raw_message = RawMessage::builder()
            .data(Blob::new(raw))
            .build()
            .map_err(|_| {
                MailError::new(
                    MailErrorKind::InvalidMessage,
                    self.name(),
                    "SES raw message could not be constructed",
                )
            })?;
        let destination = Destination::builder()
            .set_to_addresses(Some(
                message.to.iter().map(|value| value.address.clone()).collect(),
            ))
            .set_cc_addresses(Some(
                message.cc.iter().map(|value| value.address.clone()).collect(),
            ))
            .set_bcc_addresses(Some(
                message.bcc.iter().map(|value| value.address.clone()).collect(),
            ))
            .build();
        let tags = ses_tags(message, &self.config)?;
        let mut request = self
            .client
            .send_email()
            .from_email_address(self.config.from.formatted())
            .destination(destination)
            .content(EmailContent::builder().raw(raw_message).build())
            .set_email_tags(Some(tags))
            .set_configuration_set_name(self.config.configuration_set.clone())
            .set_endpoint_id(self.config.endpoint_id.clone())
            .set_tenant_name(self.config.tenant_name.clone());
        if let Some(identity_arn) = &self.config.from_identity_arn {
            request = request.from_email_address_identity_arn(identity_arn);
        }
        let output = request.send().await.map_err(classify_send_error)?;
        let provider_message_id = output.message_id().ok_or_else(|| {
            MailError::new(
                MailErrorKind::Ambiguous,
                self.name(),
                "SES accepted the request without a message identifier",
            )
        })?;
        if provider_message_id.trim().is_empty()
            || provider_message_id.len() > 512
            || provider_message_id.chars().any(char::is_control)
        {
            return Err(MailError::new(
                MailErrorKind::Ambiguous,
                self.name(),
                "SES returned an invalid message identifier",
            ));
        }
        Ok(MailReceipt {
            message_id: message.id,
            transport: self.name().into(),
            provider_message_id: provider_message_id.to_owned(),
            accepted_at: chrono::Utc::now(),
            attempt,
        })
    }
}

fn ses_tags(
    message: &MailMessage,
    config: &SesMailTransportConfig,
) -> Result<Vec<MessageTag>, MailError> {
    let mut values = config.default_tags.clone();
    values.extend(message.tags.clone());
    values.insert("minco_message_id".into(), message.id.to_string());
    values.insert("minco_topic".into(), message.topic.clone());
    values
        .into_iter()
        .map(|(name, value)| {
            MessageTag::builder()
                .name(name)
                .value(value)
                .build()
                .map_err(|_| {
                    MailError::new(
                        MailErrorKind::InvalidMessage,
                        SES_TRANSPORT_NAME,
                        "SES message tags are invalid",
                    )
                })
        })
        .collect()
}

fn classify_send_error(
    error: aws_sdk_sesv2::error::SdkError<SendEmailError>,
) -> MailError {
    let kind = error.as_service_error().map_or(MailErrorKind::Ambiguous, |service| {
        if service.is_too_many_requests_exception() || service.is_limit_exceeded_exception() {
            MailErrorKind::Throttled
        } else if service.is_message_rejected() || service.is_bad_request_exception() {
            MailErrorKind::Rejected
        } else if service.is_mail_from_domain_not_verified_exception()
            || service.is_not_found_exception()
            || service.is_sending_paused_exception()
        {
            MailErrorKind::Configuration
        } else {
            MailErrorKind::Ambiguous
        }
    });
    MailError::new(kind, SES_TRANSPORT_NAME, "SES SendEmail failed")
}

pub fn parse_ses_event(bytes: &[u8]) -> Result<MailDeliveryEvent, MailError> {
    if bytes.is_empty() || bytes.len() > 1_000_000 {
        return Err(MailError::new(
            MailErrorKind::InvalidMessage,
            SES_TRANSPORT_NAME,
            "SES event envelope is empty or too large",
        ));
    }
    let envelope: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| {
        MailError::new(
            MailErrorKind::InvalidMessage,
            SES_TRANSPORT_NAME,
            "SES event envelope is not valid JSON",
        )
    })?;
    let (event, envelope_id) = unwrap_event(envelope)?;
    let event_type = event
        .get("eventType")
        .or_else(|| event.get("notificationType"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid_event("SES event type is missing"))?;
    let mail = event
        .get("mail")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| invalid_event("SES mail event is missing"))?;
    let tags = mail
        .get("tags")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| invalid_event("SES correlation tags are missing"))?;
    let message_id = first_tag(tags, "minco_message_id")?
        .parse::<Uuid>()
        .map_err(|_| invalid_event("SES Minco message ID is invalid"))?;
    let topic = first_tag(tags, "minco_topic")?.to_owned();
    let provider_message_id = mail
        .get("messageId")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let occurred_at = event_timestamp(&event).unwrap_or_else(chrono::Utc::now);
    let kind = map_event_kind(event_type, &event)?;
    let source_event_id = envelope_id.as_deref().map_or_else(
        || {
            deterministic_mail_event_id(&[
                event_type,
                &message_id.to_string(),
                provider_message_id.as_deref().unwrap_or(""),
                &occurred_at.to_rfc3339(),
            ])
        },
        str::to_owned,
    );
    let normalized = MailDeliveryEvent {
        source_event_id,
        message_id,
        topic,
        transport: SES_TRANSPORT_NAME.into(),
        kind,
        occurred_at,
        provider_message_id,
    };
    normalized.validate()?;
    Ok(normalized)
}

fn unwrap_event(
    envelope: serde_json::Value,
) -> Result<(serde_json::Value, Option<String>), MailError> {
    if let Some(message) = envelope.get("Message").and_then(serde_json::Value::as_str) {
        let parsed = serde_json::from_str::<serde_json::Value>(message)
            .map_err(|_| invalid_event("SNS SES message is not valid JSON"))?;
        let envelope_id = envelope
            .get("MessageId")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        return Ok((parsed, envelope_id));
    }
    if let Some(detail) = envelope.get("detail").cloned() {
        let envelope_id = envelope
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        return Ok((detail, envelope_id));
    }
    let envelope_id = envelope
        .get("id")
        .or_else(|| envelope.get("eventId"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    Ok((envelope, envelope_id))
}

fn first_tag<'a>(
    tags: &'a serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Result<&'a str, MailError> {
    let value = tags
        .get(name)
        .and_then(serde_json::Value::as_array)
        .and_then(|values| values.first())
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid_event("SES Minco correlation tag is missing"))?;
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(invalid_event("SES Minco correlation tag is invalid"));
    }
    Ok(value)
}

fn event_timestamp(event: &serde_json::Value) -> Option<chrono::DateTime<chrono::Utc>> {
    [
        event.pointer("/mail/timestamp"),
        event.pointer("/delivery/timestamp"),
        event.pointer("/bounce/timestamp"),
        event.pointer("/complaint/timestamp"),
        event.pointer("/reject/timestamp"),
        event.pointer("/deliveryDelay/timestamp"),
        event.pointer("/renderingFailure/timestamp"),
        event.pointer("/open/timestamp"),
        event.pointer("/click/timestamp"),
    ]
    .into_iter()
    .flatten()
    .find_map(serde_json::Value::as_str)
    .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
    .map(|value| value.with_timezone(&chrono::Utc))
}

fn map_event_kind(
    event_type: &str,
    event: &serde_json::Value,
) -> Result<MailDeliveryEventKind, MailError> {
    match event_type.to_ascii_lowercase().as_str() {
        "delivery" => Ok(MailDeliveryEventKind::Delivered),
        "bounce" => {
            let permanent = event
                .pointer("/bounce/bounceType")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case("Permanent"));
            Ok(if permanent {
                MailDeliveryEventKind::BouncedPermanent
            } else {
                MailDeliveryEventKind::BouncedTransient
            })
        }
        "complaint" => Ok(MailDeliveryEventKind::Complaint),
        "reject" => Ok(MailDeliveryEventKind::Rejected),
        "deliverydelay" | "delivery_delay" => Ok(MailDeliveryEventKind::DeliveryDelayed),
        "renderingfailure" | "rendering_failure" => Ok(MailDeliveryEventKind::RenderingFailed),
        "open" => Ok(MailDeliveryEventKind::Opened),
        "click" => Ok(MailDeliveryEventKind::Clicked),
        "subscription" => Ok(MailDeliveryEventKind::SubscriptionChanged),
        _ => Err(invalid_event("SES event type is not supported")),
    }
}

fn invalid_event(message: &str) -> MailError {
    MailError::new(MailErrorKind::InvalidMessage, SES_TRANSPORT_NAME, message)
}

fn valid_provider_identifier(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn valid_ses_tag(value: &str) -> bool {
    valid_provider_identifier(value, 256)
}

#[cfg(test)]
mod mail_tests {
    use super::*;

    #[test]
    fn direct_delivery_event_is_normalized_without_recipient_data() {
        let message_id = Uuid::now_v7();
        let input = serde_json::json!({
            "eventType": "Delivery",
            "mail": {
                "timestamp": "2026-08-09T00:00:00Z",
                "messageId": "provider-message",
                "destination": ["secret@example.com"],
                "tags": {
                    "minco_message_id": [message_id.to_string()],
                    "minco_topic": ["invoice.ready"]
                }
            },
            "delivery": {"timestamp": "2026-08-09T00:00:01Z"}
        });
        let event = parse_ses_event(&serde_json::to_vec(&input).unwrap()).unwrap();
        assert_eq!(event.message_id, message_id);
        assert_eq!(event.kind, MailDeliveryEventKind::Delivered);
        assert_eq!(event.topic, "invoice.ready");
        assert!(!serde_json::to_string(&event).unwrap().contains("secret@example.com"));
    }

    #[test]
    fn config_rejects_reserved_tags() {
        let mut config =
            SesMailTransportConfig::new(MailAddress::new("no-reply@example.com").unwrap()).unwrap();
        config
            .default_tags
            .insert("minco_message_id".into(), "spoof".into());
        assert!(config.validate().is_err());
    }
}
