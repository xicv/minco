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
            .map_err(|_| NotificationError::Delivery("SES subject is invalid".into()))?;
        let body = Content::builder()
            .data(body_text)
            .charset("UTF-8")
            .build()
            .map_err(|_| NotificationError::Delivery("SES body is invalid".into()))?;
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
        request
            .send()
            .await
            .map_err(|_| NotificationError::Delivery("SES SendEmail failed".into()))?;
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
    operation::send_email::SendEmailError,
    primitives::Blob,
    types::{MessageTag, RawMessage},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use minco_plugin_notifications::{
    MailAddress, MailDeliveryEvent, MailDeliveryEventKind, MailError, MailErrorKind, MailMessage,
    MailReceipt, MailTransport, deterministic_mail_event_id, render_mime,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
    time::Duration,
};
use uuid::Uuid;

const SES_TRANSPORT_NAME: &str = "aws.ses";
const MAX_SES_MESSAGE_TAGS: usize = 50;
const MAX_SES_EVENT_BYTES: usize = 1_000_000;
const SES_TOPIC_ENCODING_PREFIX: &str = "b64_";

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
            || self.default_tags.len() > MAX_SES_MESSAGE_TAGS - 2
            || self.default_tags.iter().any(|(name, value)| {
                ["minco_message_id", "minco_topic"]
                    .iter()
                    .any(|reserved| name.eq_ignore_ascii_case(reserved))
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

#[derive(Clone)]
pub struct SesMailTransport {
    executor: Arc<dyn SesSendEmailExecutor>,
    config: SesMailTransportConfig,
}

impl SesMailTransport {
    fn from_executor(
        executor: Arc<dyn SesSendEmailExecutor>,
        config: SesMailTransportConfig,
    ) -> Result<Self, MailError> {
        config.validate()?;
        Ok(Self { executor, config })
    }

    pub fn from_sdk_config(
        sdk_config: &aws_config::SdkConfig,
        config: SesMailTransportConfig,
    ) -> Result<Self, MailError> {
        config.validate()?;
        let service_config = ses_service_config(sdk_config, config.operation_timeout);
        Self::from_executor(
            Arc::new(AwsSesSendEmailExecutor {
                client: aws_sdk_sesv2::Client::from_conf(service_config),
            }),
            config,
        )
    }
}

fn ses_service_config(
    sdk_config: &aws_config::SdkConfig,
    operation_timeout: Duration,
) -> aws_sdk_sesv2::Config {
    let timeout_config = aws_config::timeout::TimeoutConfig::builder()
        .operation_timeout(operation_timeout)
        .operation_attempt_timeout(operation_timeout)
        .build();
    aws_sdk_sesv2::config::Builder::from(sdk_config)
        .retry_config(aws_config::retry::RetryConfig::standard().with_max_attempts(1))
        .timeout_config(timeout_config)
        .build()
}

impl fmt::Debug for SesMailTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SesMailTransport")
            .field("executor", &self.executor)
            .field("config", &self.config)
            .finish()
    }
}

#[derive(Clone)]
struct SesSendEmailRequest {
    from_email_address: String,
    from_identity_arn: Option<String>,
    to_addresses: Vec<String>,
    cc_addresses: Vec<String>,
    bcc_addresses: Vec<String>,
    raw_message: Vec<u8>,
    tags: Vec<MessageTag>,
    configuration_set: Option<String>,
    endpoint_id: Option<String>,
    tenant_name: Option<String>,
}

impl fmt::Debug for SesSendEmailRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SesSendEmailRequest")
            .field("from_email_address", &"[REDACTED]")
            .field(
                "from_identity_arn",
                &self.from_identity_arn.as_ref().map(|_| "[SET]"),
            )
            .field("to_count", &self.to_addresses.len())
            .field("cc_count", &self.cc_addresses.len())
            .field("bcc_count", &self.bcc_addresses.len())
            .field("raw_message_bytes", &self.raw_message.len())
            .field("tag_count", &self.tags.len())
            .field("configuration_set", &self.configuration_set)
            .field("endpoint_id", &self.endpoint_id)
            .field("tenant_name", &self.tenant_name)
            .finish()
    }
}

#[async_trait]
trait SesSendEmailExecutor: Send + Sync + fmt::Debug {
    async fn send(&self, request: &SesSendEmailRequest) -> Result<Option<String>, MailError>;
}

#[derive(Debug)]
struct AwsSesSendEmailExecutor {
    client: aws_sdk_sesv2::Client,
}

