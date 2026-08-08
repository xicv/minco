use crate::{Notification, NotificationChannel, NotificationError, NotificationSink};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};
use tokio::sync::RwLock;
use uuid::Uuid;

const MAX_RECIPIENTS: usize = 50;
const MAX_BODY_BYTES: usize = 1_000_000;
const MAX_ATTACHMENT_COUNT: usize = 32;
const MAX_ATTACHMENT_BYTES: usize = 25 * 1024 * 1024;
const MAX_HEADERS: usize = 15;
const MAX_USER_TAGS: usize = 48;
const MAX_METADATA_BYTES: usize = 64 * 1024;
const RESERVED_TAGS: [&str; 2] = ["minco_message_id", "minco_topic"];

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailAddress {
    pub address: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl MailAddress {
    pub fn new(address: impl Into<String>) -> Result<Self, MailError> {
        let address = Self {
            address: address.into(),
            name: None,
        };
        address.validate()?;
        Ok(address)
    }

    pub fn named(
        address: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<Self, MailError> {
        let address = Self {
            address: address.into(),
            name: Some(name.into()),
        };
        address.validate()?;
        Ok(address)
    }

    pub fn validate(&self) -> Result<(), MailError> {
        validate_email_address(&self.address)?;
        if self.name.as_deref().is_some_and(|name| {
            name.trim().is_empty()
                || name.len() > 256
                || name.chars().any(|character| character.is_control())
        }) {
            return Err(MailError::invalid("mail display name is invalid"));
        }
        Ok(())
    }

    pub fn formatted(&self) -> String {
        match &self.name {
            None => self.address.clone(),
            Some(name) if name.is_ascii() => {
                let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{escaped}\" <{}>", self.address)
            }
            Some(name) => format!("{} <{}>", encode_header_word(name), self.address),
        }
    }
}

impl fmt::Debug for MailAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MailAddress")
            .field("address", &"[REDACTED]")
            .field("name", &self.name.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailAttachmentDisposition {
    Attachment,
    Inline,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailAttachment {
    pub file_name: String,
    pub content_type: String,
    pub content: Vec<u8>,
    pub disposition: MailAttachmentDisposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_id: Option<String>,
}

impl MailAttachment {
    pub fn attachment(
        file_name: impl Into<String>,
        content_type: impl Into<String>,
        content: impl Into<Vec<u8>>,
    ) -> Result<Self, MailError> {
        let attachment = Self {
            file_name: file_name.into(),
            content_type: content_type.into(),
            content: content.into(),
            disposition: MailAttachmentDisposition::Attachment,
            content_id: None,
        };
        attachment.validate()?;
        Ok(attachment)
    }

    pub fn inline(
        file_name: impl Into<String>,
        content_type: impl Into<String>,
        content: impl Into<Vec<u8>>,
        content_id: impl Into<String>,
    ) -> Result<Self, MailError> {
        let attachment = Self {
            file_name: file_name.into(),
            content_type: content_type.into(),
            content: content.into(),
            disposition: MailAttachmentDisposition::Inline,
            content_id: Some(content_id.into()),
        };
        attachment.validate()?;
        Ok(attachment)
    }

    fn validate(&self) -> Result<(), MailError> {
        if self.file_name.trim().is_empty()
            || self.file_name.len() > 255
            || self
                .file_name
                .chars()
                .any(|character| character.is_control() || matches!(character, '/' | '\\'))
        {
            return Err(MailError::invalid("mail attachment file name is invalid"));
        }
        if !valid_content_type(&self.content_type) {
            return Err(MailError::invalid("mail attachment content type is invalid"));
        }
        if self.content.is_empty() {
            return Err(MailError::invalid("mail attachment must not be empty"));
        }
        match (self.disposition, self.content_id.as_deref()) {
            (MailAttachmentDisposition::Attachment, None) => {}
            (MailAttachmentDisposition::Inline, Some(content_id))
                if valid_content_id(content_id) => {}
            _ => {
                return Err(MailError::invalid(
                    "inline mail attachments require a valid content ID",
                ));
            }
        }
        Ok(())
    }
}

impl fmt::Debug for MailAttachment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MailAttachment")
            .field("file_name", &"[REDACTED]")
            .field("content_type", &self.content_type)
            .field("size_bytes", &self.content.len())
            .field("disposition", &self.disposition)
            .field("content_id", &self.content_id.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct MailMessage {
    pub id: Uuid,
    pub topic: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<MailAddress>,
    pub to: Vec<MailAddress>,
    #[serde(default)]
    pub cc: Vec<MailAddress>,
    #[serde(default)]
    pub bcc: Vec<MailAddress>,
    #[serde(default)]
    pub reply_to: Vec<MailAddress>,
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    #[serde(default)]
    pub attachments: Vec<MailAttachment>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub tags: BTreeMap<String, String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl MailMessage {
    pub fn builder(topic: impl Into<String>, subject: impl Into<String>) -> MailMessageBuilder {
        MailMessageBuilder {
            message: Self {
                id: Uuid::now_v7(),
                topic: topic.into(),
                from: None,
                to: Vec::new(),
                cc: Vec::new(),
                bcc: Vec::new(),
                reply_to: Vec::new(),
                subject: subject.into(),
                text: None,
                html: None,
                attachments: Vec::new(),
                headers: BTreeMap::new(),
                tags: BTreeMap::new(),
                metadata: BTreeMap::new(),
                created_at: Utc::now(),
            },
        }
    }

    pub fn recipients(&self) -> impl Iterator<Item = &MailAddress> {
        self.to.iter().chain(&self.cc).chain(&self.bcc)
    }

    pub fn validate(&self) -> Result<(), MailError> {
        if !valid_topic(&self.topic) {
            return Err(MailError::invalid("mail topic is invalid"));
        }
        if self.subject.trim().is_empty()
            || self.subject.len() > 998
            || self.subject.chars().any(char::is_control)
        {
            return Err(MailError::invalid("mail subject is invalid"));
        }
        if self.to.is_empty() {
            return Err(MailError::invalid(
                "mail message requires at least one primary recipient",
            ));
        }
        let recipient_count = self.to.len() + self.cc.len() + self.bcc.len();
        if recipient_count > MAX_RECIPIENTS {
            return Err(MailError::invalid(
                "mail message exceeds the 50-recipient delivery boundary",
            ));
        }
        let mut unique = BTreeSet::new();
        for recipient in self.recipients() {
            recipient.validate()?;
            if !unique.insert(recipient.address.to_ascii_lowercase()) {
                return Err(MailError::invalid(
                    "mail recipient lists must not contain duplicates",
                ));
            }
        }
        if let Some(from) = &self.from {
            from.validate()?;
        }
        for reply_to in &self.reply_to {
            reply_to.validate()?;
        }
        if self.reply_to.len() > 10 {
            return Err(MailError::invalid(
                "mail message exceeds the reply-to address boundary",
            ));
        }
        match (&self.text, &self.html) {
            (None, None) => {
                return Err(MailError::invalid(
                    "mail message requires a text or HTML body",
                ));
            }
            (Some(text), _) if text.is_empty() || text.len() > MAX_BODY_BYTES => {
                return Err(MailError::invalid(
                    "mail text body exceeds the delivery boundary",
                ));
            }
            (_, Some(html)) if html.is_empty() || html.len() > MAX_BODY_BYTES => {
                return Err(MailError::invalid(
                    "mail HTML body exceeds the delivery boundary",
                ));
            }
            _ => {}
        }
        if self.attachments.len() > MAX_ATTACHMENT_COUNT {
            return Err(MailError::invalid(
                "mail message exceeds the attachment count boundary",
            ));
        }
        let mut attachment_bytes = 0_usize;
        for attachment in &self.attachments {
            attachment.validate()?;
            attachment_bytes = attachment_bytes
                .checked_add(attachment.content.len())
                .ok_or_else(|| MailError::invalid("mail attachment size overflow"))?;
        }
        if attachment_bytes > MAX_ATTACHMENT_BYTES {
            return Err(MailError::invalid(
                "mail attachments exceed the 25 MiB raw-content boundary",
            ));
        }
        if self.headers.len() > MAX_HEADERS
            || self
                .headers
                .iter()
                .any(|(name, value)| !valid_header(name, value))
        {
            return Err(MailError::invalid("mail custom headers are invalid"));
        }
        if self.tags.len() > MAX_USER_TAGS
            || self.tags.iter().any(|(name, value)| {
                RESERVED_TAGS
                    .iter()
                    .any(|reserved| name.eq_ignore_ascii_case(reserved))
                    || !valid_tag_component(name)
                    || !valid_tag_component(value)
            })
        {
            return Err(MailError::invalid("mail delivery tags are invalid"));
        }
        let metadata_size = serde_json::to_vec(&self.metadata)
            .map_err(|_| MailError::invalid("mail metadata cannot be serialized"))?
            .len();
        if metadata_size > MAX_METADATA_BYTES {
            return Err(MailError::invalid(
                "mail metadata exceeds the 64 KiB boundary",
            ));
        }
        Ok(())
    }
}

impl fmt::Debug for MailMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MailMessage")
            .field("id", &self.id)
            .field("topic", &self.topic)
            .field("from", &self.from.as_ref().map(|_| "[REDACTED]"))
            .field("to_count", &self.to.len())
            .field("cc_count", &self.cc.len())
            .field("bcc_count", &self.bcc.len())
            .field("reply_to_count", &self.reply_to.len())
            .field("subject", &"[REDACTED]")
            .field("text_bytes", &self.text.as_ref().map(String::len))
            .field("html_bytes", &self.html.as_ref().map(String::len))
            .field("attachment_count", &self.attachments.len())
            .field("header_count", &self.headers.len())
            .field("tag_names", &self.tags.keys().collect::<Vec<_>>())
            .field("metadata_keys", &self.metadata.keys().collect::<Vec<_>>())
            .field("created_at", &self.created_at)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct MailMessageBuilder {
    message: MailMessage,
}

impl MailMessageBuilder {
    pub fn id(mut self, id: Uuid) -> Self {
        self.message.id = id;
        self
    }

    pub fn created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.message.created_at = created_at;
        self
    }

    pub fn from(mut self, address: MailAddress) -> Self {
        self.message.from = Some(address);
        self
    }

    pub fn to(mut self, address: MailAddress) -> Self {
        self.message.to.push(address);
        self
    }

    pub fn cc(mut self, address: MailAddress) -> Self {
        self.message.cc.push(address);
        self
    }

    pub fn bcc(mut self, address: MailAddress) -> Self {
        self.message.bcc.push(address);
        self
    }

    pub fn reply_to(mut self, address: MailAddress) -> Self {
        self.message.reply_to.push(address);
        self
    }

    pub fn text(mut self, body: impl Into<String>) -> Self {
        self.message.text = Some(body.into());
        self
    }

    pub fn html(mut self, body: impl Into<String>) -> Self {
        self.message.html = Some(body.into());
        self
    }

    pub fn attachment(mut self, attachment: MailAttachment) -> Self {
        self.message.attachments.push(attachment);
        self
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.message.headers.insert(name.into(), value.into());
        self
    }

    pub fn tag(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.message.tags.insert(name.into(), value.into());
        self
    }

    pub fn metadata(mut self, name: impl Into<String>, value: serde_json::Value) -> Self {
        self.message.metadata.insert(name.into(), value);
        self
    }

    pub fn build(self) -> Result<MailMessage, MailError> {
        self.message.validate()?;
        Ok(self.message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailErrorKind {
    InvalidMessage,
    Configuration,
    Authentication,
    Rejected,
    Throttled,
    Unavailable,
    Ambiguous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailRetryAdvice {
    Never,
    SafeAfterBackoff,
    ReconcileBeforeRetry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("mail transport {transport} failed ({kind:?}): {message}")]
pub struct MailError {
    pub kind: MailErrorKind,
    pub transport: String,
    pub message: String,
}

impl MailError {
    pub fn new(
        kind: MailErrorKind,
        transport: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            transport: sanitize_diagnostic(&transport.into(), 64),
            message: sanitize_diagnostic(&message.into(), 2_048),
        }
    }

    pub fn retry_advice(&self) -> MailRetryAdvice {
        match self.kind {
            MailErrorKind::Throttled | MailErrorKind::Unavailable => {
                MailRetryAdvice::SafeAfterBackoff
            }
            MailErrorKind::Ambiguous => MailRetryAdvice::ReconcileBeforeRetry,
            MailErrorKind::InvalidMessage
            | MailErrorKind::Configuration
            | MailErrorKind::Authentication
            | MailErrorKind::Rejected => MailRetryAdvice::Never,
        }
    }

    pub fn can_failover(&self) -> bool {
        self.retry_advice() == MailRetryAdvice::SafeAfterBackoff
    }

    pub fn is_ambiguous(&self) -> bool {
        self.kind == MailErrorKind::Ambiguous
    }

    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::new(MailErrorKind::InvalidMessage, "mail", message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailReceipt {
    pub message_id: Uuid,
    pub transport: String,
    pub provider_message_id: String,
    pub accepted_at: DateTime<Utc>,
    pub attempt: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailEventKind {
    Prepared,
    Attempting,
    AttemptFailed,
    Accepted,
    Delivered,
    BouncedPermanent,
    BouncedTransient,
    Complaint,
    Rejected,
    DeliveryDelayed,
    RenderingFailed,
    Opened,
    Clicked,
    SubscriptionChanged,
    UnknownProviderEvent,
}

impl MailEventKind {
    fn is_warning(self) -> bool {
        matches!(
            self,
            Self::AttemptFailed
                | Self::BouncedPermanent
                | Self::BouncedTransient
                | Self::Complaint
                | Self::Rejected
                | Self::DeliveryDelayed
                | Self::RenderingFailed
                | Self::UnknownProviderEvent
        )
    }

    fn is_engagement(self) -> bool {
        matches!(self, Self::Opened | Self::Clicked | Self::SubscriptionChanged)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailEvent {
    pub id: Uuid,
    pub message_id: Uuid,
    pub topic: String,
    pub transport: String,
    pub kind: MailEventKind,
    pub occurred_at: DateTime<Utc>,
    pub attempt: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<MailErrorKind>,
}

impl MailEvent {
    pub fn provider_feedback(
        message_id: Uuid,
        topic: impl Into<String>,
        transport: impl Into<String>,
        kind: MailEventKind,
        provider_message_id: Option<String>,
        occurred_at: DateTime<Utc>,
    ) -> Result<Self, MailError> {
        if matches!(
            kind,
            MailEventKind::Prepared | MailEventKind::Attempting | MailEventKind::AttemptFailed
        ) {
            return Err(MailError::invalid(
                "provider feedback cannot use a mail service-only event kind",
            ));
        }
        let event = Self {
            id: Uuid::now_v7(),
            message_id,
            topic: topic.into(),
            transport: transport.into(),
            kind,
            occurred_at,
            attempt: 0,
            provider_message_id,
            failure_kind: None,
        };
        event.validate()?;
        Ok(event)
    }

    pub fn validate(&self) -> Result<(), MailError> {
        if self.id == Uuid::nil()
            || self.message_id == Uuid::nil()
            || !valid_topic(&self.topic)
            || !valid_transport_name(&self.transport)
            || self.provider_message_id.as_deref().is_some_and(|value| {
                value.trim().is_empty()
                    || value.len() > 512
                    || value.chars().any(char::is_control)
            })
            || (self.kind == MailEventKind::AttemptFailed && self.failure_kind.is_none())
        {
            return Err(MailError::invalid("mail lifecycle event is invalid"));
        }
        Ok(())
    }

    fn service(
        message: &MailMessage,
        transport: impl Into<String>,
        kind: MailEventKind,
        attempt: u32,
        provider_message_id: Option<String>,
        failure_kind: Option<MailErrorKind>,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            message_id: message.id,
            topic: message.topic.clone(),
            transport: transport.into(),
            kind,
            occurred_at: Utc::now(),
            attempt,
            provider_message_id,
            failure_kind,
        }
    }
}

#[async_trait]
pub trait MailObserver: Send + Sync + fmt::Debug {
    async fn observe(&self, event: &MailEvent);
}

#[derive(Debug, Default)]
pub struct NoopMailObserver;

#[async_trait]
impl MailObserver for NoopMailObserver {
    async fn observe(&self, _event: &MailEvent) {}
}

#[derive(Debug, Default)]
pub struct MemoryMailObserver {
    events: RwLock<Vec<MailEvent>>,
}

impl MemoryMailObserver {
    pub async fn events(&self) -> Vec<MailEvent> {
        self.events.read().await.clone()
    }

    pub async fn clear(&self) {
        self.events.write().await.clear();
    }
}

#[async_trait]
impl MailObserver for MemoryMailObserver {
    async fn observe(&self, event: &MailEvent) {
        self.events.write().await.push(event.clone());
    }
}

#[derive(Clone)]
pub struct CompositeMailObserver {
    observers: Vec<Arc<dyn MailObserver>>,
}

impl CompositeMailObserver {
    pub fn new(observers: Vec<Arc<dyn MailObserver>>) -> Self {
        Self { observers }
    }
}

impl fmt::Debug for CompositeMailObserver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CompositeMailObserver")
            .field("observer_count", &self.observers.len())
            .finish()
    }
}

#[async_trait]
impl MailObserver for CompositeMailObserver {
    async fn observe(&self, event: &MailEvent) {
        for observer in &self.observers {
            observer.observe(event).await;
        }
    }
}

#[derive(Debug, Default)]
pub struct TracingMailObserver;

#[async_trait]
impl MailObserver for TracingMailObserver {
    async fn observe(&self, event: &MailEvent) {
        let provider_message_id = event.provider_message_id.as_deref().unwrap_or("");
        if event.kind.is_warning() {
            tracing::warn!(
                target: "minco.mail",
                mail_event_id = %event.id,
                mail_message_id = %event.message_id,
                mail_topic = %event.topic,
                mail_transport = %event.transport,
                mail_event = ?event.kind,
                mail_attempt = event.attempt,
                mail_provider_message_id = provider_message_id,
                mail_failure_kind = ?event.failure_kind,
                "mail lifecycle event"
            );
        } else if event.kind.is_engagement() {
            tracing::debug!(
                target: "minco.mail",
                mail_event_id = %event.id,
                mail_message_id = %event.message_id,
                mail_topic = %event.topic,
                mail_transport = %event.transport,
                mail_event = ?event.kind,
                mail_attempt = event.attempt,
                mail_provider_message_id = provider_message_id,
                "mail lifecycle event"
            );
        } else {
            tracing::info!(
                target: "minco.mail",
                mail_event_id = %event.id,
                mail_message_id = %event.message_id,
                mail_topic = %event.topic,
                mail_transport = %event.transport,
                mail_event = ?event.kind,
                mail_attempt = event.attempt,
                mail_provider_message_id = provider_message_id,
                "mail lifecycle event"
            );
        }
    }
}

#[async_trait]
pub trait MailTransport: Send + Sync + fmt::Debug {
    fn name(&self) -> &str;

    async fn send(&self, message: &MailMessage, attempt: u32)
    -> Result<MailReceipt, MailError>;
}

#[derive(Clone)]
pub struct MailService {
    transports: Vec<Arc<dyn MailTransport>>,
    observer: Arc<dyn MailObserver>,
}

impl MailService {
    pub fn new(
        transports: Vec<Arc<dyn MailTransport>>,
        observer: Arc<dyn MailObserver>,
    ) -> Result<Self, MailError> {
        if transports.is_empty() {
            return Err(MailError::new(
                MailErrorKind::Configuration,
                "mail",
                "mail service requires at least one transport",
            ));
        }
        let mut names = BTreeSet::new();
        for transport in &transports {
            if !valid_transport_name(transport.name())
                || !names.insert(transport.name().to_owned())
            {
                return Err(MailError::new(
                    MailErrorKind::Configuration,
                    "mail",
                    "mail transport names must be unique stable identifiers",
                ));
            }
        }
        Ok(Self {
            transports,
            observer,
        })
    }

    pub fn single(
        transport: Arc<dyn MailTransport>,
        observer: Arc<dyn MailObserver>,
    ) -> Result<Self, MailError> {
        Self::new(vec![transport], observer)
    }

    pub async fn observe_provider_event(&self, event: MailEvent) -> Result<(), MailError> {
        event.validate()?;
        if matches!(
            event.kind,
            MailEventKind::Prepared | MailEventKind::Attempting | MailEventKind::AttemptFailed
        ) {
            return Err(MailError::invalid(
                "provider feedback cannot use a mail service-only event kind",
            ));
        }
        self.observer.observe(&event).await;
        Ok(())
    }

    pub async fn send(&self, message: MailMessage) -> Result<MailReceipt, MailError> {
        message.validate()?;
        self.observer
            .observe(&MailEvent::service(
                &message,
                "mail.service",
                MailEventKind::Prepared,
                0,
                None,
                None,
            ))
            .await;

        for (index, transport) in self.transports.iter().enumerate() {
            let attempt = u32::try_from(index + 1).map_err(|_| {
                MailError::new(
                    MailErrorKind::Configuration,
                    "mail",
                    "mail transport attempt count overflow",
                )
            })?;
            self.observer
                .observe(&MailEvent::service(
                    &message,
                    transport.name(),
                    MailEventKind::Attempting,
                    attempt,
                    None,
                    None,
                ))
                .await;

            match transport.send(&message, attempt).await {
                Ok(receipt) => {
                    if receipt.message_id != message.id
                        || receipt.transport != transport.name()
                        || receipt.attempt != attempt
                        || receipt.provider_message_id.trim().is_empty()
                        || receipt
                            .provider_message_id
                            .chars()
                            .any(char::is_control)
                    {
                        let error = MailError::new(
                            MailErrorKind::Ambiguous,
                            transport.name(),
                            "mail transport accepted the request but returned an invalid receipt",
                        );
                        self.observer
                            .observe(&MailEvent::service(
                                &message,
                                transport.name(),
                                MailEventKind::AttemptFailed,
                                attempt,
                                None,
                                Some(error.kind),
                            ))
                            .await;
                        return Err(error);
                    }
                    self.observer
                        .observe(&MailEvent::service(
                            &message,
                            transport.name(),
                            MailEventKind::Accepted,
                            attempt,
                            Some(receipt.provider_message_id.clone()),
                            None,
                        ))
                        .await;
                    return Ok(receipt);
                }
                Err(error) => {
                    self.observer
                        .observe(&MailEvent::service(
                            &message,
                            transport.name(),
                            MailEventKind::AttemptFailed,
                            attempt,
                            None,
                            Some(error.kind),
                        ))
                        .await;
                    let has_fallback = index + 1 < self.transports.len();
                    if !error.can_failover() || !has_fallback {
                        return Err(error);
                    }
                }
            }
        }

        Err(MailError::new(
            MailErrorKind::Unavailable,
            "mail",
            "all configured mail transports were unavailable",
        ))
    }
}

impl fmt::Debug for MailService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MailService")
            .field(
                "transports",
                &self
                    .transports
                    .iter()
                    .map(|transport| transport.name())
                    .collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

pub struct MemoryMailTransport {
    name: String,
    messages: RwLock<Vec<MailMessage>>,
}

impl MemoryMailTransport {
    pub fn named(name: impl Into<String>) -> Result<Self, MailError> {
        let name = name.into();
        if !valid_transport_name(&name) {
            return Err(MailError::new(
                MailErrorKind::Configuration,
                "memory",
                "memory mail transport name is invalid",
            ));
        }
        Ok(Self {
            name,
            messages: RwLock::new(Vec::new()),
        })
    }

    pub async fn messages(&self) -> Vec<MailMessage> {
        self.messages.read().await.clone()
    }

    pub async fn count(&self) -> usize {
        self.messages.read().await.len()
    }

    pub async fn clear(&self) {
        self.messages.write().await.clear();
    }

    pub async fn sent_to(&self, address: &str) -> bool {
        self.messages.read().await.iter().any(|message| {
            message
                .recipients()
                .any(|recipient| recipient.address.eq_ignore_ascii_case(address))
        })
    }

    pub async fn assert_sent_count(&self, expected: usize) {
        assert_eq!(self.count().await, expected, "unexpected sent-mail count");
    }

    pub async fn assert_sent_to(&self, address: &str) {
        assert!(
            self.sent_to(address).await,
            "no captured mail was sent to the expected address"
        );
    }
}

impl Default for MemoryMailTransport {
    fn default() -> Self {
        Self::named("memory").expect("static memory transport name")
    }
}

impl fmt::Debug for MemoryMailTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MemoryMailTransport")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl MailTransport for MemoryMailTransport {
    fn name(&self) -> &str {
        &self.name
    }

    async fn send(
        &self,
        message: &MailMessage,
        attempt: u32,
    ) -> Result<MailReceipt, MailError> {
        message.validate()?;
        let mut messages = self.messages.write().await;
        messages.push(message.clone());
        Ok(MailReceipt {
            message_id: message.id,
            transport: self.name.clone(),
            provider_message_id: format!("memory:{}:{}", message.id, messages.len()),
            accepted_at: Utc::now(),
            attempt,
        })
    }
}

#[derive(Clone)]
pub struct Legacyage> {
    .await;
        m           .  attempt,
        })
    }
}

#[deriignorenesult<Self, Maip5y(|mememememememe,    "no captured mail was sent lEventKind::Attempting,
          " {
            tracing::wa!:{}:{}", ememe,  7attemsmessages = self.me.meme,  7attemsmessages =u      .=u      .=,
    pub conTransessages =tempting,
cmememe,  y_from(index + 1).map_err(|_| {
                Mail   . {
  ttemsmessages + 1).y,mpt l }
}

#[derive(Debug,from(index + 1).map_erta.ke5r  eveh        .any(chr:n0   }

    pub async fn count(&self) -> usize {
        self.messages.read().await.len()
    }

    pub async fn clear(&self) {
        self.messages.write().await.clear();
    }

    pub async fn sent_to(&self, address: &str) -> bool {
        self.messages.e.meme, w }
}ug,from(index + 1).map_erta.ke5r  eveh        .any(chr:n0   }

clear();
    }

    pub asy address: &str) -> bool {
           accepted_at: Utc:pted_sagawa,erve(event).await;
        }
    }
}

#[derive(Debug, Defaulttice(
               = index          N>ess: &s@sl {
        self.messages.e.meme, w }
}ug,from(index + 1).map_ert       selfn   }

    pub asy address: &sk(receipt) => {
                Rnddress: &sk(receipt) => {
         ult, skip_serializing_if = "Option::is_ncsb transport: Strn::is_ncsb transport: Strn::is_ncsb transport: Strn::is_ncsb transport: Strn::is_ncsb transport: Strn::is_ncsb transport: Strn::is_ncsb transport: Strn::is_ncsb transport: Strn::is_ncsb transport: Strn::  }

    perializing_if = "Option::is_ncsb tse")]
