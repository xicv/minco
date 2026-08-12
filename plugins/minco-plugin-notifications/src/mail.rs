use crate::{Notification, NotificationChannel, NotificationError, NotificationSink};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt::{self, Write as _},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    sync::{Mutex, RwLock},
    task::JoinSet,
    time::timeout,
};
use uuid::Uuid;

const MAX_RECIPIENTS: usize = 50;
const MAX_BODY_BYTES: usize = 1_000_000;
const MAX_ATTACHMENT_COUNT: usize = 32;
const MAX_ATTACHMENT_BYTES: usize = 25 * 1024 * 1024;
const MAX_RENDERED_MESSAGE_BYTES: usize = 39_000_000;
const MAX_HEADERS: usize = 15;
const MAX_USER_TAGS: usize = 48;
const MAX_METADATA_BYTES: usize = 64 * 1024;
const MAX_PROVIDER_MESSAGE_ID_BYTES: usize = 512;
const MAX_TRACING_DELIVERY_DEDUPE_IDS: usize = 4_096;
const OBSERVER_TIMEOUT: Duration = Duration::from_millis(100);
const OBSERVER_CHILD_TIMEOUT: Duration = Duration::from_millis(75);
const MAX_OBSERVERS: usize = 16;
const HEADER_SOFT_LINE_BYTES: usize = 78;
const HEADER_HARD_LINE_BYTES: usize = 998;
const ENCODED_WORD_INPUT_BYTES: usize = 45;
const RESERVED_TAGS: [&str; 2] = ["minco_message_id", "minco_topic"];
const RESERVED_HEADERS: [&str; 17] = [
    "bcc",
    "cc",
    "content-transfer-encoding",
    "content-type",
    "date",
    "dkim-signature",
    "from",
    "message-id",
    "mime-version",
    "received",
    "reply-to",
    "return-path",
    "sender",
    "subject",
    "to",
    "x-minco-message-id",
    "x-minco-topic",
];

#[derive(Clone, PartialEq, Eq)]
pub struct MailAddress {
    pub address: String,
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