#[async_trait]
impl SesSendEmailExecutor for AwsSesSendEmailExecutor {
    async fn send(&self, request: &SesSendEmailRequest) -> Result<Option<String>, MailError> {
        let raw_message = RawMessage::builder()
            .data(Blob::new(request.raw_message.clone()))
            .build()
            .map_err(|_| {
                MailError::new(
                    MailErrorKind::InvalidMessage,
                    SES_TRANSPORT_NAME,
                    "SES raw message could not be constructed",
                )
            })?;
        let destination = Destination::builder()
            .set_to_addresses(Some(request.to_addresses.clone()))
            .set_cc_addresses(Some(request.cc_addresses.clone()))
            .set_bcc_addresses(Some(request.bcc_addresses.clone()))
            .build();
        let mut operation = self
            .client
            .send_email()
            .from_email_address(&request.from_email_address)
            .destination(destination)
            .content(EmailContent::builder().raw(raw_message).build())
            .set_email_tags(Some(request.tags.clone()))
            .set_configuration_set_name(request.configuration_set.clone())
            .set_endpoint_id(request.endpoint_id.clone())
            .set_tenant_name(request.tenant_name.clone());
        if let Some(identity_arn) = &request.from_identity_arn {
            operation = operation.from_email_address_identity_arn(identity_arn);
        }
        operation
            .send()
            .await
            .map(|output| output.message_id().map(str::to_owned))
            .map_err(|error| classify_send_error(&error))
    }
}

#[async_trait]
impl MailTransport for SesMailTransport {
    fn name(&self) -> &str {
        SES_TRANSPORT_NAME
    }