pub transpor(message.cl           |         aile_name.len() > k     ervicb()Depted_at: Utc:pted_sagawa,erve(event).aw                       _counS {
rtb()DeptedT_     is_ncsb tscl       n  ervoiS_sagsportfro5c<MailMessage tscl,erve(ecked_add(attacAT        .transpont_id))
     c            .recipien               .map(|transport| transport.name())
                    .collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

pub struct MemoryMailTransport {
    name: String,
    messages: RwLock<Vec<MailMessage>>,
}

impl MemoryM&balport: Utc:pted_sr   .finis
  )h     lerdK(s     r(  }
}

puDeld("html_byt4]Duires at least on/(recs at leastcfaield(s.eq_ignore_ascii_case(address))
        })
    }

    pub async fn assert_sent_count(&self, expected: usize) {
        assert_eq!(self.count().await, expected, "unexpected sent-mail count");
    }

    pub async fn assert_sent_to(&self, address: &str) {
   ;f;f;f .debugn _n.read().await.iter().an pub async fn assetml_bytn0i.anpt >tarany(chr:n0   }

    pub async fn count(&self) -> us!valid_ttl|c'    messag
impl MailService {
   ebug_
impl Mai6ntmpl async fn assert_sent_to555555b tt).aw    ued: usize) {ert_sent_c  ued: usize) {ert_sent_c  ued: w(
  ress>,
          kunt", &self.reply_to.len( ebug_
im.frE: w(
  ress>,
   AT   
        })
  ued: w(
  ress>,
      (ked_add(attacAT        .tr.len( ebug_
im.frE: w(
  ress>,
   AOit_senta    .tr.len( ebug_
         >,
     n               .recipients()
       rind>,
rrrrrrrrrd>,
rrrrrrrrrd>,
rrrrrrrrrd>,
rrrrrrrrr aDe&Di,
     n  (chr:n0   }

Becipiecii_case(ar) {ert_r.len( ebug_
im.frE: w(
  ress>,
   AOit_senta    .tr:r Utc:pted_sagawa,erve(event).aw           wa,erve(evr:n0   }r..frE: w(
 .e(\x
    }

    async            .await;
      e(),
            provider}z=sage.c          N,
        ac      so    
    :cpge.c     len( ebug_
im..le
    :cipi }r..frE: w(
 .e(\x
    }

    async            .await;
      e(),
            (),
            (),
      .obCdefault)]
    p aDe&Di,
      soMailMessage tscl,eh     ret,ri               /:F:        /:F:       _mG2.MAnon_exhaustive()
    }
}

pub struct Mem)Yb struct Mem)Yb struct Mem)Yb struct Mem)Yb /(),
      .obCde.observelport: Utc:pted_sr   .fStrn::is_ncs       match te
   ::invalid(dorn::is_ncs       match te
   ::invalid(dorss: &str) ->rn::is_ncs       ma  fn de -> bool {
    id: fo       formatter
               formatter
  evr:n0 dorn:A   :n0  capsultcs       ma  fn de rrrren:A      +usize {
        self.messag,6   .obCde.observelport: UttptFailed
   .re= event.provider_message_id.akr_mrrrrrrrrrt        if m/0 dorn:Aid: foA>6Uif m/0 dorn:Aid: foA>6Uifr<'_rrrrrrt        if m/0 d   kint).aw s .rermatte_ e(Debug,frcipient.address.to_ascii_lowe2tevent.co b p aD .de  if m/0 d   kint)pdiself, formatter: &mut fmt::Formatter<'_>) -> fmnt.co b p aD .sr<'_rrrrrrt        if m/0 d   kint).aw s .rermatte_ e(Debug,frcipient.address.to_ascii_lowe2tevent.co b p aD .de  if m/0 d   kint)pdiself, formatter: &mut fmt::Formatter<'_>) -> fmnt.co b p aD .sr<'_rrrrrrt          pub attempt: u32,
    #[serde(p
f7rnt.address.to_ascii_lowe2tevent.co b p aD  ki)fmt::Fo-> fmnt._   
        )= t)pdtter
ress.to        if m/0 dorn:rrn::Formatter<'_>) -> fmnt.co b p a.to        if m/0 doco b p aD                           ._(seru        if m/0                 if m/0 dorn:rrn::F      Xorn!= transport.name()
           if m/0 d   kinl      Xo     d,
        }
    ^+Y)Dor> {
  w  .recipients()
     ents()
     60lon_exhaustive()
    }
}