    pub fn named(address: impl Into<String>, name: impl Into<String>) -> Result<Self, MailError> {
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
            name.trim().is_empty() || name.len() > 256 || name.chars().any(char::is_control)
        }) {
            return Err(MailError::invalid("mail display name is invalid"));
        }
        Ok(())
    }

    pub fn formatted(&self) -> String {
        match &self.name {
            None => self.address.clone(),
            Some(name) if name.is_ascii() && name.len() <= 60 => {
                let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{escaped}\" <{}>", self.address)
            }
            Some(name) => format!("{} <{}>", encode_header_words(name), self.address),
        }
    }

    pub fn domain(&self) -> &str {
        self.address
            .rsplit_once('@')
            .map_or("localhost", |(_, domain)| domain)
    }

    fn normalized_key(&self) -> String {
        let (local, domain) = self
            .address
            .rsplit_once('@')
            .expect("validated mail address contains @");
        format!("{local}@{}", domain.to_ascii_lowercase())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailAttachmentDisposition {
    Attachment,
    Inline,
}

#[derive(Clone, PartialEq, Eq)]
pub struct MailAttachment {
    pub file_name: String,
    pub content_type: String,
    pub content: Vec<u8>,
    pub disposition: MailAttachmentDisposition,
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
            return Err(MailError::invalid(
                "mail attachment content type is invalid",
            ));
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
            .field(
                "content_id",
                &self.content_id.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct MailMessage {
    pub id: Uuid,
    pub topic: String,
    pub to: Vec<MailAddress>,
    pub cc: Vec<MailAddress>,
    pub bcc: Vec<MailAddress>,
    pub reply_to: Vec<MailAddress>,
    pub subject: String,
    pub text: Option<String>,
    pub html: Option<String>,
    pub attachments: Vec<MailAttachment>,
    pub headers: BTreeMap<String, String>,
    pub tags: BTreeMap<String, String>,
    pub metadata: BTreeMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl MailMessage {
    pub fn builder(topic: impl Into<String>, subject: impl Into<String>) -> MailMessageBuilder {
        MailMessageBuilder {
            message: Self {
                id: Uuid::now_v7(),
                topic: topic.into(),
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
        if self.id.is_nil() || !valid_topic(&self.topic) {
            return Err(MailError::invalid("mail identity or topic is invalid"));
        }
        if self.subject.trim().is_empty()
            || self.subject.len() > 998
            || self.subject.chars().any(char::is_control)
        {
            return Err(MailError::invalid("mail subject is invalid"));
        }

        let recipient_count = self.to.len() + self.cc.len() + self.bcc.len();
        if recipient_count == 0 || recipient_count > MAX_RECIPIENTS {
            return Err(MailError::invalid(
                "mail message must contain between 1 and 50 recipients",
            ));
        }
        let mut unique = BTreeSet::new();
        for recipient in self.recipients() {
            recipient.validate()?;
            if !unique.insert(recipient.normalized_key()) {
                return Err(MailError::invalid(
                    "mail recipient lists must not contain duplicate mailboxes",
                ));
            }
        }
        if self.reply_to.len() > 10 {
            return Err(MailError::invalid(
                "mail message exceeds the reply-to address boundary",
            ));
        }
        for reply_to in &self.reply_to {
            reply_to.validate()?;
        }

        match (&self.text, &self.html) {
            (None, None) => {
                return Err(MailError::invalid(
                    "mail message requires a text or HTML body",
                ));
            }
            (Some(text), _) if !valid_body(text) => {
                return Err(MailError::invalid("mail text body is invalid"));
            }
            (_, Some(html)) if !valid_body(html) => {
                return Err(MailError::invalid("mail HTML body is invalid"));
            }
            _ => {}
        }

        if self.attachments.len() > MAX_ATTACHMENT_COUNT {
            return Err(MailError::invalid(
                "mail message exceeds the attachment count boundary",
            ));
        }
        let mut attachment_bytes = 0_usize;
        let mut content_ids = BTreeSet::new();
        for attachment in &self.attachments {
            attachment.validate()?;
            attachment_bytes = attachment_bytes
                .checked_add(attachment.content.len())
                .ok_or_else(|| MailError::invalid("mail attachment size overflow"))?;
            if let Some(content_id) = &attachment.content_id
                && !content_ids.insert(content_id.to_ascii_lowercase())
            {
                return Err(MailError::invalid(
                    "mail inline attachment content IDs must be unique",
                ));
            }
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
            .field("to_count", &self.to.len())
            .field("cc_count", &self.cc.len())
            .field("bcc_count", &self.bcc.len())
            .field("reply_to_count", &self.reply_to.len())
            .field("subject", &"[REDACTED]")
            .field("text_bytes", &self.text.as_ref().map(String::len))
            .field("html_bytes", &self.html.as_ref().map(String::len))
            .field("attachment_count", &self.attachments.len())
            .field("header_count", &self.headers.len())
            .field("tag_count", &self.tags.len())
            .field("metadata_count", &self.metadata.len())
            .field("created_at", &self.created_at)
            .finish()
    }
}

#[must_use]
#[derive(Debug, Clone)]
pub struct MailMessageBuilder {
    message: MailMessage,
}

impl MailMessageBuilder {
    pub const fn id(mut self, id: Uuid) -> Self {
        self.message.id = id;
        self
    }

    pub const fn created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.message.created_at = created_at;
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
    Protocol,
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

    pub const fn retry_advice(&self) -> MailRetryAdvice {
        match self.kind {
            MailErrorKind::Throttled | MailErrorKind::Unavailable => {
                MailRetryAdvice::SafeAfterBackoff
            }
            MailErrorKind::Ambiguous => MailRetryAdvice::ReconcileBeforeRetry,
            MailErrorKind::InvalidMessage
            | MailErrorKind::Configuration
            | MailErrorKind::Authentication
            | MailErrorKind::Rejected
            | MailErrorKind::Protocol => MailRetryAdvice::Never,
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
pub enum MailSubmissionEventKind {
    Prepared,
    Attempting,
    AttemptFailed,
    Accepted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailSubmissionEvent {
    pub event_id: Uuid,
    pub message_id: Uuid,
    pub topic: String,
    pub transport: String,
    pub kind: MailSubmissionEventKind,
    pub occurred_at: DateTime<Utc>,
    pub attempt: u32,
    pub failure_kind: Option<MailErrorKind>,
    pub duration_ms: Option<u64>,
}

impl MailSubmissionEvent {
    fn new(
        message: &MailMessage,
        transport: impl Into<String>,
        kind: MailSubmissionEventKind,
        attempt: u32,
        failure_kind: Option<MailErrorKind>,
        duration: Option<Duration>,
    ) -> Self {
        Self {
            event_id: Uuid::now_v7(),
            message_id: message.id,
            topic: message.topic.clone(),
            transport: transport.into(),
            kind,
            occurred_at: Utc::now(),
            attempt,
            failure_kind,
            duration_ms: duration.map(|value| u64::try_from(value.as_millis()).unwrap_or(u64::MAX)),
        }
    }
}

#[async_trait]
pub trait MailObserver: Send + Sync + fmt::Debug {
    async fn observe(&self, event: &MailSubmissionEvent);
}

#[derive(Debug, Default)]
pub struct NoopMailObserver;

#[async_trait]
impl MailObserver for NoopMailObserver {
    async fn observe(&self, _event: &MailSubmissionEvent) {}
}

#[derive(Debug, Default)]
pub struct MemoryMailObserver {
    events: RwLock<Vec<MailSubmissionEvent>>,
}

impl MemoryMailObserver {
    pub async fn events(&self) -> Vec<MailSubmissionEvent> {
        self.events.read().await.clone()
    }

    pub async fn clear(&self) {
        self.events.write().await.clear();
    }
}

#[async_trait]
impl MailObserver for MemoryMailObserver {
    async fn observe(&self, event: &MailSubmissionEvent) {
        self.events.write().await.push(event.clone());
    }
}

#[derive(Clone)]
pub struct CompositeMailObserver {
    observers: Vec<Arc<dyn MailObserver>>,
}

impl CompositeMailObserver {
    pub fn new(observers: Vec<Arc<dyn MailObserver>>) -> Result<Self, MailError> {
        if observers.is_empty() || observers.len() > MAX_OBSERVERS {
            return Err(MailError::new(
                MailErrorKind::Configuration,
                "mail",
                "mail observer composition must contain between 1 and 16 observers",
            ));
        }
        Ok(Self { observers })
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
    async fn observe(&self, event: &MailSubmissionEvent) {
        let mut tasks = JoinSet::new();
        for (index, observer) in self.observers.iter().cloned().enumerate() {
            let event = event.clone();
            tasks.spawn(async move {
                (
                    index,
                    timeout(OBSERVER_CHILD_TIMEOUT, observer.observe(&event))
                        .await
                        .is_ok(),
                )
            });
        }
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok((_, true)) => {}
                Ok((index, false)) => tracing::warn!(
                    target: "minco.mail",
                    mail_event_id = %event.event_id,
                    mail_event = ?event.kind,
                    mail_observer_index = index,
                    "mail observer timed out"
                ),
                Err(_) => tracing::warn!(
                    target: "minco.mail",
                    mail_event_id = %event.event_id,
                    mail_event = ?event.kind,
                    "mail observer task failed"
                ),
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct TracingMailObserver;

#[async_trait]
impl MailObserver for TracingMailObserver {
    async fn observe(&self, event: &MailSubmissionEvent) {
        if event.kind == MailSubmissionEventKind::AttemptFailed {
            tracing::warn!(
                target: "minco.mail",
                mail_event_id = %event.event_id,
                mail_message_id = %event.message_id,
                mail_topic = %event.topic,
                mail_transport = %event.transport,
                mail_event = ?event.kind,
                mail_attempt = event.attempt,
                mail_failure_kind = ?event.failure_kind,
                mail_duration_ms = event.duration_ms,
                "mail submission event"
            );
        } else {
            tracing::info!(
                target: "minco.mail",
                mail_event_id = %event.event_id,
                mail_message_id = %event.message_id,
                mail_topic = %event.topic,
                mail_transport = %event.transport,
                mail_event = ?event.kind,
                mail_attempt = event.attempt,
                mail_failure_kind = ?event.failure_kind,
                mail_duration_ms = event.duration_ms,
                "mail submission event"
            );
        }
    }
}

#[async_trait]
pub trait MailTransport: Send + Sync + fmt::Debug {
    fn name(&self) -> &str;

    async fn send(&self, message: &MailMessage, attempt: u32) -> Result<MailReceipt, MailError>;
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
            if !valid_transport_name(transport.name()) || !names.insert(transport.name().to_owned())
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

    async fn observe(&self, event: MailSubmissionEvent) {
        if timeout(OBSERVER_TIMEOUT, self.observer.observe(&event))
            .await
            .is_err()
        {
            tracing::warn!(
                target: "minco.mail",
                mail_event_id = %event.event_id,
                mail_event = ?event.kind,
                "mail observer timed out"
            );
        }
    }

    pub async fn send(&self, message: MailMessage) -> Result<MailReceipt, MailError> {
        message.validate()?;
        self.observe(MailSubmissionEvent::new(
            &message,
            "mail.service",
            MailSubmissionEventKind::Prepared,
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
            self.observe(MailSubmissionEvent::new(
                &message,
                transport.name(),
                MailSubmissionEventKind::Attempting,
                attempt,
                None,
                None,
            ))
            .await;

            let started_at = Instant::now();
            match transport.send(&message, attempt).await {
                Ok(receipt) => {
                    if let Err(error) =
                        validate_receipt(&receipt, &message, transport.name(), attempt)
                    {
                        self.observe(MailSubmissionEvent::new(
                            &message,
                            transport.name(),
                            MailSubmissionEventKind::AttemptFailed,
                            attempt,
                            Some(error.kind),
                            Some(started_at.elapsed()),
                        ))
                        .await;
                        return Err(error);
                    }
                    self.observe(MailSubmissionEvent::new(
                        &message,
                        transport.name(),
                        MailSubmissionEventKind::Accepted,
                        attempt,
                        None,
                        Some(started_at.elapsed()),
                    ))
                    .await;
                    return Ok(receipt);
                }
                Err(error) => {
                    let error = normalize_transport_error(error, transport.name());
                    self.observe(MailSubmissionEvent::new(
                        &message,
                        transport.name(),
                        MailSubmissionEventKind::AttemptFailed,
                        attempt,
                        Some(error.kind),
                        Some(started_at.elapsed()),
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

#[derive(Clone, PartialEq, Eq)]
pub struct MailAttempt {
    pub message: MailMessage,
    pub attempt: u32,
}

impl fmt::Debug for MailAttempt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MailAttempt")
            .field("message_id", &self.message.id)
            .field("topic", &self.message.topic)
            .field("attempt", &self.attempt)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
struct FakeMailFailure {
    kind: MailErrorKind,
    message: String,
}

/// Deterministic mail transport fake for retry and fallback tests.
pub struct FakeMailTransport {
    name: String,
    attempts: RwLock<Vec<MailAttempt>>,
    failures: Mutex<VecDeque<FakeMailFailure>>,
}

impl FakeMailTransport {
    pub fn named(name: impl Into<String>) -> Result<Self, MailError> {
        let name = name.into();
        if !valid_transport_name(&name) {
            return Err(MailError::new(
                MailErrorKind::Configuration,
                "fake",
                "fake mail transport name is invalid",
            ));
        }
        Ok(Self {
            name,
            attempts: RwLock::new(Vec::new()),
            failures: Mutex::new(VecDeque::new()),
        })
    }

    pub async fn fail_next(&self, kind: MailErrorKind, message: impl Into<String>) {
        self.failures.lock().await.push_back(FakeMailFailure {
            kind,
            message: message.into(),
        });
    }

    pub async fn attempts(&self) -> Vec<MailAttempt> {
        self.attempts.read().await.clone()
    }
}

impl Default for FakeMailTransport {
    fn default() -> Self {
        Self::named("fake").expect("static fake transport name")
    }
}

impl fmt::Debug for FakeMailTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FakeMailTransport")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl MailTransport for FakeMailTransport {
    fn name(&self) -> &str {
        &self.name
    }

    async fn send(&self, message: &MailMessage, attempt: u32) -> Result<MailReceipt, MailError> {
        message.validate()?;
        let sequence = {
            let mut attempts = self.attempts.write().await;
            attempts.push(MailAttempt {
                message: message.clone(),
                attempt,
            });
            attempts.len()
        };
        let failure = self.failures.lock().await.pop_front();
        if let Some(failure) = failure {
            return Err(MailError::new(failure.kind, &self.name, failure.message));
        }
        Ok(MailReceipt {
            message_id: message.id,
            transport: self.name.clone(),
            provider_message_id: format!("fake:{}:{sequence}", message.id),
            accepted_at: DateTime::<Utc>::UNIX_EPOCH,
            attempt,
        })
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
        let Ok(expected) = MailAddress::new(address) else {
            return false;
        };
        self.messages.read().await.iter().any(|message| {
            message
                .recipients()
                .any(|recipient| same_mailbox(recipient, &expected))
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

    async fn send(&self, message: &MailMessage, attempt: u32) -> Result<MailReceipt, MailError> {
        message.validate()?;
        let sequence = {
            let mut messages = self.messages.write().await;
            messages.push(message.clone());
            messages.len()
        };
        Ok(MailReceipt {
            message_id: message.id,
            transport: self.name.clone(),
            provider_message_id: format!("memory:{}:{sequence}", message.id),
            accepted_at: Utc::now(),
            attempt,
        })
    }
}

#[derive(Clone)]
pub struct LegacyNotificationMailTransport {
    sink: Arc<dyn NotificationSink>,
}

impl LegacyNotificationMailTransport {
    pub fn new(sink: Arc<dyn NotificationSink>) -> Self {
        Self { sink }
    }
}

impl fmt::Debug for LegacyNotificationMailTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LegacyNotificationMailTransport")
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl MailTransport for LegacyNotificationMailTransport {
    fn name(&self) -> &'static str {
        "legacy-notification"
    }

    async fn send(&self, message: &MailMessage, attempt: u32) -> Result<MailReceipt, MailError> {
        message.validate()?;
        if message.to.len() != 1
            || !message.cc.is_empty()
            || !message.bcc.is_empty()
            || !message.reply_to.is_empty()
            || message.html.is_some()
            || !message.attachments.is_empty()
            || !message.headers.is_empty()
            || !message.tags.is_empty()
        {
            return Err(MailError::new(
                MailErrorKind::Configuration,
                self.name(),
                "legacy notification transport accepts only one plain-text recipient",
            ));
        }
        let text = message.text.clone().ok_or_else(|| {
            MailError::new(
                MailErrorKind::Configuration,
                self.name(),
                "legacy notification transport requires a text body",
            )
        })?;
        let mut notification = Notification::new(
            message.topic.clone(),
            NotificationChannel::Email,
            message.to[0].address.clone(),
            message.subject.clone(),
            text,
        );
        notification.id = message.id;
        notification.created_at = message.created_at;
        notification.metadata = message.metadata.clone();
        self.sink
            .send(notification)
            .await
            .map_err(|error| match error {
                NotificationError::InvalidRecipient => MailError::new(
                    MailErrorKind::Rejected,
                    self.name(),
                    "legacy notification recipient was rejected",
                ),
                NotificationError::Delivery(_) => MailError::new(
                    MailErrorKind::Ambiguous,
                    self.name(),
                    "legacy notification delivery outcome is unknown",
                ),
            })?;
        Ok(MailReceipt {
            message_id: message.id,
            transport: self.name().into(),
            provider_message_id: format!("legacy:{}", message.id),
            accepted_at: Utc::now(),
            attempt,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailDeliveryEventKind {
    Submitted,
    Delivered,
    BouncedPermanent,
    BouncedTransient,
    BouncedUndetermined,
    Complaint,
    Rejected,
    DeliveryDelayed,
    RenderingFailed,
    Opened,
    Clicked,
    SubscriptionChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailDeliveryEvent {
    pub source_event_id: String,
    pub message_id: Uuid,
    pub topic: String,
    pub transport: String,
    pub kind: MailDeliveryEventKind,
    pub occurred_at: DateTime<Utc>,
    pub provider_message_id: Option<String>,
}

impl MailDeliveryEvent {
    pub fn validate(&self) -> Result<(), MailError> {
        if self.source_event_id.trim().is_empty()
            || self.source_event_id.len() > 512
            || self.source_event_id.chars().any(char::is_control)
            || self.message_id.is_nil()
            || !valid_topic(&self.topic)
            || !valid_transport_name(&self.transport)
            || self.provider_message_id.as_deref().is_some_and(|value| {
                value.trim().is_empty()
                    || value.len() > MAX_PROVIDER_MESSAGE_ID_BYTES
                    || value.chars().any(char::is_control)
            })
        {
            return Err(MailError::invalid("mail delivery event is invalid"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailDeliveryDisposition {
    Recorded,
    Duplicate,
}

#[async_trait]
pub trait MailDeliveryEventSink: Send + Sync + fmt::Debug {
    async fn record(&self, event: MailDeliveryEvent) -> Result<MailDeliveryDisposition, MailError>;
}

#[derive(Debug, Default)]
pub struct MemoryMailDeliveryEventSink {
    source_ids: RwLock<BTreeSet<String>>,
    events: RwLock<Vec<MailDeliveryEvent>>,
}

impl MemoryMailDeliveryEventSink {
    pub async fn events(&self) -> Vec<MailDeliveryEvent> {
        self.events.read().await.clone()
    }
}

#[async_trait]
impl MailDeliveryEventSink for MemoryMailDeliveryEventSink {
    async fn record(&self, event: MailDeliveryEvent) -> Result<MailDeliveryDisposition, MailError> {
        event.validate()?;
        {
            let mut source_ids = self.source_ids.write().await;
            if !source_ids.insert(event.source_event_id.clone()) {
                return Ok(MailDeliveryDisposition::Duplicate);
            }
        }
        self.events.write().await.push(event);
        Ok(MailDeliveryDisposition::Recorded)
    }
}

#[derive(Debug)]
pub struct TracingMailDeliveryEventSink {
    source_ids: RwLock<DeliveryDedupeWindow>,
}

impl Default for TracingMailDeliveryEventSink {
    fn default() -> Self {
        Self {
            source_ids: RwLock::new(DeliveryDedupeWindow::new(MAX_TRACING_DELIVERY_DEDUPE_IDS)),
        }
    }
}

#[derive(Debug)]
struct DeliveryDedupeWindow {
    source_ids: BTreeSet<String>,
    insertion_order: VecDeque<String>,
    capacity: usize,
}

impl DeliveryDedupeWindow {
    fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "delivery dedupe capacity must be positive");
        Self {
            source_ids: BTreeSet::new(),
            insertion_order: VecDeque::new(),
            capacity,
        }
    }

    fn record_if_new(&mut self, source_event_id: &str) -> bool {
        if self.source_ids.contains(source_event_id) {
            return false;
        }
        if self.source_ids.len() == self.capacity
            && let Some(oldest) = self.insertion_order.pop_front()
        {
            self.source_ids.remove(&oldest);
        }
        let source_event_id = source_event_id.to_owned();
        self.source_ids.insert(source_event_id.clone());
        self.insertion_order.push_back(source_event_id);
        true
    }
}

#[async_trait]
impl MailDeliveryEventSink for TracingMailDeliveryEventSink {
    async fn record(&self, event: MailDeliveryEvent) -> Result<MailDeliveryDisposition, MailError> {
        event.validate()?;
        {
            let mut source_ids = self.source_ids.write().await;
            if !source_ids.record_if_new(&event.source_event_id) {
                return Ok(MailDeliveryDisposition::Duplicate);
            }
        }
        let source_event_digest = deterministic_mail_event_id(&[&event.source_event_id]);
        match event.kind {
            MailDeliveryEventKind::Submitted
            | MailDeliveryEventKind::Delivered
            | MailDeliveryEventKind::Opened
            | MailDeliveryEventKind::Clicked
            | MailDeliveryEventKind::SubscriptionChanged => tracing::info!(
                target: "minco.mail",
                mail_source_event_digest = %source_event_digest,
                mail_message_id = %event.message_id,
                mail_topic = %event.topic,
                mail_transport = %event.transport,
                mail_delivery_event = ?event.kind,
                "mail delivery event"
            ),
            _ => tracing::warn!(
                target: "minco.mail",
                mail_source_event_digest = %source_event_digest,
                mail_message_id = %event.message_id,
                mail_topic = %event.topic,
                mail_transport = %event.transport,
                mail_delivery_event = ?event.kind,
                "mail delivery event"
            ),
        }
        Ok(MailDeliveryDisposition::Recorded)
    }
}

pub fn deterministic_mail_event_id(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update(u64::try_from(part.len()).unwrap_or(u64::MAX).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("sha256:{}", lower_hex(&digest.finalize()))
}

pub fn render_mime(message: &MailMessage, from: &MailAddress) -> Result<Vec<u8>, MailError> {
    message.validate()?;
    from.validate()?;

    let mut rendered = String::new();
    write_address_header(&mut rendered, "From", std::slice::from_ref(from))?;
    if !message.to.is_empty() {
        write_address_header(&mut rendered, "To", &message.to)?;
    }
    if !message.cc.is_empty() {
        write_address_header(&mut rendered, "Cc", &message.cc)?;
    }
    if !message.reply_to.is_empty() {
        write_address_header(&mut rendered, "Reply-To", &message.reply_to)?;
    }
    write_unstructured_header(&mut rendered, "Subject", &message.subject)?;
    writeln_crlf(
        &mut rendered,
        &format!("Date: {}", message.created_at.to_rfc2822()),
    );
    writeln_crlf(
        &mut rendered,
        &format!("Message-ID: <{}@{}>", message.id, from.domain()),
    );
    writeln_crlf(&mut rendered, "MIME-Version: 1.0");
    writeln_crlf(
        &mut rendered,
        &format!("X-Minco-Message-ID: {}", message.id),
    );
    writeln_crlf(&mut rendered, &format!("X-Minco-Topic: {}", message.topic));
    for (name, value) in &message.headers {
        write_ascii_header(&mut rendered, name, value)?;
    }
    rendered.push_str(&render_body_entity(message));

    if rendered
        .split("\r\n")
        .any(|line| line.len() > HEADER_HARD_LINE_BYTES)
    {
        return Err(MailError::invalid(
            "rendered mail contains a header line above the RFC hard boundary",
        ));
    }

    let bytes = rendered.into_bytes();
    if bytes.len() > MAX_RENDERED_MESSAGE_BYTES {
        return Err(MailError::invalid(
            "rendered mail exceeds the 39 MB provider boundary",
        ));
    }
    Ok(bytes)
}

fn render_body_entity(message: &MailMessage) -> String {
    let regular = message
        .attachments
        .iter()
        .filter(|attachment| attachment.disposition == MailAttachmentDisposition::Attachment)
        .collect::<Vec<_>>();
    let inline = message
        .attachments
        .iter()
        .filter(|attachment| attachment.disposition == MailAttachmentDisposition::Inline)
        .collect::<Vec<_>>();

    let mut entity = render_alternative_entity(message);
    if !inline.is_empty() {
        let boundary = format!("minco-related-{}", message.id.simple());
        let mut related = multipart_header("related", &boundary);
        append_part(&mut related, &boundary, &entity);
        for attachment in inline {
            append_part(
                &mut related,
                &boundary,
                &render_attachment_entity(attachment),
            );
        }
        finish_multipart(&mut related, &boundary);
        entity = related;
    }
    if !regular.is_empty() {
        let boundary = format!("minco-mixed-{}", message.id.simple());
        let mut mixed = multipart_header("mixed", &boundary);
        append_part(&mut mixed, &boundary, &entity);
        for attachment in regular {
            append_part(&mut mixed, &boundary, &render_attachment_entity(attachment));
        }
        finish_multipart(&mut mixed, &boundary);
        entity = mixed;
    }
    entity
}

fn render_alternative_entity(message: &MailMessage) -> String {
    match (&message.text, &message.html) {
        (Some(text), Some(html)) => {
            let boundary = format!("minco-alternative-{}", message.id.simple());
            let mut alternative = multipart_header("alternative", &boundary);
            append_part(
                &mut alternative,
                &boundary,
                &render_text_entity("text/plain", text),
            );
            append_part(
                &mut alternative,
                &boundary,
                &render_text_entity("text/html", html),
            );
            finish_multipart(&mut alternative, &boundary);
            alternative
        }
        (Some(text), None) => render_text_entity("text/plain", text),
        (None, Some(html)) => render_text_entity("text/html", html),
        (None, None) => unreachable!("validated mail has a body"),
    }
}

fn render_text_entity(content_type: &str, body: &str) -> String {
    let mut rendered = String::new();
    writeln_crlf(
        &mut rendered,
        &format!("Content-Type: {content_type}; charset=UTF-8"),
    );
    writeln_crlf(&mut rendered, "Content-Transfer-Encoding: base64");
    rendered.push_str("\r\n");
    rendered.push_str(&base64_lines(body.as_bytes()));
    rendered
}

fn render_attachment_entity(attachment: &MailAttachment) -> String {
    let mut rendered = String::new();
    writeln_crlf(
        &mut rendered,
        &format!("Content-Type: {}", attachment.content_type),
    );
    writeln_crlf(&mut rendered, "Content-Transfer-Encoding: base64");
    let disposition = match attachment.disposition {
        MailAttachmentDisposition::Attachment => "attachment",
        MailAttachmentDisposition::Inline => "inline",
    };
    writeln_crlf(
        &mut rendered,
        &format!(
            "Content-Disposition: {disposition}; filename*=UTF-8''{}",
            percent_encode(&attachment.file_name)
        ),
    );
    if let Some(content_id) = &attachment.content_id {
        writeln_crlf(&mut rendered, &format!("Content-ID: <{content_id}>"));
    }
    rendered.push_str("\r\n");
    rendered.push_str(&base64_lines(&attachment.content));
    rendered
}

fn multipart_header(subtype: &str, boundary: &str) -> String {
    format!("Content-Type: multipart/{subtype}; boundary=\"{boundary}\"\r\n\r\n")
}

fn append_part(target: &mut String, boundary: &str, part: &str) {
    let _ = write!(target, "--{boundary}\r\n{part}");
    if !target.ends_with("\r\n") {
        target.push_str("\r\n");
    }
}

fn finish_multipart(target: &mut String, boundary: &str) {
    let _ = write!(target, "--{boundary}--\r\n");
}

fn base64_lines(bytes: &[u8]) -> String {
    let encoded = STANDARD.encode(bytes);
    let mut rendered = String::with_capacity(encoded.len() + encoded.len() / 76 * 2 + 2);
    for chunk in encoded.as_bytes().chunks(76) {
        rendered.push_str(std::str::from_utf8(chunk).expect("base64 is ASCII"));
        rendered.push_str("\r\n");
    }
    rendered
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(*byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn write_address_header(
    target: &mut String,
    name: &str,
    addresses: &[MailAddress],
) -> Result<(), MailError> {
    let prefix = format!("{name}: ");
    target.push_str(&prefix);
    let mut line_bytes = prefix.len();
    for (index, address) in addresses.iter().enumerate() {
        let formatted = address.formatted();
        if formatted.len() + 1 > HEADER_HARD_LINE_BYTES {
            return Err(MailError::invalid(
                "rendered mail address exceeds the RFC header boundary",
            ));
        }
        if index > 0 {
            if line_bytes + 2 + formatted.len() > HEADER_SOFT_LINE_BYTES {
                target.push_str(",\r\n ");
                line_bytes = 1;
            } else {
                target.push_str(", ");
                line_bytes += 2;
            }
        }
        if line_bytes + formatted.len() > HEADER_HARD_LINE_BYTES {
            return Err(MailError::invalid(
                "rendered mail address header exceeds the RFC hard boundary",
            ));
        }
        target.push_str(&formatted);
        line_bytes += formatted.len();
    }
    target.push_str("\r\n");
    Ok(())
}

fn write_unstructured_header(
    target: &mut String,
    name: &str,
    value: &str,
) -> Result<(), MailError> {
    if value.is_ascii() && name.len() + value.len() + 2 <= HEADER_SOFT_LINE_BYTES {
        return write_ascii_header(target, name, value);
    }
    let encoded = encode_header_words(value);
    let prefix = format!("{name}: ");
    target.push_str(&prefix);
    let mut line_bytes = prefix.len();
    for (index, word) in encoded.split(' ').enumerate() {
        if index > 0 {
            if line_bytes + 1 + word.len() > HEADER_SOFT_LINE_BYTES {
                target.push_str("\r\n ");
                line_bytes = 1;
            } else {
                target.push(' ');
                line_bytes += 1;
            }
        }
        if line_bytes + word.len() > HEADER_HARD_LINE_BYTES {
            return Err(MailError::invalid(
                "rendered unstructured header exceeds the RFC hard boundary",
            ));
        }
        target.push_str(word);
        line_bytes += word.len();
    }
    target.push_str("\r\n");
    Ok(())
}

fn write_ascii_header(target: &mut String, name: &str, value: &str) -> Result<(), MailError> {
    let prefix = format!("{name}: ");
    target.push_str(&prefix);
    let mut line_bytes = prefix.len();
    let mut remaining = value;
    while line_bytes + remaining.len() > HEADER_SOFT_LINE_BYTES {
        let available = HEADER_SOFT_LINE_BYTES.saturating_sub(line_bytes);
        let split = remaining
            .get(..available.min(remaining.len()))
            .and_then(|candidate| candidate.rfind(' '));
        let Some(split) = split.filter(|split| *split > 0) else {
            break;
        };
        target.push_str(&remaining[..split]);
        target.push_str("\r\n ");
        remaining = &remaining[split + 1..];
        line_bytes = 1;
    }
    if line_bytes + remaining.len() > HEADER_HARD_LINE_BYTES {
        return Err(MailError::invalid(
            "rendered custom header exceeds the RFC hard boundary",
        ));
    }
    target.push_str(remaining);
    target.push_str("\r\n");
    Ok(())
}

fn encode_header_words(value: &str) -> String {
    let mut words = Vec::new();
    let mut remaining = value;
    while !remaining.is_empty() {
        let mut end = remaining.len().min(ENCODED_WORD_INPUT_BYTES);
        while !remaining.is_char_boundary(end) {
            end -= 1;
        }
        let (chunk, rest) = remaining.split_at(end);
        words.push(format!("=?UTF-8?B?{}?=", STANDARD.encode(chunk.as_bytes())));
        remaining = rest;
    }
    words.join(" ")
}

fn writeln_crlf(target: &mut String, value: &str) {
    target.push_str(value);
    target.push_str("\r\n");
}

fn validate_receipt(
    receipt: &MailReceipt,
    message: &MailMessage,
    transport: &str,
    attempt: u32,
) -> Result<(), MailError> {
    if receipt.message_id != message.id
        || receipt.transport != transport
        || receipt.attempt != attempt
        || receipt.provider_message_id.trim().is_empty()
        || receipt.provider_message_id.len() > MAX_PROVIDER_MESSAGE_ID_BYTES
        || receipt.provider_message_id.chars().any(char::is_control)
    {
        return Err(MailError::new(
            MailErrorKind::Ambiguous,
            transport,
            "mail transport returned an invalid acceptance receipt",
        ));
    }
    Ok(())
}

fn normalize_transport_error(error: MailError, transport: &str) -> MailError {
    if error.transport == transport {
        error
    } else {
        MailError::new(
            MailErrorKind::Protocol,
            transport,
            "mail transport returned an error for a different transport",
        )
    }
}

fn same_mailbox(left: &MailAddress, right: &MailAddress) -> bool {
    left.normalized_key() == right.normalized_key()
}

fn validate_email_address(value: &str) -> Result<(), MailError> {
    if value.len() > 254
        || !value.is_ascii()
        || value
            .chars()
            .any(|character| character.is_control() || character.is_ascii_whitespace())
        || value.matches('@').count() != 1
    {
        return Err(MailError::invalid("mail address is invalid"));
    }
    let (local, domain) = value
        .rsplit_once('@')
        .ok_or_else(|| MailError::invalid("mail address is invalid"))?;
    if local.is_empty()
        || local.len() > 64
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
        || !local.bytes().all(valid_local_byte)
        || domain.is_empty()
        || domain.len() > 253
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
        return Err(MailError::invalid("mail address is invalid"));
    }
    Ok(())
}

const fn valid_local_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'/'
                | b'='
                | b'?'
                | b'^'
                | b'_'
                | b'`'
                | b'{'
                | b'|'
                | b'}'
                | b'~'
        )
}

fn valid_topic(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_transport_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_body(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_BODY_BYTES && !value.contains('\0')
}

fn valid_content_type(value: &str) -> bool {
    let Some((kind, subtype)) = value.split_once('/') else {
        return false;
    };
    !kind.is_empty()
        && !subtype.is_empty()
        && value.len() <= 127
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#' | b'$' | b'&' | b'+' | b'-' | b'.' | b'^' | b'_'
                )
                || byte == b'/'
        })
}

fn valid_content_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b'<' | b'>' | b'"' | b'\\'))
}

fn valid_header(name: &str, value: &str) -> bool {
    !name.is_empty()
        && name.len() <= 78
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && !RESERVED_HEADERS
            .iter()
            .any(|reserved| name.eq_ignore_ascii_case(reserved))
        && !name.to_ascii_lowercase().starts_with("x-ses-")
        && !value.is_empty()
        && value.len() <= 998
        && value.bytes().all(|byte| matches!(byte, 32..=126))
}

fn valid_tag_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn sanitize_diagnostic(value: &str, max_bytes: usize) -> String {
    let mut sanitized = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    while sanitized.len() > max_bytes {
        sanitized.pop();
    }
    if sanitized.trim().is_empty() {
        "unspecified mail error".into()
    } else {
        sanitized
    }
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use mail_parser::MessageParser;
    use std::collections::VecDeque;
    use tokio::sync::Mutex;

    fn message() -> MailMessage {
        MailMessage::builder("account.welcome", "Welcome")
            .to(MailAddress::new("person@example.com").unwrap())
            .text("Welcome")
            .build()
            .unwrap()
    }

    #[test]
    fn address_deduplication_preserves_local_part_case() {
        let message = MailMessage::builder("topic", "Subject")
            .to(MailAddress::new("Person@Example.com").unwrap())
            .cc(MailAddress::new("person@example.com").unwrap())
            .text("Body")
            .build();
        assert!(message.is_ok());

        let duplicate = MailMessage::builder("topic", "Subject")
            .to(MailAddress::new("person@Example.com").unwrap())
            .cc(MailAddress::new("person@example.com").unwrap())
            .text("Body")
            .build();
        assert!(duplicate.is_err());
    }

    #[test]
    fn mime_omits_bcc_and_encodes_bodies_and_attachments() {
        let message = MailMessage::builder("invoice.ready", "Invoice ✓")
            .to(MailAddress::new("person@example.com").unwrap())
            .bcc(MailAddress::new("audit@example.com").unwrap())
            .text("Plain body")
            .html("<p>HTML body</p>")
            .attachment(
                MailAttachment::attachment("invoice.pdf", "application/pdf", b"PDF".to_vec())
                    .unwrap(),
            )
            .build()
            .unwrap();
        let rendered = String::from_utf8(
            render_mime(&message, &MailAddress::new("no-reply@example.com").unwrap()).unwrap(),
        )
        .unwrap();
        assert!(!rendered.contains("audit@example.com"));
        assert!(!rendered.contains("Plain body"));
        assert!(rendered.contains("multipart/mixed"));
        assert!(rendered.contains("application/pdf"));
        assert!(rendered.contains("=?UTF-8?B?"));

        let parsed = MessageParser::default()
            .parse(rendered.as_bytes())
            .expect("rendered MIME must be independently parseable");
        assert_eq!(parsed.subject(), Some("Invoice ✓"));
        assert_eq!(parsed.body_text(0).as_deref(), Some("Plain body"));
        assert_eq!(parsed.body_html(0).as_deref(), Some("<p>HTML body</p>"));
        assert!(parsed.attachment(0).is_some());
        assert!(parsed.bcc().is_none());
    }

    #[test]
    fn mime_folds_large_address_and_unstructured_headers_within_the_hard_limit() {
        let mut builder = MailMessage::builder(
            "invoice.ready",
            format!("Quarterly statement {}", "長".repeat(240)),
        )
        .text("Body")
        .header("X-Long-Audit-Token", "segment ".repeat(120));
        for index in 0..50 {
            let local = format!("recipient-{index:02}-{}", "x".repeat(48));
            let domain = format!("{}.{}.example", "a".repeat(63), "b".repeat(63));
            let address = MailAddress::named(
                format!("{local}@{domain}"),
                format!("Recipient {index:02} {}", "名".repeat(70)),
            )
            .unwrap();
            builder = builder.to(address);
        }
        for index in 0..10 {
            builder = builder.reply_to(
                MailAddress::named(
                    format!("reply-{index}@example.com"),
                    format!("Reply destination {index} {}", "係".repeat(50)),
                )
                .unwrap(),
            );
        }
        let rendered = render_mime(
            &builder.build().unwrap(),
            &MailAddress::named("no-reply@example.com", "送信者".repeat(25)).unwrap(),
        )
        .unwrap();
        let header_end = rendered
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("rendered header terminator");
        for line in rendered[..header_end].split(|byte| *byte == b'\n') {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            assert!(
                line.len() <= 998,
                "physical MIME header line is {} bytes",
                line.len()
            );
        }
    }

    #[test]
    fn near_attachment_boundary_stays_inside_the_rendered_provider_limit() {
        let message = MailMessage::builder("attachment.boundary", "Boundary")
            .to(MailAddress::new("person@example.com").unwrap())
            .text("Body")
            .attachment(
                MailAttachment::attachment(
                    "boundary.bin",
                    "application/octet-stream",
                    vec![0_u8; MAX_ATTACHMENT_BYTES],
                )
                .unwrap(),
            )
            .build()
            .unwrap();
        let rendered =
            render_mime(&message, &MailAddress::new("no-reply@example.com").unwrap()).unwrap();
        assert!(rendered.len() > MAX_ATTACHMENT_BYTES);
        assert!(rendered.len() <= MAX_RENDERED_MESSAGE_BYTES);
    }

    #[test]
    fn custom_headers_cannot_spoof_minco_or_ses_control_state() {
        for name in [
            "x-minco-message-id",
            "X-MiNcO-ToPiC",
            "X-SES-MESSAGE-TAGS",
            "x-ses-configuration-set",
            "X-SES-SOURCE-ARN",
        ] {
            let result = MailMessage::builder("topic", "Subject")
                .to(MailAddress::new("person@example.com").unwrap())
                .text("Body")
                .header(name, "spoof")
                .build();
            assert!(result.is_err(), "{name} must be reserved");
        }
        assert!(
            MailMessage::builder("topic", "Subject")
                .to(MailAddress::new("person@example.com").unwrap())
                .text("Body")
                .header("X-Application-Label", "not ASCII: ✓")
                .build()
                .is_err()
        );
    }

    #[derive(Debug)]
    struct ScriptedTransport {
        name: &'static str,
        outcomes: Mutex<VecDeque<Result<(), MailErrorKind>>>,
    }

    #[derive(Debug)]
    struct InvalidReceiptTransport;

    #[async_trait]
    impl MailTransport for InvalidReceiptTransport {
        fn name(&self) -> &'static str {
            "invalid-receipt"
        }

        async fn send(
            &self,
            message: &MailMessage,
            attempt: u32,
        ) -> Result<MailReceipt, MailError> {
            Ok(MailReceipt {
                message_id: message.id,
                transport: self.name().into(),
                provider_message_id: String::new(),
                accepted_at: Utc::now(),
                attempt,
            })
        }
    }

    #[derive(Debug)]
    struct SlowObserver;

    #[async_trait]
    impl MailObserver for SlowObserver {
        async fn observe(&self, _event: &MailSubmissionEvent) {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    #[async_trait]
    impl MailTransport for ScriptedTransport {
        fn name(&self) -> &str {
            self.name
        }

        async fn send(
            &self,
            message: &MailMessage,
            attempt: u32,
        ) -> Result<MailReceipt, MailError> {
            let outcome = {
                let mut outcomes = self.outcomes.lock().await;
                outcomes.pop_front().unwrap_or(Ok(()))
            };
            match outcome {
                Ok(()) => Ok(MailReceipt {
                    message_id: message.id,
                    transport: self.name.into(),
                    provider_message_id: format!("{}:{}", self.name, message.id),
                    accepted_at: Utc::now(),
                    attempt,
                }),
                Err(kind) => Err(MailError::new(kind, self.name, "scripted failure")),
            }
        }
    }

    #[tokio::test]
    async fn ambiguous_outcome_never_fails_over() {
        let primary = Arc::new(ScriptedTransport {
            name: "primary",
            outcomes: Mutex::new(VecDeque::from([Err(MailErrorKind::Ambiguous)])),
        });
        let fallback = Arc::new(MemoryMailTransport::named("fallback").unwrap());
        let service =
            MailService::new(vec![primary, fallback.clone()], Arc::new(NoopMailObserver)).unwrap();
        let error = service.send(message()).await.unwrap_err();
        assert!(error.is_ambiguous());
        assert_eq!(fallback.count().await, 0);
    }

    #[tokio::test]
    async fn explicit_unavailability_can_use_fallback() {
        let primary = Arc::new(ScriptedTransport {
            name: "primary",
            outcomes: Mutex::new(VecDeque::from([Err(MailErrorKind::Unavailable)])),
        });
        let fallback = Arc::new(MemoryMailTransport::named("fallback").unwrap());
        let observer = Arc::new(MemoryMailObserver::default());
        let service = MailService::new(vec![primary, fallback.clone()], observer.clone()).unwrap();
        let receipt = service.send(message()).await.unwrap();
        assert_eq!(receipt.transport, "fallback");
        assert_eq!(fallback.count().await, 1);
        assert_eq!(observer.events().await.len(), 5);
    }

    #[tokio::test]
    async fn invalid_acceptance_receipt_emits_ambiguous_failure_observation() {
        let observer = Arc::new(MemoryMailObserver::default());
        let service =
            MailService::single(Arc::new(InvalidReceiptTransport), observer.clone()).unwrap();
        let error = service.send(message()).await.unwrap_err();
        assert_eq!(error.kind, MailErrorKind::Ambiguous);
        let events = observer.events().await;
        assert_eq!(events.len(), 3);
        assert_eq!(events[2].kind, MailSubmissionEventKind::AttemptFailed);
        assert_eq!(events[2].failure_kind, Some(MailErrorKind::Ambiguous));
    }

    #[tokio::test]
    async fn slow_first_observer_does_not_suppress_later_observers() {
        let fast = Arc::new(MemoryMailObserver::default());
        let observer = Arc::new(
            CompositeMailObserver::new(vec![Arc::new(SlowObserver), fast.clone()]).unwrap(),
        );
        let service =
            MailService::single(Arc::new(MemoryMailTransport::default()), observer).unwrap();
        let started = Instant::now();
        service.send(message()).await.unwrap();
        assert_eq!(fast.events().await.len(), 3);
        assert!(started.elapsed() < Duration::from_millis(400));
    }

    #[tokio::test]
    async fn delivery_sink_deduplicates_provider_events() {
        let sink = MemoryMailDeliveryEventSink::default();
        let event = MailDeliveryEvent {
            source_event_id: "provider-event-1".into(),
            message_id: Uuid::now_v7(),
            topic: "invoice.ready".into(),
            transport: "aws.ses".into(),
            kind: MailDeliveryEventKind::Delivered,
            occurred_at: Utc::now(),
            provider_message_id: Some("provider-message".into()),
        };
        assert_eq!(
            sink.record(event.clone()).await.unwrap(),
            MailDeliveryDisposition::Recorded
        );
        assert_eq!(
            sink.record(event).await.unwrap(),
            MailDeliveryDisposition::Duplicate
        );
        assert_eq!(sink.events().await.len(), 1);
    }

    #[tokio::test]
    async fn tracing_delivery_sink_also_deduplicates_provider_events() {
        let sink = TracingMailDeliveryEventSink::default();
        let event = MailDeliveryEvent {
            source_event_id: "provider-event-1".into(),
            message_id: Uuid::now_v7(),
            topic: "invoice.ready".into(),
            transport: "aws.ses".into(),
            kind: MailDeliveryEventKind::Delivered,
            occurred_at: Utc::now(),
            provider_message_id: Some("provider-message".into()),
        };
        assert_eq!(
            sink.record(event.clone()).await.unwrap(),
            MailDeliveryDisposition::Recorded
        );
        assert_eq!(
            sink.record(event).await.unwrap(),
            MailDeliveryDisposition::Duplicate
        );
    }

    #[test]
    fn tracing_delivery_dedupe_window_evicts_the_oldest_id() {
        let mut window = DeliveryDedupeWindow::new(2);
        assert!(window.record_if_new("one"));
        assert!(window.record_if_new("two"));
        assert!(!window.record_if_new("one"));
        assert!(window.record_if_new("three"));
        assert!(window.record_if_new("one"));
    }
}