    async fn send(&self, message: &MailMessage, attempt: u32) -> Result<MailReceipt, MailError> {
        message.validate()?;
        let raw = render_mime(message, &self.config.from)?;
        let tags = ses_tags(message, &self.config)?;
        let request = SesSendEmailRequest {
            from_email_address: self.config.from.formatted(),
            from_identity_arn: self.config.from_identity_arn.clone(),
            to_addresses: message
                .to
                .iter()
                .map(|value| value.address.clone())
                .collect(),
            cc_addresses: message
                .cc
                .iter()
                .map(|value| value.address.clone())
                .collect(),
            bcc_addresses: message
                .bcc
                .iter()
                .map(|value| value.address.clone())
                .collect(),
            raw_message: raw,
            tags,
            configuration_set: self.config.configuration_set.clone(),
            endpoint_id: self.config.endpoint_id.clone(),
            tenant_name: self.config.tenant_name.clone(),
        };
        let provider_message_id = self.executor.send(&request).await?.ok_or_else(|| {
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
            provider_message_id,
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
    values.insert("minco_topic".into(), encode_ses_topic(&message.topic));
    if values.len() > MAX_SES_MESSAGE_TAGS
        || values
            .iter()
            .any(|(name, value)| !valid_ses_tag(name) || !valid_ses_tag(value))
    {
        return Err(MailError::new(
            MailErrorKind::InvalidMessage,
            SES_TRANSPORT_NAME,
            "merged SES message tags are invalid or exceed the 50-tag boundary",
        ));
    }
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

fn encode_ses_topic(topic: &str) -> String {
    format!(
        "{SES_TOPIC_ENCODING_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(topic.as_bytes())
    )
}

fn decode_ses_topic(encoded: &str) -> Result<String, MailError> {
    let payload = encoded
        .strip_prefix(SES_TOPIC_ENCODING_PREFIX)
        .ok_or_else(|| invalid_event("SES Minco topic encoding is invalid"))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| invalid_event("SES Minco topic encoding is invalid"))?;
    let topic = String::from_utf8(bytes)
        .map_err(|_| invalid_event("SES Minco topic encoding is invalid"))?;
    if encode_ses_topic(&topic) != encoded || !valid_minco_topic(&topic) {
        return Err(invalid_event("SES Minco topic encoding is invalid"));
    }
    Ok(topic)
}

fn classify_send_error(error: &aws_sdk_sesv2::error::SdkError<SendEmailError>) -> MailError {
    let kind = error
        .as_service_error()
        .map_or(MailErrorKind::Ambiguous, classify_send_service_error);
    MailError::new(kind, SES_TRANSPORT_NAME, "SES SendEmail failed")
}

fn classify_send_service_error(service: &SendEmailError) -> MailErrorKind {
    if service.is_too_many_requests_exception() || service.is_limit_exceeded_exception() {
        MailErrorKind::Throttled
    } else if service.is_message_rejected() || service.is_bad_request_exception() {
        MailErrorKind::Rejected
    } else if service.is_mail_from_domain_not_verified_exception()
        || service.is_not_found_exception()
        || service.is_sending_paused_exception()
        || service.is_account_suspended_exception()
    {
        MailErrorKind::Configuration
    } else {
        MailErrorKind::Ambiguous
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SesEventTrustPolicy {
    Sns {
        expected_topic_arn: String,
    },
    EventBridge {
        expected_account: String,
        expected_region: String,
        allowed_detail_types: BTreeSet<String>,
        expected_resource_arn: Option<String>,
    },
}

impl SesEventTrustPolicy {
    pub fn sns(expected_topic_arn: impl Into<String>) -> Result<Self, MailError> {
        let expected_topic_arn = expected_topic_arn.into();
        if !valid_sns_topic_arn(&expected_topic_arn)
            || expected_topic_arn.len() > 2_048
            || expected_topic_arn.chars().any(char::is_control)
        {
            return Err(invalid_event("expected SNS topic ARN is invalid"));
        }
        Ok(Self::Sns { expected_topic_arn })
    }

    pub fn event_bridge(
        expected_account: impl Into<String>,
        expected_region: impl Into<String>,
        allowed_detail_types: impl IntoIterator<Item = String>,
    ) -> Result<Self, MailError> {
        let expected_account = expected_account.into();
        let expected_region = expected_region.into();
        let allowed_detail_types = allowed_detail_types.into_iter().collect::<BTreeSet<_>>();
        if expected_account.len() != 12
            || !expected_account.bytes().all(|byte| byte.is_ascii_digit())
            || !valid_provider_identifier(&expected_region, 64)
            || allowed_detail_types.is_empty()
            || allowed_detail_types.iter().any(|value| {
                value.is_empty() || value.len() > 128 || value.chars().any(char::is_control)
            })
        {
            return Err(invalid_event("expected EventBridge boundary is invalid"));
        }
        Ok(Self::EventBridge {
            expected_account,
            expected_region,
            allowed_detail_types,
            expected_resource_arn: None,
        })
    }

    pub fn with_eventbridge_resource(
        mut self,
        expected_resource_arn: impl Into<String>,
    ) -> Result<Self, MailError> {
        let expected_resource_arn = expected_resource_arn.into();
        if !expected_resource_arn.starts_with("arn:")
            || expected_resource_arn.len() > 2_048
            || expected_resource_arn.chars().any(char::is_control)
        {
            return Err(invalid_event(
                "expected EventBridge resource ARN is invalid",
            ));
        }
        let Self::EventBridge {
            expected_resource_arn: target,
            ..
        } = &mut self
        else {
            return Err(invalid_event(
                "EventBridge resource policy requires an EventBridge trust policy",
            ));
        };
        *target = Some(expected_resource_arn);
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SesEventEnvelopeKind {
    Sns,
    EventBridge,
}

pub struct SesEventEnvelope<'a> {
    kind: SesEventEnvelopeKind,
    raw: &'a [u8],
    envelope_id: &'a str,
}

impl SesEventEnvelope<'_> {
    pub const fn kind(&self) -> SesEventEnvelopeKind {
        self.kind
    }

    pub const fn raw(&self) -> &[u8] {
        self.raw
    }

    pub const fn envelope_id(&self) -> &str {
        self.envelope_id
    }
}

impl fmt::Debug for SesEventEnvelope<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SesEventEnvelope")
            .field("kind", &self.kind)
            .field("raw_bytes", &self.raw.len())
            .field("envelope_id", &"[REDACTED]")
            .finish()
    }
}

pub trait SesEventEnvelopeVerifier: Send + Sync + fmt::Debug {
    fn verify(&self, envelope: &SesEventEnvelope<'_>) -> Result<(), MailError>;
}

/// Authenticates an SNS or `EventBridge` wrapper before normalizing its SES event.
///
/// An SNS verifier must validate the AWS signature and certificate URL. The exact
/// `TopicArn` is checked here. `EventBridge` callers must establish the selected
/// rule/bus identity at the invocation boundary; this function additionally
/// checks source, detail type, account, Region, and an optional resource ARN.
pub fn verify_and_normalize_ses_event(
    bytes: &[u8],
    policy: &SesEventTrustPolicy,
    verifier: &dyn SesEventEnvelopeVerifier,
) -> Result<MailDeliveryEvent, MailError> {
    let envelope = parse_event_json(bytes)?;
    match policy {
        SesEventTrustPolicy::Sns { expected_topic_arn } => {
            let envelope_object = envelope
                .as_object()
                .ok_or_else(|| invalid_event("SNS SES envelope is invalid"))?;
            let message_type = envelope_string(envelope_object, "Type")?;
            let topic_arn = envelope_string(envelope_object, "TopicArn")?;
            let envelope_id = envelope_string(envelope_object, "MessageId")?;
            if message_type != "Notification" || topic_arn != expected_topic_arn {
                return Err(invalid_event("SNS SES trust boundary did not match"));
            }
            verifier.verify(&SesEventEnvelope {
                kind: SesEventEnvelopeKind::Sns,
                raw: bytes,
                envelope_id,
            })?;
            let message = envelope_object
                .get("Message")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty() && value.len() <= MAX_SES_EVENT_BYTES)
                .ok_or_else(|| invalid_event("SNS SES message is invalid"))?;
            let event = serde_json::from_str::<serde_json::Value>(message)
                .map_err(|_| invalid_event("SNS SES message is not valid JSON"))?;
            normalize_ses_event(&event, Some(("aws.ses.sns", envelope_id)))
        }
        SesEventTrustPolicy::EventBridge {
            expected_account,
            expected_region,
            allowed_detail_types,
            expected_resource_arn,
        } => {
            let envelope_object = envelope
                .as_object()
                .ok_or_else(|| invalid_event("EventBridge SES envelope is invalid"))?;
            let source = envelope_string(envelope_object, "source")?;
            let detail_type = envelope_string(envelope_object, "detail-type")?;
            let account = envelope_string(envelope_object, "account")?;
            let region = envelope_string(envelope_object, "region")?;
            let envelope_id = envelope_string(envelope_object, "id")?;
            if source != "aws.ses"
                || account != expected_account
                || region != expected_region
                || !allowed_detail_types.contains(detail_type)
            {
                return Err(invalid_event(
                    "EventBridge SES trust boundary did not match",
                ));
            }
            if let Some(expected_resource_arn) = expected_resource_arn {
                let resources = envelope_object
                    .get("resources")
                    .and_then(serde_json::Value::as_array)
                    .ok_or_else(|| invalid_event("EventBridge resources are missing"))?;
                if !resources.iter().any(|resource| {
                    resource
                        .as_str()
                        .is_some_and(|value| value == expected_resource_arn)
                }) {
                    return Err(invalid_event(
                        "EventBridge SES resource boundary did not match",
                    ));
                }
            }
            verifier.verify(&SesEventEnvelope {
                kind: SesEventEnvelopeKind::EventBridge,
                raw: bytes,
                envelope_id,
            })?;
            let event = envelope_object
                .get("detail")
                .cloned()
                .ok_or_else(|| invalid_event("EventBridge SES detail is missing"))?;
            normalize_ses_event(&event, Some(("aws.ses.eventbridge", envelope_id)))
        }
    }
}

/// Normalizes a direct SES JSON object received through an already trusted
/// internal transport. This function does not authenticate provider input and
/// deliberately rejects SNS and `EventBridge` wrappers.
pub fn normalize_trusted_ses_event(bytes: &[u8]) -> Result<MailDeliveryEvent, MailError> {
    let event = parse_event_json(bytes)?;
    if event.get("Message").is_some() || event.get("detail").is_some() {
        return Err(invalid_event(
            "wrapped SES events require authenticated envelope ingestion",
        ));
    }
    normalize_ses_event(&event, None)
}

fn parse_event_json(bytes: &[u8]) -> Result<serde_json::Value, MailError> {
    if bytes.is_empty() || bytes.len() > MAX_SES_EVENT_BYTES {
        return Err(MailError::new(
            MailErrorKind::InvalidMessage,
            SES_TRANSPORT_NAME,
            "SES event envelope is empty or too large",
        ));
    }
    serde_json::from_slice(bytes).map_err(|_| {
        MailError::new(
            MailErrorKind::InvalidMessage,
            SES_TRANSPORT_NAME,
            "SES event envelope is not valid JSON",
        )
    })
}

fn normalize_ses_event(
    event: &serde_json::Value,
    envelope_identity: Option<(&str, &str)>,
) -> Result<MailDeliveryEvent, MailError> {
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
    let topic = decode_ses_topic(first_tag(tags, "minco_topic")?)?;
    let provider_message_id = mail
        .get("messageId")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let kind = map_event_kind(event_type, event)?;
    let occurred_at = event_timestamp(kind, event)?;
    let source_event_id = if let Some((namespace, envelope_id)) = envelope_identity {
        deterministic_mail_event_id(&<[&str; 2]>::from((namespace, envelope_id)))
    } else {
        let canonical_payload = serde_json::to_string(&event)
            .map_err(|_| invalid_event("SES event cannot be canonicalized"))?;
        deterministic_mail_event_id(&["aws.ses.direct", &canonical_payload])
    };
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

fn envelope_string<'a>(
    envelope: &'a serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Result<&'a str, MailError> {
    let value = envelope
        .get(name)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid_event("SES event envelope field is missing"))?;
    if value.is_empty() || value.len() > 4_096 || value.chars().any(char::is_control) {
        return Err(invalid_event("SES event envelope field is invalid"));
    }
    Ok(value)
}

fn first_tag<'a>(
    tags: &'a serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Result<&'a str, MailError> {
    let values = tags
        .get(name)
        .and_then(serde_json::Value::as_array)
        .filter(|values| values.len() == 1)
        .ok_or_else(|| invalid_event("SES Minco correlation tag is missing or ambiguous"))?;
    let value = values
        .first()
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid_event("SES Minco correlation tag is missing"))?;
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(invalid_event("SES Minco correlation tag is invalid"));
    }
    Ok(value)
}

fn event_timestamp(
    kind: MailDeliveryEventKind,
    event: &serde_json::Value,
) -> Result<chrono::DateTime<chrono::Utc>, MailError> {
    let pointer = match kind {
        MailDeliveryEventKind::Submitted
        | MailDeliveryEventKind::Rejected
        | MailDeliveryEventKind::RenderingFailed => "/mail/timestamp",
        MailDeliveryEventKind::Delivered => "/delivery/timestamp",
        MailDeliveryEventKind::BouncedPermanent
        | MailDeliveryEventKind::BouncedTransient
        | MailDeliveryEventKind::BouncedUndetermined => "/bounce/timestamp",
        MailDeliveryEventKind::Complaint => "/complaint/timestamp",
        MailDeliveryEventKind::DeliveryDelayed => "/deliveryDelay/timestamp",
        MailDeliveryEventKind::Opened => "/open/timestamp",
        MailDeliveryEventKind::Clicked => "/click/timestamp",
        MailDeliveryEventKind::SubscriptionChanged => "/subscription/timestamp",
    };
    let value = event
        .pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid_event("SES event-specific timestamp is missing"))?;
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&chrono::Utc))
        .map_err(|_| invalid_event("SES event-specific timestamp is invalid"))
}