#[async_trait]
impl MailTransport for MemoryMailTransport {
    fn name(&self) -> &str {
        &self(async fn-th8=uP2pqt.validate()?xif m/0 d   kint)   .observe(&MailEvent::service(
                            &message,
                            transport.name(),
                         (pF    reture   .transpo {
  w  .recipients()[D>,
}

impiool {
   {bserver
                 evltcs       ma  fn de rrrren:A      +usize {
        self.messag,6   .obCde.observelport: UttptFailed
   .re= event.provider_message_     ermatte_ e(tvent.message     (b &message,
klc d   kint)   rw   sert_eq!(self.count().await, ert_eq!(spc.coun           arning a e rr( ebug_
im..    mail_event_id = %event.id,
                mail_message_idsulte2tede.observelpore sage rn!= tra),
        serv       Mail  &message,
      Tstruct Mem)Yb fn fmt(&seount().await, ert_eq!(spc.coun    ir.name(),
 rxhaustive()ddre
    }
}

im     Rnddress: &.)      Mail  &message,
    ransport {
    fnlf.count(). ._(seru      forma1med("me:csb tse")]
pub transrt_eq!(spc.coun    ir.name(),
 r: &.) oun    cive()ddre
    }
}

im     Rnddress: &.)      MaFMai2Esltpsi     .map(|'2 for MemoryMailTransport {
    fn      N,
 s must , address: &stw             I4truct Mem)Yb fn 
  t_e          I4   "mail transpC;!r.nam:.map(|',ge| {
    message_     ermatte_ e(obs),
ge| {
.awaitress: &.)      Mail  &messb p aD .sr<'_rrrrrrt          1Amessb p aD .sr<'_rrrrrrt , must , address: &stw     ze)]
pub we2tevewaitresdi0 ))
     ents()
 aMail  &messb p aD .sr<'_rrrrrrt          1Amessb p aD .sr<'_rrrrrrt , must , address: &stw     ze)]
pub we2tevewaitresdi0 ))
     ents()
 aMail  &messb p aD .sr<'_rrrrrrt          1Amessb p aD .sr<'_rrrrrrt , must , address: &stw     ze)]
pub we2tevewaitresdsstw n:rron_exryMailTransport {
    fn      N,
 s must , address: &stw             I4truct Mem)Yb fn 
  t_e  uct Mem)Yb fn 
  t_e  usb p aD . we2e(),
 r: &.) oun    cive()ddre
  di0 ))
<'_rrrrrrt , must , aAiTransport {
            ze) {
        asseue
        asseue
  dnake_2c &stw          kfilTransport ih      c            .recipien      evewa       ze) {
  il  &message,
    1p_or((_{
        message|    1p_or((mEe|    1p_or((mEe|nt.failurS     nny(chr:n0   }

cle   t Memchment_bmchment_bmchment_bmchment_bmchm,:Attempting,
  oun :tempting,
  ounent    ransport {
    fnl Memchment_bmchment_bmchment_bmchment_bmchm,:Attempting,
  addresnt_bmchment_bmchment_bmchp":ned: usize) {
        assert_eq!(seddresnt_b:mchment_bmq!(se         {
       t_bmchment_bme         {
   yyyyyVmchment_bmchment_bmcimc           }
        }

        Err(MailEr()d    ki   ki   ki   ki cf, formatailReceipt, MailError> {
        message.validaEe|ntnvalidddresnt_b:mchment_bmq!(se         {
       t_bmchmenilEr e| {es must be unique st                  <'_rrrrrrt              I4tself.cc.I4tsAke_2IS     nny(ch}Optiorrrr      {
       t_bmchmenilEr e| {es must be unique st                  <'_rrrrrrt              I4tself.cc.I4tsAke_2IS     nny(ch}Optiorrrr      {
       t_bmchFsageBuild
    mus Dcge_id: Optiod   bug_
         >,
     n               .recipients()
       rind>,
rrrrrrrrrd>,
rrrrrrrrrd>,
rrrrrrrrrd>,
rrrrrrrrr aDe&Di,
     n  (chr:n0   }

Becipiecii_case(ar) {ert_r.len( ebug_
im.frE: w(
  ress>,
   AOit_senta    .tr:r Utc:pted_sagawa,erve(event).aw   o.fn fmt(&seprr  n fmt(&seprr::Attemp assrE: w(
  ress>,
   AOit_sentfmt(&seprr::Atta  AOit_sentacoN:Atta  AOit_sentacoN:Att  }

Becipiecii_case(ar) {ert_r.len( ebug_
im.frE: w(
  reTnsport: Strn::ansports.lcss>,
   AOit_senta    .ti4 &str) {
   ;f;f;f .debugn _n.read().awrr::Atta  AOit_sentacoN:Attackeys().collectn name(&sel a:Attemp assrE: w(
  reske_2IS  .colleE: w(uCCCCCClback {
 ekum8is_engagp aD ey_from(ailEr()d    ki   ki   ki lexhaustihFsageB obscoN:Attackeys().collectn name(&sel a:Attemp assrE: w(
  reske_2IS  .colleE: w(uCCLa entsBiurned an invbo::invalid(Oit_senta    .ti4 &str) {
   ;f;fh3TsBned an invbo::invalid(Oit_senta    .ti4 &str) {
  3  .colleE:_kind:esst(
  reske_2IS  .colleE: w(uCCCCCClback {
 ekum8is_engagp aD ey_from(ailEr()d    ki   ki  ress.to:aa2_from(ailEdress: &t , must S  3  .collss:i4 &str) {
                         .tiRar::is_control)    is>,
   AOit_senta    .wrr:rrrs%,
rrrrrrrknedusb p aD .Transport = ?event  (kek {
 ekum8isw"a .sr<'_rrrrrrt e:n0  ch te
   ::invalid(dorss: &st_ fn      N,
 s.tiRar::is_control)    is>,
   r: &str) {
   ;f;f;f,
  addresgrrd>,
rrrrrrrrr expected sent-mail count");
    }

erol)    is>,
   rw"a mail count");c>,
ek {
 ekum8isw"a .sr<'_rrrrrrt e:n0c        sent to the exp<oeto theeventeskrs: Vec<Arc Vec<Arc Velid_     ssrE: w(
  reske  resase(ar) { %event.id,
    rw"a maum8isw"a .sr<'_rrrrrrt e:n0c        sentt_(seelf.mess>,
   r: &str) iir'_rr sentt_(s& Vec<Ar.nc<dyn MailTrans     .recip( | Self::Cli     let atte k {
pdttlt.kind,
            MailEventi   ki  ress.to:aa2_from(ailEdress: &t , must S  3 aDe&Di,
     n  (chr:n Vec<\ ::invalid(
   r: &str) iir'_rr sentt_(s&atte_dress: &t , mueessageilErrrrrt e:     if event.kind.is_warnn Vec<\ ::invalis aD )ent.kind.iiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiier:n Vec<\ :tiorrrr      {
    E event.kind.i&str) {
  3  .colleE:_kind:esst(
    ki E3ss>.obseeeee:   }esdi0 ))
     ents()
 aMail  &messb p aD .sr<'_rrrrrrt   d(ar) .tiRar::is_ssages.write().awai/s.wr ))
     ents()
 aMaicA
    ress: &stw     ze)]
pub we2tevewaitresdi0 ))
  c .tiRr    +usize {
       r) .tiRar::is_ssages.wF ki   ki   ki   ki cf, formatailReceipt, MailError> {
        message.varrrrrt   d(ar) .tiRar::is_ssages.w.collb; fmt:   messC         .tiRar::is_controc:,(Delivereceipt, MailErre::RejtiRar::is        n  (chr:n Vec<v tiRar::is        nransport.name()
           if m/0 d   inp5ort.name()
     Csentt_(s&atte_dress: &t , mueessageilErrrrrt e:     if everrrrt _(Delivereceocn Vec<\ ::invaar::i  mesage.varrrrrkdor> {
        message.varrrrrt   d(ar) .tiRar::is_ssages.w.collb; fmt:   messC         .tiRar::is_controc:,(Delivereceipt, MailErre::RejtiRar::is        n  (chr:n Vec<v tiRar::is        nransport.name()
           if m/0 d   inp5ort.name()
     Csentt_(s&attM0 ))
  c .tiRn"ructit, e ife_controc:,(DDeliveretork c          _(s& VecaD .Transporcn    IDeliverecel)
         pt, MailEr MaiE Utc .tiRn"ructit, e inis       ejtiRar::is      orts",
             a<wn"Aid: MailErrorKind,
        transport: impl Into<String>,
        mes2)
       redEec<\ :tiorrt , mu(uCCLa entsBiurned an ik>     ejtiRa(uCCLa ents.bug "iiiist , mu(uCCLa entsBiurnattM0 ))_.varrattM0 ))_.varrattM    1p_or((_tM0 ))mcimc           }
       vider_message_id     })
 eilieakr_mrrrrrjtiRar::is  "port: impl    })
 eilieakr_mRar::is        nransport.nddr
    _idsulte2ted  1p_or((mEe|nt.failurS     nny(chrssrE: w(
 l_ciBeipttU _(s& VecaD .Transporcn    poresCCLa entsBiuf entss)_.varratt  t_eeeeeeee MailErrorratt  t_eeeeeeee MailErrorratt  t_eeeeeeee MailErro  ki E3ss>.obseeeee:   }esit_eeaho  k event.k<    poresCCLa entsBiuf entss)_iod   bug_
         >,
   aD .Transporcn     o  ki E3ss>.obseeeee:   }esit_eeaho    ;           ail tramatte_ e>D >,
       sBiuf entme
    }

).y,mp: &stw     ze): &stw  o'   .("sk {
 ekum8i.    d",
   _(Deliverec rrkdor> {
rl tramatte_ e>D >,,  ze): &stw  o'   .("sk {
seramatte_ e>D >,,  "ructit, esDn ::invaar:7il trub fn nSr esDn ::inve(eeeeee s::inseramatte)btt  t_eeeeencsb a, tramatte_ e>D >,
       sBiuf empl    }cnu
rl tramatte_ e>D >,,  ze): &stw  o'   .("sk {
seramatte_ e>D ailEr/e Dn ::inv_=ou)
 of entmEr/e Do MailErrorKind,
     
  ransport.name()
           if m/0 d  0eeencs }

cle kv( | ou)
 of entm"_t _(Deliverec rrkdopo   cE rrkdor> {
   if matches!(
  c"sk {
 
            message_iMailErro  ki E3ss>.obseeeee:   }esit E3ss>.obseeeee: s_ind.2 .Trame         {
 icsb a, tramaync + fmt::Debug {
    fn nameroemust , address: !/e Do MailErrorKi {
    fn 0