fn map_event_kind(
    event_type: &str,
    event: &serde_json::Value,
) -> Result<MailDeliveryEventKind, MailError> {
    let normalized_type = event_type.trim().to_ascii_lowercase();
    match normalized_type.as_str() {
        "send" => Ok(MailDeliveryEventKind::Submitted),
        "delivery" => Ok(MailDeliveryEventKind::Delivered),
        "bounce" => {
            let bounce_type = event
                .pointer("/bounce/bounceType")
                .and_then(serde_json::Value::as_str);
            Ok(match bounce_type {
                Some(value) if value.eq_ignore_ascii_case("Permanent") => {
                    MailDeliveryEventKind::BouncedPermanent
                }
                Some(value) if value.eq_ignore_ascii_case("Transient") => {
                    MailDeliveryEventKind::BouncedTransient
                }
                _ => MailDeliveryEventKind::BouncedUndetermined,
            })
        }
        "complaint" => Ok(MailDeliveryEventKind::Complaint),
        "reject" => Ok(MailDeliveryEventKind::Rejected),
        "deliverydelay" => Ok(MailDeliveryEventKind::DeliveryDelayed),
        "rendering failure" => Ok(MailDeliveryEventKind::RenderingFailed),
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

fn valid_sns_topic_arn(value: &str) -> bool {
    if value.len() > 2_048 || value.chars().any(char::is_control) {
        return false;
    }
    let parts = value.splitn(6, ':').collect::<Vec<_>>();
    parts.len() == 6
        && parts[0] == "arn"
        && matches!(parts[1], "aws" | "aws-cn" | "aws-us-gov")
        && parts[2] == "sns"
        && valid_provider_identifier(parts[3], 64)
        && parts[4].len() == 12
        && parts[4].bytes().all(|byte| byte.is_ascii_digit())
        && !parts[5].is_empty()
        && parts[5].len() <= 256
        && parts[5]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_ses_tag(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_minco_topic(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

#[cfg(test)]
mod mail_tests {
    use super::*;
    use minco_plugin_notifications::MailAttachment;
    use std::sync::Mutex;

    #[derive(Debug)]
    struct RecordingExecutor {
        requests: Mutex<Vec<SesSendEmailRequest>>,
        outcome: Result<Option<String>, MailError>,
    }

    impl RecordingExecutor {
        fn new(outcome: Result<Option<String>, MailError>) -> Self {
            Self {
                requests: Mutex::new(Vec::new()),
                outcome,
            }
        }

        fn requests(&self) -> Vec<SesSendEmailRequest> {
            self.requests.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl SesSendEmailExecutor for RecordingExecutor {
        async fn send(&self, request: &SesSendEmailRequest) -> Result<Option<String>, MailError> {
            self.requests.lock().unwrap().push(request.clone());
            self.outcome.clone()
        }
    }

    #[derive(Debug)]
    struct RejectingVerifier;

    impl SesEventEnvelopeVerifier for RejectingVerifier {
        fn verify(&self, _envelope: &SesEventEnvelope<'_>) -> Result<(), MailError> {
            Err(MailError::new(
                MailErrorKind::Authentication,
                SES_TRANSPORT_NAME,
                "test verifier rejected the envelope",
            ))
        }
    }

    #[derive(Debug)]
    struct AcceptingVerifier;

    impl SesEventEnvelopeVerifier for AcceptingVerifier {
        fn verify(&self, _envelope: &SesEventEnvelope<'_>) -> Result<(), MailError> {
            Ok(())
        }
    }

    fn direct_event(
        event_type: &str,
        event_object_name: &str,
        event_object: serde_json::Value,
    ) -> serde_json::Value {
        let message_id = Uuid::from_u128(0x018f_0000_0000_7000_8000_0000_0000_0001);
        let mut input = serde_json::json!({
            "eventType": event_type,
            "mail": {
                "timestamp": "2026-08-09T00:00:00Z",
                "messageId": "provider-message",
                "tags": {
                    "minco_message_id": [message_id.to_string()],
                    "minco_topic": ["b64_aW52b2ljZS5yZWFkeQ"]
                }
            }
        });
        input
            .as_object_mut()
            .unwrap()
            .insert(event_object_name.into(), event_object);
        input
    }

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
                    "minco_topic": ["b64_aW52b2ljZS5yZWFkeQ"]
                }
            },
            "delivery": {"timestamp": "2026-08-09T00:00:01Z"}
        });
        let event = normalize_trusted_ses_event(&serde_json::to_vec(&input).unwrap()).unwrap();
        assert_eq!(event.message_id, message_id);
        assert_eq!(event.kind, MailDeliveryEventKind::Delivered);
        assert_eq!(event.topic, "invoice.ready");
        assert!(
            !serde_json::to_string(&event)
                .unwrap()
                .contains("secret@example.com")
        );
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

    #[test]
    fn sdk_service_config_disables_internal_retries() {
        let sdk_config = aws_config::SdkConfig::builder().build();
        let service_config = ses_service_config(&sdk_config, Duration::from_secs(3));
        assert_eq!(
            service_config.retry_config().unwrap().max_attempts(),
            1,
            "one MailService attempt must cause at most one SES API call"
        );
    }

    #[tokio::test]
    async fn rich_mail_maps_to_one_observable_ses_request() {
        let executor = Arc::new(RecordingExecutor::new(Ok(Some(
            "ses-provider-message".into(),
        ))));
        let mut config = SesMailTransportConfig::new(
            MailAddress::named("sender@example.com", "Minco 送信者").unwrap(),
        )
        .unwrap();
        config.from_identity_arn =
            Some("arn:aws:ses:ap-southeast-2:123456789012:identity/example.com".into());
        config.configuration_set = Some("minco_mail".into());
        config.endpoint_id = Some("endpoint_1".into());
        config.tenant_name = Some("tenant_1".into());
        config
            .default_tags
            .insert("environment".into(), "test".into());
        let transport = SesMailTransport::from_executor(executor.clone(), config).unwrap();
        let message = MailMessage::builder("invoice.ready", "Invoice ✓")
            .to(MailAddress::named("to@example.com", "Primary Recipient").unwrap())
            .cc(MailAddress::new("cc@example.com").unwrap())
            .bcc(MailAddress::new("bcc@example.com").unwrap())
            .reply_to(MailAddress::new("reply@example.com").unwrap())
            .text("Plain body")
            .html("<strong>HTML body</strong><img src=\"cid:logo\">")
            .attachment(
                MailAttachment::attachment("invoice.pdf", "application/pdf", b"PDF".to_vec())
                    .unwrap(),
            )
            .attachment(
                MailAttachment::inline("logo.png", "image/png", b"PNG".to_vec(), "logo").unwrap(),
            )
            .header("X-Correlation-Id", "trace-123")
            .tag("application_tag", "invoice")
            .build()
            .unwrap();

        let receipt = transport.send(&message, 3).await.unwrap();
        assert_eq!(receipt.provider_message_id, "ses-provider-message");
        assert_eq!(receipt.attempt, 3);
        let requests = executor.requests();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(request.to_addresses, ["to@example.com"]);
        assert_eq!(request.cc_addresses, ["cc@example.com"]);
        assert_eq!(request.bcc_addresses, ["bcc@example.com"]);
        assert!(request.from_identity_arn.is_some());
        assert_eq!(request.configuration_set.as_deref(), Some("minco_mail"));
        assert_eq!(request.endpoint_id.as_deref(), Some("endpoint_1"));
        assert_eq!(request.tenant_name.as_deref(), Some("tenant_1"));
        let raw = String::from_utf8(request.raw_message.clone()).unwrap();
        assert!(raw.contains("Reply-To: reply@example.com"));
        assert!(raw.contains("multipart/mixed"));
        assert!(raw.contains("Content-ID: <logo>"));
        assert!(raw.lines().all(|line| !line.starts_with("Bcc:")));
        assert!(
            request.tags.iter().any(|tag| {
                tag.name() == "minco_topic" && tag.value() == "b64_aW52b2ljZS5yZWFkeQ"
            })
        );
    }

    #[tokio::test]
    async fn executor_outcomes_are_classified_after_exactly_one_call() {
        for (outcome, expected_kind) in [
            (
                Err(MailError::new(
                    MailErrorKind::Ambiguous,
                    SES_TRANSPORT_NAME,
                    "simulated transport ambiguity",
                )),
                MailErrorKind::Ambiguous,
            ),
            (Ok(None), MailErrorKind::Ambiguous),
            (
                Ok(Some("invalid\nidentifier".into())),
                MailErrorKind::Ambiguous,
            ),
            (
                Err(MailError::new(
                    MailErrorKind::Throttled,
                    SES_TRANSPORT_NAME,
                    "simulated provider throttle",
                )),
                MailErrorKind::Throttled,
            ),
            (
                Err(MailError::new(
                    MailErrorKind::Rejected,
                    SES_TRANSPORT_NAME,
                    "simulated provider rejection",
                )),
                MailErrorKind::Rejected,
            ),
        ] {
            let executor = Arc::new(RecordingExecutor::new(outcome));
            let transport = SesMailTransport::from_executor(
                executor.clone(),
                SesMailTransportConfig::new(MailAddress::new("sender@example.com").unwrap())
                    .unwrap(),
            )
            .unwrap();
            let message = MailMessage::builder("topic", "Subject")
                .to(MailAddress::new("person@example.com").unwrap())
                .text("Body")
                .build()
                .unwrap();

            let error = transport.send(&message, 1).await.unwrap_err();
            assert_eq!(error.kind, expected_kind);
            assert_eq!(executor.requests().len(), 1);
        }
    }

    #[test]
    fn sdk_error_classification_is_coarse_and_redacts_provider_diagnostics() {
        use aws_sdk_sesv2::types::error::{MessageRejected, TooManyRequestsException};

        let throttled = SendEmailError::TooManyRequestsException(
            TooManyRequestsException::builder()
                .message("provider detail must stay private")
                .build(),
        );
        assert_eq!(
            classify_send_service_error(&throttled),
            MailErrorKind::Throttled
        );
        let rejected = SendEmailError::MessageRejected(
            MessageRejected::builder()
                .message("recipient and provider detail must stay private")
                .build(),
        );
        assert_eq!(
            classify_send_service_error(&rejected),
            MailErrorKind::Rejected
        );

        let sdk_error: aws_sdk_sesv2::error::SdkError<SendEmailError> =
            aws_sdk_sesv2::error::SdkError::construction_failure(std::io::Error::other(
                "secret endpoint and recipient detail",
            ));
        let public = classify_send_error(&sdk_error);
        assert_eq!(public.kind, MailErrorKind::Ambiguous);
        assert!(!public.to_string().contains("secret"));
        assert_eq!(public.message, "SES SendEmail failed");
    }

    #[test]
    fn wrappers_require_their_exact_policy_and_verifier() {
        let detail = direct_event(
            "Delivery",
            "delivery",
            serde_json::json!({"timestamp": "2026-08-09T00:01:00Z"}),
        );
        let sns = serde_json::json!({
            "Type": "Notification",
            "TopicArn": "arn:aws:sns:ap-southeast-2:123456789012:minco-mail",
            "MessageId": "sns-event-1",
            "Message": serde_json::to_string(&detail).unwrap()
        });
        let sns_bytes = serde_json::to_vec(&sns).unwrap();
        assert!(normalize_trusted_ses_event(&sns_bytes).is_err());
        let policy =
            SesEventTrustPolicy::sns("arn:aws:sns:ap-southeast-2:123456789012:minco-mail").unwrap();
        let error =
            verify_and_normalize_ses_event(&sns_bytes, &policy, &RejectingVerifier).unwrap_err();
        assert_eq!(error.kind, MailErrorKind::Authentication);

        let event_bridge = serde_json::json!({
            "id": "eventbridge-event-1",
            "source": "aws.ses",
            "detail-type": "Email Delivery",
            "account": "123456789012",
            "region": "ap-southeast-2",
            "resources": ["arn:aws:ses:ap-southeast-2:123456789012:configuration-set/minco"],
            "detail": detail
        });
        let event_bridge_bytes = serde_json::to_vec(&event_bridge).unwrap();
        assert!(normalize_trusted_ses_event(&event_bridge_bytes).is_err());
        let policy = SesEventTrustPolicy::event_bridge(
            "123456789012",
            "ap-southeast-2",
            ["Email Delivery".to_owned()],
        )
        .unwrap()
        .with_eventbridge_resource(
            "arn:aws:ses:ap-southeast-2:123456789012:configuration-set/minco",
        )
        .unwrap();
        let error =
            verify_and_normalize_ses_event(&event_bridge_bytes, &policy, &RejectingVerifier)
                .unwrap_err();
        assert_eq!(error.kind, MailErrorKind::Authentication);
    }

    #[test]
    fn sns_policy_validates_partition_service_account_and_topic_shape() {
        assert!(
            SesEventTrustPolicy::sns("arn:aws-cn:sns:cn-north-1:123456789012:minco-mail").is_ok()
        );
        assert!(
            SesEventTrustPolicy::sns("arn:aws:sqs:ap-southeast-2:123456789012:minco-mail").is_err()
        );
        assert!(
            SesEventTrustPolicy::sns("arn:aws:sns:ap-southeast-2:not-an-account:minco-mail")
                .is_err()
        );
    }

    #[test]
    fn envelope_source_ids_are_namespaced_by_authenticated_transport() {
        let detail = direct_event(
            "Delivery",
            "delivery",
            serde_json::json!({"timestamp": "2026-08-09T00:01:00Z"}),
        );
        let sns = serde_json::json!({
            "Type": "Notification",
            "TopicArn": "arn:aws:sns:ap-southeast-2:123456789012:minco-mail",
            "MessageId": "shared-envelope-id",
            "Message": serde_json::to_string(&detail).unwrap()
        });
        let sns_policy =
            SesEventTrustPolicy::sns("arn:aws:sns:ap-southeast-2:123456789012:minco-mail").unwrap();
        let sns_event = verify_and_normalize_ses_event(
            &serde_json::to_vec(&sns).unwrap(),
            &sns_policy,
            &AcceptingVerifier,
        )
        .unwrap();

        let event_bridge = serde_json::json!({
            "id": "shared-envelope-id",
            "source": "aws.ses",
            "detail-type": "Email Delivery",
            "account": "123456789012",
            "region": "ap-southeast-2",
            "resources": [],
            "detail": detail
        });
        let event_bridge_policy = SesEventTrustPolicy::event_bridge(
            "123456789012",
            "ap-southeast-2",
            ["Email Delivery".to_owned()],
        )
        .unwrap();
        let event_bridge_event = verify_and_normalize_ses_event(
            &serde_json::to_vec(&event_bridge).unwrap(),
            &event_bridge_policy,
            &AcceptingVerifier,
        )
        .unwrap();

        assert_ne!(
            sns_event.source_event_id,
            event_bridge_event.source_event_id
        );
    }

    #[test]
    fn ambiguous_correlation_tags_and_non_official_event_names_fail_closed() {
        let mut duplicate_tag = direct_event(
            "Delivery",
            "delivery",
            serde_json::json!({"timestamp": "2026-08-09T00:01:00Z"}),
        );
        duplicate_tag["mail"]["tags"]["minco_topic"] =
            serde_json::json!(["b64_aW52b2ljZS5yZWFkeQ", "b64_b3RoZXI"]);
        assert!(normalize_trusted_ses_event(&serde_json::to_vec(&duplicate_tag).unwrap()).is_err());

        let forged = direct_event(
            "Rendering-Failure",
            "failure",
            serde_json::json!({"templateName": "receipt", "errorMessage": "missing"}),
        );
        assert!(normalize_trusted_ses_event(&serde_json::to_vec(&forged).unwrap()).is_err());
    }

    #[test]
    fn dotted_topic_is_reversibly_encoded_for_ses_tags() {
        let message = MailMessage::builder("invoice.ready", "Invoice")
            .to(MailAddress::new("person@example.com").unwrap())
            .text("Body")
            .build()
            .unwrap();
        let config =
            SesMailTransportConfig::new(MailAddress::new("no-reply@example.com").unwrap()).unwrap();
        let tags = ses_tags(&message, &config).unwrap();
        let encoded = tags
            .iter()
            .find(|tag| tag.name() == "minco_topic")
            .unwrap()
            .value();
        assert_eq!(encoded, "b64_aW52b2ljZS5yZWFkeQ");
        assert!(
            encoded
                .bytes()
                .all(|byte| { byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-') })
        );
    }

    #[test]
    fn merged_ses_tags_are_bounded_and_provider_safe() {
        let mut config =
            SesMailTransportConfig::new(MailAddress::new("no-reply@example.com").unwrap()).unwrap();
        for index in 0..48 {
            config
                .default_tags
                .insert(format!("default_{index}"), "value".into());
        }
        let mut builder = MailMessage::builder("topic", "Subject")
            .to(MailAddress::new("person@example.com").unwrap())
            .text("Body");
        for index in 0..48 {
            builder = builder.tag(format!("message_{index}"), "value");
        }
        assert!(ses_tags(&builder.build().unwrap(), &config).is_err());

        let invalid = MailMessage::builder("topic", "Subject")
            .to(MailAddress::new("person@example.com").unwrap())
            .text("Body")
            .tag("application.tag", "value")
            .build()
            .unwrap();
        assert!(
            ses_tags(
                &invalid,
                &SesMailTransportConfig::new(MailAddress::new("no-reply@example.com").unwrap())
                    .unwrap()
            )
            .is_err()
        );
    }

    #[test]
    fn event_specific_timestamps_win_over_submission_time() {
        for (event_type, object_name) in [
            ("Delivery", "delivery"),
            ("Bounce", "bounce"),
            ("Complaint", "complaint"),
        ] {
            let mut event_object = serde_json::json!({
                "timestamp": "2026-08-09T00:01:00Z"
            });
            if event_type == "Bounce" {
                event_object["bounceType"] = serde_json::Value::String("Permanent".into());
            }
            let input = direct_event(event_type, object_name, event_object);
            let event = normalize_trusted_ses_event(&serde_json::to_vec(&input).unwrap()).unwrap();
            assert_eq!(event.occurred_at.to_rfc3339(), "2026-08-09T00:01:00+00:00");
        }
    }

    #[test]
    fn rendering_failure_and_subscription_use_official_shapes() {
        let rendering = direct_event(
            "Rendering Failure",
            "failure",
            serde_json::json!({"templateName": "receipt", "errorMessage": "missing"}),
        );
        assert_eq!(
            normalize_trusted_ses_event(&serde_json::to_vec(&rendering).unwrap())
                .unwrap()
                .kind,
            MailDeliveryEventKind::RenderingFailed
        );

        let subscription = direct_event(
            "Subscription",
            "subscription",
            serde_json::json!({"timestamp": "2026-08-09T00:02:00Z"}),
        );
        assert_eq!(
            normalize_trusted_ses_event(&serde_json::to_vec(&subscription).unwrap())
                .unwrap()
                .occurred_at
                .to_rfc3339(),
            "2026-08-09T00:02:00+00:00"
        );
    }

    #[test]
    fn direct_event_identity_is_deterministic_and_distinguishes_clicks() {
        let first = direct_event(
            "Click",
            "click",
            serde_json::json!({"timestamp": "2026-08-09T00:03:00Z", "link": "https://example.invalid/one"}),
        );
        let second = direct_event(
            "Click",
            "click",
            serde_json::json!({"timestamp": "2026-08-09T00:04:00Z", "link": "https://example.invalid/two"}),
        );
        let first_bytes = serde_json::to_vec(&first).unwrap();
        let first_event = normalize_trusted_ses_event(&first_bytes).unwrap();
        assert_eq!(
            first_event.source_event_id,
            normalize_trusted_ses_event(&first_bytes)
                .unwrap()
                .source_event_id
        );
        assert_ne!(
            first_event.source_event_id,
            normalize_trusted_ses_event(&serde_json::to_vec(&second).unwrap())
                .unwrap()
                .source_event_id
        );

        let missing = direct_event("Click", "click", serde_json::json!({"link": "x"}));
        assert!(normalize_trusted_ses_event(&serde_json::to_vec(&missing).unwrap()).is_err());
    }
}
