use crate::{
    AttachmentUpload, AudioInput, CreateFeedbackInput, CreateFeedbackResult, DeveloperReplyInput,
    FeedbackAccessToken, FeedbackAiContext, FeedbackAttachment, FeedbackAttachmentKind, FeedbackId,
    FeedbackListFilter, FeedbackMessage, FeedbackMessageSource, FeedbackMutationResult,
    FeedbackReleaseBinding, FeedbackStatus, FeedbackStoreError, FeedbackStoreService,
    FeedbackSummary, FeedbackThread, FeedbackValidationError, FeedbackWarning, Transcript,
    TranscriptionError, TranscriptionService, TransitionFeedbackInput, hash_access_token,
};
use chrono::{TimeDelta, Utc};
use minco_plugin_audit::{AuditEvent, AuditService};
use minco_plugin_events::{DomainEvent, EventServices, OutboxRecord};
use minco_plugin_notifications::{Notification, NotificationChannel, NotificationService};
use minco_plugin_object_storage::{
    ObjectKey, ObjectStoreError, ObjectStoreService, PutObject, StoredObject,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
use uuid::Uuid;

pub const FEEDBACK_BASE_PATH: &str = "/_minco/feedback";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackWidgetPosition {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

impl FeedbackWidgetPosition {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TopLeft => "top_left",
            Self::TopRight => "top_right",
            Self::BottomLeft => "bottom_left",
            Self::BottomRight => "bottom_right",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackWidgetTheme {
    Light,
    Dark,
    #[default]
    Auto,
}

impl FeedbackWidgetTheme {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Auto => "auto",
        }
    }
}

/// Browser storage used for the opaque client conversation token.
///
/// `session` is the privacy-preserving default: the thread remains available
/// across navigation in the current tab but is not retained after the tab is
/// closed. `local` is an explicit opt-in for longer-lived review environments.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackTokenStorage {
    #[default]
    Session,
    Local,
}

impl FeedbackTokenStorage {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Local => "local",
        }
    }
}

/// Runtime configuration for a feedback deployment.
///
/// Browser-visible project keys are an abuse-control mechanism rather than an
/// authentication secret. Developer bearer tokens are intended only as a
/// local/operator fallback; production applications should inject a
/// `minco_http::Principal` with the `feedback.manage` permission.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
// These independent deployment controls intentionally map one-to-one to the
// public plugin configuration schema and are not mutually exclusive states.
#[allow(clippy::struct_excessive_bools)]
pub struct FeedbackConfig {
    pub project_id: String,
    #[serde(default = "default_widget_label")]
    pub widget_label: String,
    #[serde(default)]
    pub widget_position: FeedbackWidgetPosition,
    #[serde(default = "default_offset")]
    pub offset_x_px: u16,
    #[serde(default = "default_offset")]
    pub offset_y_px: u16,
    #[serde(default)]
    pub theme: FeedbackWidgetTheme,
    #[serde(default)]
    pub token_storage: FeedbackTokenStorage,
    #[serde(default = "default_http_body_limit")]
    pub max_http_body_bytes: usize,
    #[serde(default = "default_screenshot_limit")]
    pub max_screenshot_bytes: usize,
    #[serde(default = "default_audio_limit")]
    pub max_audio_bytes: usize,
    #[serde(default = "default_file_limit")]
    pub max_file_bytes: usize,
    #[serde(default = "default_max_attachments")]
    pub max_attachments: usize,
    #[serde(default = "default_recording_seconds")]
    pub max_recording_seconds: u32,
    #[serde(default)]
    pub project_key: Option<String>,
    #[serde(default)]
    pub allow_anonymous: bool,
    #[serde(default)]
    pub developer_token: Option<String>,
    #[serde(default = "default_developer_recipient")]
    pub developer_recipient: String,
    #[serde(default)]
    pub developer_link_base: Option<String>,
    #[serde(default = "default_true")]
    pub notify_client_updates: bool,
    /// Publish newly enqueued domain events on the request path. Disabled by default so feedback
    /// submission latency is not coupled to SQS, `EventBridge`, or another external broker.
    #[serde(default)]
    pub publish_events_inline: bool,
    #[serde(default)]
    pub transcription_enabled: bool,
    #[serde(default)]
    pub auto_transcribe_audio: bool,
    #[serde(default = "default_true")]
    pub screenshot_enabled: bool,
    #[serde(default)]
    pub voice_enabled: bool,
    /// Include URL query parameters in captured page context. Disabled by default because query
    /// strings commonly contain personal data or temporary credentials.
    #[serde(default)]
    pub include_url_query: bool,
    #[serde(default = "default_redacted_query_parameters")]
    pub redact_query_parameters: Vec<String>,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
    #[serde(default)]
    pub privacy_notice: Option<String>,
}

impl std::fmt::Debug for FeedbackConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FeedbackConfig")
            .field("project_id", &self.project_id)
            .field("widget_label", &self.widget_label)
            .field("widget_position", &self.widget_position)
            .field("offset_x_px", &self.offset_x_px)
            .field("offset_y_px", &self.offset_y_px)
            .field("theme", &self.theme)
            .field("token_storage", &self.token_storage)
            .field("max_http_body_bytes", &self.max_http_body_bytes)
            .field("max_screenshot_bytes", &self.max_screenshot_bytes)
            .field("max_audio_bytes", &self.max_audio_bytes)
            .field("max_file_bytes", &self.max_file_bytes)
            .field("max_attachments", &self.max_attachments)
            .field("max_recording_seconds", &self.max_recording_seconds)
            .field(
                "project_key",
                &self.project_key.as_ref().map(|_| "[REDACTED]"),
            )
            .field("allow_anonymous", &self.allow_anonymous)
            .field(
                "developer_token",
                &self.developer_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("developer_recipient", &self.developer_recipient)
            .field(
                "developer_link_base_configured",
                &self.developer_link_base.is_some(),
            )
            .field("notify_client_updates", &self.notify_client_updates)
            .field("publish_events_inline", &self.publish_events_inline)
            .field("transcription_enabled", &self.transcription_enabled)
            .field("auto_transcribe_audio", &self.auto_transcribe_audio)
            .field("screenshot_enabled", &self.screenshot_enabled)
            .field("voice_enabled", &self.voice_enabled)
            .field("include_url_query", &self.include_url_query)
            .field("redact_query_parameters", &self.redact_query_parameters)
            .field("poll_interval_ms", &self.poll_interval_ms)
            .field("privacy_notice", &self.privacy_notice)
            .finish()
    }
}

impl Default for FeedbackConfig {
    fn default() -> Self {
        Self {
            project_id: "default".into(),
            widget_label: default_widget_label(),
            widget_position: FeedbackWidgetPosition::BottomRight,
            offset_x_px: default_offset(),
            offset_y_px: default_offset(),
            theme: FeedbackWidgetTheme::Auto,
            token_storage: FeedbackTokenStorage::Session,
            max_http_body_bytes: default_http_body_limit(),
            max_screenshot_bytes: default_screenshot_limit(),
            max_audio_bytes: default_audio_limit(),
            max_file_bytes: default_file_limit(),
            max_attachments: default_max_attachments(),
            max_recording_seconds: default_recording_seconds(),
            project_key: None,
            allow_anonymous: false,
            developer_token: None,
            developer_recipient: default_developer_recipient(),
            developer_link_base: None,
            notify_client_updates: true,
            publish_events_inline: false,
            transcription_enabled: false,
            auto_transcribe_audio: false,
            screenshot_enabled: true,
            voice_enabled: false,
            include_url_query: false,
            redact_query_parameters: default_redacted_query_parameters(),
            poll_interval_ms: default_poll_interval(),
            privacy_notice: None,
        }
    }
}

impl FeedbackConfig {
    pub fn validate(&self) -> Result<(), FeedbackServiceError> {
        validate_config_text("project_id", &self.project_id, 100)?;
        validate_config_text("widget_label", &self.widget_label, 80)?;
        validate_config_text("developer_recipient", &self.developer_recipient, 200)?;
        validate_optional_config_text("project_key", self.project_key.as_deref(), 500)?;
        validate_optional_config_text(
            "developer_link_base",
            self.developer_link_base.as_deref(),
            2_000,
        )?;
        validate_optional_config_text("privacy_notice", self.privacy_notice.as_deref(), 2_000)?;
        if let Some(token) = self.developer_token.as_deref()
            && token
                .chars()
                .filter(|character| !character.is_whitespace())
                .count()
                < 24
        {
            return Err(FeedbackServiceError::Configuration(
                "developer_token must contain at least 24 non-whitespace characters".into(),
            ));
        }
        if self.max_http_body_bytes < 256 * 1024 || self.max_http_body_bytes > 8 * 1024 * 1024 {
            return Err(FeedbackServiceError::Configuration(
                "max_http_body_bytes must be between 256 KiB and 8 MiB for the default serverless HTTP profile".into(),
            ));
        }
        if self.max_screenshot_bytes == 0
            || self.max_audio_bytes == 0
            || self.max_file_bytes == 0
            || self.max_screenshot_bytes > self.max_http_body_bytes
            || self.max_audio_bytes > self.max_http_body_bytes
            || self.max_file_bytes > self.max_http_body_bytes
        {
            return Err(FeedbackServiceError::Configuration(
                "feedback attachment limits must be greater than zero and no larger than max_http_body_bytes".into(),
            ));
        }
        if self.max_attachments > 8 {
            return Err(FeedbackServiceError::Configuration(
                "max_attachments must be between 0 and 8".into(),
            ));
        }
        if !(5..=300).contains(&self.max_recording_seconds) {
            return Err(FeedbackServiceError::Configuration(
                "max_recording_seconds must be between 5 and 300".into(),
            ));
        }
        if !(1_000..=300_000).contains(&self.poll_interval_ms) {
            return Err(FeedbackServiceError::Configuration(
                "poll_interval_ms must be between 1000 and 300000".into(),
            ));
        }
        if self.auto_transcribe_audio && !self.transcription_enabled {
            return Err(FeedbackServiceError::Configuration(
                "auto_transcribe_audio requires transcription_enabled".into(),
            ));
        }
        if self.transcription_enabled && (self.allow_anonymous || self.project_key.is_some()) {
            return Err(FeedbackServiceError::Configuration(
                "transcription_enabled requires authenticated feedback.create submissions; it cannot be combined with allow_anonymous or project_key".into(),
            ));
        }
        if self.redact_query_parameters.iter().any(|value| {
            value.trim().is_empty() || value.len() > 100 || value.chars().any(char::is_control)
        }) {
            return Err(FeedbackServiceError::Configuration(
                "redact_query_parameters entries must contain 1-100 visible characters".into(),
            ));
        }
        Ok(())
    }

    #[must_use]
    pub fn widget_config(&self) -> FeedbackWidgetConfig {
        FeedbackWidgetConfig {
            enabled: true,
            project_id: self.project_id.clone(),
            label: self.widget_label.clone(),
            position: self.widget_position.as_str().replace('_', "-"),
            offset_x_px: self.offset_x_px,
            offset_y_px: self.offset_y_px,
            theme: self.theme.as_str().into(),
            token_storage: self.token_storage.as_str().into(),
            screenshot_enabled: self.screenshot_enabled,
            voice_enabled: self.voice_enabled,
            transcription_enabled: self.transcription_enabled,
            max_http_body_bytes: self.max_http_body_bytes,
            max_screenshot_bytes: self.max_screenshot_bytes,
            max_audio_bytes: self.max_audio_bytes,
            max_file_bytes: self.max_file_bytes,
            max_attachments: self.max_attachments,
            max_recording_seconds: self.max_recording_seconds,
            include_url_query: self.include_url_query,
            redact_query_parameters: self.redact_query_parameters.clone(),
            poll_interval_ms: self.poll_interval_ms,
            privacy_notice: self.privacy_notice.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// This is the browser-safe projection of independent widget capabilities.
#[allow(clippy::struct_excessive_bools)]
pub struct FeedbackWidgetConfig {
    pub enabled: bool,
    pub project_id: String,
    pub label: String,
    pub position: String,
    pub offset_x_px: u16,
    pub offset_y_px: u16,
    pub theme: String,
    pub token_storage: String,
    pub screenshot_enabled: bool,
    pub voice_enabled: bool,
    pub transcription_enabled: bool,
    pub max_http_body_bytes: usize,
    pub max_screenshot_bytes: usize,
    pub max_audio_bytes: usize,
    pub max_file_bytes: usize,
    pub max_attachments: usize,
    pub max_recording_seconds: u32,
    pub include_url_query: bool,
    pub redact_query_parameters: Vec<String>,
    pub poll_interval_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privacy_notice: Option<String>,
}

const fn default_offset() -> u16 {
    24
}

const fn default_http_body_limit() -> usize {
    7 * 1024 * 1024
}

const fn default_screenshot_limit() -> usize {
    4 * 1024 * 1024
}

const fn default_audio_limit() -> usize {
    5 * 1024 * 1024
}

const fn default_file_limit() -> usize {
    5 * 1024 * 1024
}

const fn default_max_attachments() -> usize {
    3
}

const fn default_recording_seconds() -> u32 {
    90
}

fn default_redacted_query_parameters() -> Vec<String> {
    [
        "access_token",
        "api_key",
        "code",
        "key",
        "password",
        "secret",
        "signature",
        "token",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

const fn default_poll_interval() -> u64 {
    15_000
}

fn default_widget_label() -> String {
    "Share feedback".into()
}

fn default_developer_recipient() -> String {
    "developers".into()
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotificationAudience {
    None,
    Developer,
    Client,
}

#[derive(Clone)]
pub struct FeedbackService {
    store: FeedbackStoreService,
    objects: ObjectStoreService,
    notifications: NotificationService,
    audit: AuditService,
    events: EventServices,
    transcription: Option<TranscriptionService>,
    config: Arc<FeedbackConfig>,
    release_binding: Option<Arc<FeedbackReleaseBinding>>,
}

impl std::fmt::Debug for FeedbackService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FeedbackService")
            .field("config", &self.config)
            .field("transcription_configured", &self.transcription.is_some())
            .field("release_bound", &self.release_binding.is_some())
            .finish_non_exhaustive()
    }
}

impl FeedbackService {
    pub fn new(
        store: FeedbackStoreService,
        objects: ObjectStoreService,
        notifications: NotificationService,
        audit: AuditService,
        events: EventServices,
        transcription: Option<TranscriptionService>,
        config: FeedbackConfig,
    ) -> Result<Self, FeedbackServiceError> {
        config.validate()?;
        if config.transcription_enabled && transcription.is_none() {
            return Err(FeedbackServiceError::Configuration(
                "transcription_enabled requires a TranscriptionService".into(),
            ));
        }
        Ok(Self {
            store,
            objects,
            notifications,
            audit,
            events,
            transcription,
            config: Arc::new(config),
            release_binding: None,
        })
    }

    pub fn with_release_binding(
        mut self,
        binding: FeedbackReleaseBinding,
    ) -> Result<Self, FeedbackServiceError> {
        binding.validate()?;
        self.release_binding = Some(Arc::new(binding));
        Ok(self)
    }

    #[must_use]
    pub fn release_binding(&self) -> Option<&FeedbackReleaseBinding> {
        self.release_binding.as_deref()
    }

    #[must_use]
    pub fn config(&self) -> &FeedbackConfig {
        &self.config
    }

    pub async fn ready(&self) -> Result<(), FeedbackServiceError> {
        self.store.ready().await?;
        Ok(())
    }

    pub async fn create(
        &self,
        mut input: CreateFeedbackInput,
        uploads: Vec<AttachmentUpload>,
        correlation_id: Uuid,
    ) -> Result<CreateFeedbackResult, FeedbackServiceError> {
        if input.project_id.trim().is_empty() {
            input.project_id.clone_from(&self.config.project_id);
        }
        if input.project_id != self.config.project_id {
            return Err(FeedbackValidationError::InvalidField {
                field: "project_id",
                detail: "does not match the configured feedback project".into(),
            }
            .into());
        }

        if let Some(binding) = self.release_binding.as_deref() {
            input.context.release_id = Some(binding.release_id.clone());
            input.context.environment = Some(binding.environment.clone());
        }

        if uploads.len() > self.config.max_attachments {
            return Err(FeedbackServiceError::InvalidAttachment(format!(
                "feedback contains {} attachments; configured maximum is {}",
                uploads.len(),
                self.config.max_attachments
            )));
        }
        let aggregate_bytes = uploads.iter().try_fold(0_usize, |total, upload| {
            total.checked_add(upload.bytes.len()).ok_or_else(|| {
                FeedbackServiceError::InvalidAttachment(
                    "aggregate attachment size exceeds the platform address space".into(),
                )
            })
        })?;
        if aggregate_bytes > self.config.max_http_body_bytes {
            return Err(FeedbackServiceError::InvalidAttachment(format!(
                "aggregate attachment payload is {aggregate_bytes} bytes; configured HTTP body ceiling is {} bytes",
                self.config.max_http_body_bytes
            )));
        }

        let mut thread = FeedbackThread::create(input)?;
        if let Some(binding) = self.release_binding.as_deref() {
            thread.messages.push(binding.system_message()?);
        }
        let client_token = FeedbackAccessToken::generate();
        let mut stored_keys = Vec::new();
        let mut warnings = Vec::new();

        for upload in uploads {
            let (attachment, attachment_warnings) =
                match self.store_attachment(thread.id, upload).await {
                    Ok(value) => value,
                    Err(error) => {
                        self.cleanup_objects(&stored_keys).await;
                        return Err(error);
                    }
                };
            stored_keys.push(ObjectKey::parse(attachment.object_key.clone())?);
            if let Some(transcript) = attachment.transcript.clone() {
                let message = match FeedbackMessage::new(
                    crate::FeedbackAuthorRole::Client,
                    None,
                    transcript,
                    FeedbackMessageSource::VoiceTranscript,
                    true,
                ) {
                    Ok(message) => message,
                    Err(error) => {
                        self.cleanup_objects(&stored_keys).await;
                        return Err(error.into());
                    }
                };
                thread.append_message(message);
            }
            thread.add_attachment(attachment);
            warnings.extend(attachment_warnings);
        }

        if let Err(error) = self
            .store
            .create(thread.clone(), hash_access_token(&client_token))
            .await
        {
            self.cleanup_objects(&stored_keys).await;
            return Err(error.into());
        }

        warnings.extend(self.record_created(&thread, correlation_id).await);
        Ok(CreateFeedbackResult {
            thread,
            client_token,
            warnings,
        })
    }

    pub async fn get_for_client(
        &self,
        id: FeedbackId,
        token: &FeedbackAccessToken,
    ) -> Result<FeedbackThread, FeedbackServiceError> {
        Ok(self.client_thread(id, token).await?.client_view())
    }

    pub async fn get_for_developer(
        &self,
        id: FeedbackId,
    ) -> Result<FeedbackThread, FeedbackServiceError> {
        let thread = self
            .store
            .get(id)
            .await?
            .ok_or(FeedbackServiceError::NotFound(id))?;
        if thread.project_id != self.config.project_id {
            return Err(FeedbackServiceError::NotFound(id));
        }
        Ok(thread)
    }

    pub async fn list(
        &self,
        mut filter: FeedbackListFilter,
    ) -> Result<Vec<FeedbackSummary>, FeedbackServiceError> {
        if filter
            .project_id
            .as_deref()
            .is_some_and(|project_id| project_id != self.config.project_id)
        {
            return Err(FeedbackValidationError::InvalidField {
                field: "project_id",
                detail: "does not match the configured feedback project".into(),
            }
            .into());
        }
        filter.project_id = Some(self.config.project_id.clone());
        Ok(self.store.list(filter).await?)
    }

    pub async fn reply_as_client(
        &self,
        id: FeedbackId,
        token: &FeedbackAccessToken,
        body: impl Into<String>,
        correlation_id: Uuid,
    ) -> Result<FeedbackMutationResult, FeedbackServiceError> {
        let mut thread = self.client_thread(id, token).await?;
        let expected_revision = thread.revision;
        thread.append_message(FeedbackMessage::client(body)?);
        if thread.status == FeedbackStatus::NeedsClarification {
            thread.transition(FeedbackStatus::Acknowledged, None)?;
        }
        self.store.save(thread.clone(), expected_revision).await?;
        let warnings = self
            .record_mutation(
                &thread,
                "feedback.client_replied",
                "Client replied to feedback",
                NotificationAudience::Developer,
                thread.context.client_subject.clone(),
                correlation_id,
            )
            .await;
        Ok(FeedbackMutationResult {
            thread: thread.client_view(),
            warnings,
        })
    }

    pub async fn reply_as_developer(
        &self,
        id: FeedbackId,
        input: DeveloperReplyInput,
        actor_subject: String,
        correlation_id: Uuid,
    ) -> Result<FeedbackMutationResult, FeedbackServiceError> {
        let mut thread = self.get_for_developer(id).await?;
        let expected_revision = thread.revision;
        thread.append_message(FeedbackMessage::developer(
            input.author_display,
            input.body,
            input.visible_to_client,
        )?);
        if thread.status == FeedbackStatus::New {
            thread.transition(FeedbackStatus::Acknowledged, None)?;
        }
        self.store.save(thread.clone(), expected_revision).await?;
        let warnings = self
            .record_mutation(
                &thread,
                "feedback.developer_replied",
                "Developer replied to feedback",
                if input.visible_to_client && self.config.notify_client_updates {
                    NotificationAudience::Client
                } else {
                    NotificationAudience::None
                },
                Some(actor_subject),
                correlation_id,
            )
            .await;
        Ok(FeedbackMutationResult { thread, warnings })
    }

    pub async fn transition(
        &self,
        id: FeedbackId,
        input: TransitionFeedbackInput,
        actor_subject: String,
        correlation_id: Uuid,
    ) -> Result<FeedbackMutationResult, FeedbackServiceError> {
        let mut thread = self.get_for_developer(id).await?;
        let expected_revision = thread.revision;
        let previous = thread.status;
        thread.transition(input.status, input.resolution)?;
        thread.append_message(FeedbackMessage::new(
            crate::FeedbackAuthorRole::System,
            input.author_display,
            format!("Status changed from {previous} to {}", input.status),
            FeedbackMessageSource::StatusChange,
            true,
        )?);
        self.store.save(thread.clone(), expected_revision).await?;
        let warnings = self
            .record_mutation(
                &thread,
                "feedback.status_changed",
                &format!("Feedback status changed to {}", input.status),
                if self.config.notify_client_updates {
                    NotificationAudience::Client
                } else {
                    NotificationAudience::None
                },
                Some(actor_subject),
                correlation_id,
            )
            .await;
        Ok(FeedbackMutationResult { thread, warnings })
    }

    pub async fn transcribe(&self, audio: AudioInput) -> Result<Transcript, FeedbackServiceError> {
        if !self.config.voice_enabled {
            return Err(FeedbackServiceError::Configuration(
                "voice feedback is disabled for this deployment".into(),
            ));
        }
        if !self.config.transcription_enabled {
            return Err(TranscriptionError::NotConfigured.into());
        }
        self.validate_upload_size(FeedbackAttachmentKind::Audio, audio.bytes.len())?;
        let service = self
            .transcription
            .as_ref()
            .ok_or(TranscriptionError::NotConfigured)?;
        Ok(service.transcribe(audio).await?)
    }

    pub async fn ai_context(
        &self,
        id: FeedbackId,
    ) -> Result<FeedbackAiContext, FeedbackServiceError> {
        Ok(FeedbackAiContext::from_thread(
            self.get_for_developer(id).await?,
        ))
    }

    pub async fn attachment_for_developer(
        &self,
        id: FeedbackId,
        attachment_id: Uuid,
    ) -> Result<StoredObject, FeedbackServiceError> {
        let thread = self.get_for_developer(id).await?;
        self.load_attachment(&thread, attachment_id).await
    }

    pub async fn attachment_for_client(
        &self,
        id: FeedbackId,
        token: &FeedbackAccessToken,
        attachment_id: Uuid,
    ) -> Result<StoredObject, FeedbackServiceError> {
        let thread = self.client_thread(id, token).await?;
        self.load_attachment(&thread, attachment_id).await
    }

    async fn client_thread(
        &self,
        id: FeedbackId,
        token: &FeedbackAccessToken,
    ) -> Result<FeedbackThread, FeedbackServiceError> {
        let thread = self
            .store
            .get_for_client(id, &hash_access_token(token))
            .await?
            .ok_or(FeedbackServiceError::ClientAccessDenied)?;
        if thread.project_id != self.config.project_id {
            return Err(FeedbackServiceError::ClientAccessDenied);
        }
        Ok(thread)
    }

    async fn load_attachment(
        &self,
        thread: &FeedbackThread,
        attachment_id: Uuid,
    ) -> Result<StoredObject, FeedbackServiceError> {
        let attachment = thread
            .attachments
            .iter()
            .find(|attachment| attachment.id == attachment_id)
            .ok_or(FeedbackServiceError::AttachmentNotFound(attachment_id))?;
        let key = ObjectKey::parse(attachment.object_key.clone())?;
        self.objects
            .get(&key)
            .await?
            .ok_or(FeedbackServiceError::AttachmentNotFound(attachment_id))
    }

    async fn store_attachment(
        &self,
        feedback_id: FeedbackId,
        upload: AttachmentUpload,
    ) -> Result<(FeedbackAttachment, Vec<FeedbackWarning>), FeedbackServiceError> {
        self.validate_attachment(&upload)?;
        let safe_name = safe_file_name(&upload.file_name);
        let attachment_id = Uuid::now_v7();
        let object_key = ObjectKey::parse(format!(
            "feedback/{feedback_id}/{attachment_id}/{safe_name}"
        ))?;
        let mut attributes = BTreeMap::from([
            ("feedback_id".into(), feedback_id.to_string()),
            ("attachment_id".into(), attachment_id.to_string()),
            ("file_name".into(), safe_name.clone()),
            (
                "kind".into(),
                format!("{:?}", upload.kind).to_ascii_lowercase(),
            ),
        ]);
        attributes.insert("project_id".into(), self.config.project_id.clone());
        let metadata = self
            .objects
            .put(PutObject {
                key: object_key.clone(),
                bytes: upload.bytes.clone(),
                content_type: upload.content_type.clone(),
                attributes,
            })
            .await?;
        let mut warnings = Vec::new();
        let transcript = if upload.kind == FeedbackAttachmentKind::Audio
            && self.config.transcription_enabled
            && self.config.auto_transcribe_audio
        {
            match self
                .transcription
                .as_ref()
                .ok_or(TranscriptionError::NotConfigured)?
                .transcribe(AudioInput {
                    bytes: upload.bytes,
                    file_name: safe_name.clone(),
                    content_type: upload.content_type.clone(),
                    language: None,
                    prompt: Some("Transcribe client product feedback accurately.".into()),
                })
                .await
            {
                Ok(transcript) => Some(transcript.text),
                Err(error) => {
                    warnings.push(downstream_warning(
                        "feedback_transcription_failed",
                        "Audio was stored, but automatic transcription did not complete.",
                        &error,
                    ));
                    None
                }
            }
        } else {
            None
        };
        Ok((
            FeedbackAttachment {
                id: attachment_id,
                kind: upload.kind,
                object_key: object_key.as_str().to_owned(),
                file_name: safe_name,
                content_type: upload.content_type,
                size_bytes: metadata.size_bytes,
                sha256: metadata.sha256,
                created_at: metadata.created_at,
                transcript,
            },
            warnings,
        ))
    }

    fn validate_attachment(&self, upload: &AttachmentUpload) -> Result<(), FeedbackServiceError> {
        match upload.kind {
            FeedbackAttachmentKind::Screenshot if !self.config.screenshot_enabled => {
                return Err(FeedbackServiceError::InvalidAttachment(
                    "screenshot feedback is disabled for this deployment".into(),
                ));
            }
            FeedbackAttachmentKind::Audio if !self.config.voice_enabled => {
                return Err(FeedbackServiceError::InvalidAttachment(
                    "voice feedback is disabled for this deployment".into(),
                ));
            }
            _ => {}
        }
        self.validate_upload_size(upload.kind, upload.bytes.len())?;
        if upload.file_name.trim().is_empty() || upload.file_name.chars().any(char::is_control) {
            return Err(FeedbackServiceError::InvalidAttachment(
                "attachment file name is invalid".into(),
            ));
        }
        let content_type = upload.content_type.trim().to_ascii_lowercase();
        if content_type.is_empty()
            || (upload.kind == FeedbackAttachmentKind::Screenshot
                && !content_type.starts_with("image/"))
            || (upload.kind == FeedbackAttachmentKind::Audio && !content_type.starts_with("audio/"))
        {
            return Err(FeedbackServiceError::InvalidAttachment(format!(
                "content type {:?} is not valid for {:?}",
                upload.content_type, upload.kind
            )));
        }
        Ok(())
    }

    async fn cleanup_objects(&self, keys: &[ObjectKey]) {
        for key in keys {
            if let Err(error) = self.objects.delete(key).await {
                tracing::warn!(
                    object_key = key.as_str(),
                    %error,
                    "failed to clean up feedback attachment after aborted submission"
                );
            }
        }
    }

    fn validate_upload_size(
        &self,
        kind: FeedbackAttachmentKind,
        actual: usize,
    ) -> Result<(), FeedbackServiceError> {
        let maximum = match kind {
            FeedbackAttachmentKind::Screenshot => self.config.max_screenshot_bytes,
            FeedbackAttachmentKind::Audio => self.config.max_audio_bytes,
            FeedbackAttachmentKind::File => self.config.max_file_bytes,
        };
        if actual == 0 {
            return Err(FeedbackServiceError::InvalidAttachment(
                "attachment must not be empty".into(),
            ));
        }
        if actual > maximum {
            return Err(FeedbackServiceError::AttachmentTooLarge {
                kind,
                actual,
                maximum,
            });
        }
        Ok(())
    }

    async fn record_created(
        &self,
        thread: &FeedbackThread,
        correlation_id: Uuid,
    ) -> Vec<FeedbackWarning> {
        self.record_mutation(
            thread,
            "feedback.created",
            "New client feedback",
            NotificationAudience::Developer,
            thread.context.client_subject.clone(),
            correlation_id,
        )
        .await
    }

    async fn record_mutation(
        &self,
        thread: &FeedbackThread,
        event_type: &str,
        title: &str,
        audience: NotificationAudience,
        actor_subject: Option<String>,
        correlation_id: Uuid,
    ) -> Vec<FeedbackWarning> {
        let mut warnings = Vec::new();

        let mut audit = AuditEvent::new(
            event_type,
            "feedback",
            thread.id.to_string(),
            correlation_id,
        );
        audit.actor_subject = actor_subject;
        audit.metadata.insert(
            "project_id".into(),
            serde_json::Value::String(thread.project_id.clone()),
        );
        audit.metadata.insert(
            "status".into(),
            serde_json::Value::String(thread.status.to_string()),
        );
        if let Err(error) = self.audit.append(audit).await {
            warnings.push(downstream_warning(
                "feedback_audit_failed",
                "Feedback was saved, but audit recording did not complete.",
                &error,
            ));
        }

        warnings.extend(self.record_event(thread, event_type, correlation_id).await);

        if let Some(notification) = self.notification(thread, event_type, title, audience)
            && let Err(error) = self.notifications.send(notification).await
        {
            warnings.push(downstream_warning(
                "feedback_notification_failed",
                "Feedback was saved, but notification delivery did not complete.",
                &error,
            ));
        }

        warnings
    }

    async fn record_event(
        &self,
        thread: &FeedbackThread,
        event_type: &str,
        correlation_id: Uuid,
    ) -> Vec<FeedbackWarning> {
        let payload = match serde_json::to_value(thread) {
            Ok(value) => value,
            Err(error) => {
                return vec![downstream_warning(
                    "feedback_event_serialization_failed",
                    "Feedback was saved, but event preparation did not complete.",
                    &error,
                )];
            }
        };
        let event = DomainEvent::new(
            event_type,
            "feedback",
            thread.id.to_string(),
            correlation_id,
            payload,
        );
        let record = OutboxRecord::pending(event.clone());
        if let Err(error) = self.events.outbox.enqueue(record).await {
            return vec![downstream_warning(
                "feedback_event_enqueue_failed",
                "Feedback was saved, but event queuing did not complete.",
                &error,
            )];
        }
        if !self.config.publish_events_inline {
            return Vec::new();
        }
        let worker_id = format!("feedback-request-{correlation_id}");
        let claimed = match self
            .events
            .outbox
            .claim_event(event.id, &worker_id, Utc::now() + TimeDelta::minutes(1))
            .await
        {
            Ok(Some(record)) => record,
            Ok(None) => {
                return vec![FeedbackWarning::new(
                    "feedback_event_claim_unavailable",
                    "the feedback event was durably queued for later publication",
                )];
            }
            Err(error) => {
                return vec![downstream_warning(
                    "feedback_event_claim_failed",
                    "The feedback event was queued, but immediate publication did not start.",
                    &error,
                )];
            }
        };
        match self.events.publisher.publish(&claimed.event).await {
            Ok(()) => {
                if let Err(error) = self
                    .events
                    .outbox
                    .mark_published(event.id, &worker_id)
                    .await
                {
                    vec![downstream_warning(
                        "feedback_event_mark_published_failed",
                        "The feedback event was published, but its delivery state was not finalized.",
                        &error,
                    )]
                } else {
                    Vec::new()
                }
            }
            Err(error) => {
                let detail = error.to_string();
                let _ = self
                    .events
                    .outbox
                    .mark_failed(
                        event.id,
                        &worker_id,
                        detail.clone(),
                        Utc::now() + TimeDelta::seconds(30),
                    )
                    .await;
                vec![downstream_warning(
                    "feedback_event_publish_failed",
                    "The feedback event was queued, but immediate publication did not complete.",
                    &error,
                )]
            }
        }
    }

    fn notification(
        &self,
        thread: &FeedbackThread,
        event_type: &str,
        title: &str,
        audience: NotificationAudience,
    ) -> Option<Notification> {
        let (channel, recipient) = match audience {
            NotificationAudience::None => return None,
            NotificationAudience::Developer => (
                NotificationChannel::DeveloperInbox,
                self.config.developer_recipient.clone(),
            ),
            NotificationAudience::Client => (
                NotificationChannel::InApp,
                thread.context.client_subject.clone()?,
            ),
        };
        let mut notification = Notification::new(
            event_type,
            channel,
            recipient,
            title,
            format!(
                "{} [{}] — {}",
                thread.title, thread.status, thread.context.page_url
            ),
        );
        notification.link = self
            .config
            .developer_link_base
            .as_ref()
            .map(|base| format!("{}/{}", base.trim_end_matches('/'), thread.id));
        notification.metadata.insert(
            "feedback_id".into(),
            serde_json::Value::String(thread.id.to_string()),
        );
        notification.metadata.insert(
            "project_id".into(),
            serde_json::Value::String(thread.project_id.clone()),
        );
        Some(notification)
    }
}

fn safe_file_name(value: &str) -> String {
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
    if safe.trim_matches('-').is_empty() {
        "attachment.bin".into()
    } else {
        safe
    }
}

fn downstream_warning(
    code: &'static str,
    public_detail: &'static str,
    error: &impl std::fmt::Display,
) -> FeedbackWarning {
    tracing::warn!(warning_code = code, %error, "feedback downstream action failed");
    FeedbackWarning::new(code, public_detail)
}

fn validate_config_text(
    field: &'static str,
    value: &str,
    maximum: usize,
) -> Result<(), FeedbackServiceError> {
    if value.trim().is_empty()
        || value.chars().count() > maximum
        || value.chars().any(char::is_control)
    {
        return Err(FeedbackServiceError::Configuration(format!(
            "{field} must contain 1-{maximum} visible characters"
        )));
    }
    Ok(())
}

fn validate_optional_config_text(
    field: &'static str,
    value: Option<&str>,
    maximum: usize,
) -> Result<(), FeedbackServiceError> {
    if let Some(value) = value {
        validate_config_text(field, value, maximum)?;
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum FeedbackServiceError {
    #[error(transparent)]
    Validation(#[from] FeedbackValidationError),
    #[error("feedback was not found: {0}")]
    NotFound(FeedbackId),
    #[error("feedback client access was denied")]
    ClientAccessDenied,
    #[error("feedback attachment was not found: {0}")]
    AttachmentNotFound(Uuid),
    #[error("invalid feedback attachment: {0}")]
    InvalidAttachment(String),
    #[error("feedback attachment {kind:?} is {actual} bytes; limit is {maximum} bytes")]
    AttachmentTooLarge {
        kind: FeedbackAttachmentKind,
        actual: usize,
        maximum: usize,
    },
    #[error("invalid feedback configuration: {0}")]
    Configuration(String),
    #[error(transparent)]
    Store(#[from] FeedbackStoreError),
    #[error(transparent)]
    ObjectStore(#[from] ObjectStoreError),
    #[error(transparent)]
    Transcription(#[from] TranscriptionError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FeedbackContext, FeedbackKind, FeedbackPriority, FeedbackStore, MemoryFeedbackStore,
    };
    use async_trait::async_trait;
    use minco_plugin_audit::MemoryAuditSink;
    use minco_plugin_events::MemoryEventBus;
    use minco_plugin_notifications::MemoryNotificationSink;
    use minco_plugin_object_storage::MemoryObjectStore;
    use std::collections::BTreeSet;

    struct Harness {
        service: FeedbackService,
        notifications: Arc<MemoryNotificationSink>,
        audit: Arc<MemoryAuditSink>,
        events: Arc<MemoryEventBus>,
        objects: Arc<MemoryObjectStore>,
    }

    fn harness() -> Harness {
        harness_with_config(FeedbackConfig {
            project_id: "example".into(),
            publish_events_inline: true,
            ..FeedbackConfig::default()
        })
    }

    fn harness_with_config(config: FeedbackConfig) -> Harness {
        harness_with_store(
            FeedbackStoreService::new(Arc::new(MemoryFeedbackStore::default())),
            config,
        )
    }

    fn harness_with_store(store: FeedbackStoreService, config: FeedbackConfig) -> Harness {
        let notifications = Arc::new(MemoryNotificationSink::default());
        let audit = Arc::new(MemoryAuditSink::default());
        let events = Arc::new(MemoryEventBus::default());
        let objects = Arc::new(MemoryObjectStore::default());
        let service = FeedbackService::new(
            store,
            ObjectStoreService::new(objects.clone()),
            NotificationService::new(notifications.clone()),
            AuditService::new(audit.clone()),
            EventServices {
                publisher: events.clone(),
                outbox: events.clone(),
            },
            None,
            config,
        )
        .unwrap();
        Harness {
            service,
            notifications,
            audit,
            events,
            objects,
        }
    }

    #[derive(Debug)]
    struct RejectingFeedbackStore;

    #[async_trait]
    impl FeedbackStore for RejectingFeedbackStore {
        async fn create(
            &self,
            _thread: FeedbackThread,
            _client_token_hash: String,
        ) -> Result<(), FeedbackStoreError> {
            Err(FeedbackStoreError::Infrastructure(
                "injected persistence failure".into(),
            ))
        }

        async fn get(&self, _id: FeedbackId) -> Result<Option<FeedbackThread>, FeedbackStoreError> {
            Err(FeedbackStoreError::Infrastructure(
                "injected persistence failure".into(),
            ))
        }

        async fn get_for_client(
            &self,
            _id: FeedbackId,
            _client_token_hash: &str,
        ) -> Result<Option<FeedbackThread>, FeedbackStoreError> {
            Err(FeedbackStoreError::Infrastructure(
                "injected persistence failure".into(),
            ))
        }

        async fn list(
            &self,
            _filter: FeedbackListFilter,
        ) -> Result<Vec<FeedbackSummary>, FeedbackStoreError> {
            Err(FeedbackStoreError::Infrastructure(
                "injected persistence failure".into(),
            ))
        }

        async fn save(
            &self,
            _thread: FeedbackThread,
            _expected_revision: u64,
        ) -> Result<(), FeedbackStoreError> {
            Err(FeedbackStoreError::Infrastructure(
                "injected persistence failure".into(),
            ))
        }
    }

    #[test]
    fn browser_tokens_are_tab_scoped_by_default() {
        let config = FeedbackConfig::default();
        assert_eq!(config.token_storage, FeedbackTokenStorage::Session);
        assert_eq!(config.widget_config().token_storage, "session");
        assert!(!config.allow_anonymous);

        let deserialized: FeedbackConfig =
            serde_json::from_value(serde_json::json!({"project_id": "example"})).unwrap();
        assert!(!deserialized.allow_anonymous);
    }

    #[test]
    fn text_only_profile_can_disable_all_attachments() {
        let config = FeedbackConfig {
            project_id: "text-only".into(),
            max_attachments: 0,
            screenshot_enabled: false,
            voice_enabled: false,
            ..FeedbackConfig::default()
        };

        config.validate().expect("zero-attachment profile");
        assert_eq!(config.widget_config().max_attachments, 0);
    }

    #[test]
    fn public_submission_modes_cannot_enable_transcription() {
        for config in [
            FeedbackConfig {
                project_id: "example".into(),
                transcription_enabled: true,
                allow_anonymous: true,
                ..FeedbackConfig::default()
            },
            FeedbackConfig {
                project_id: "example".into(),
                transcription_enabled: true,
                project_key: Some("browser-visible-key".into()),
                ..FeedbackConfig::default()
            },
        ] {
            assert!(matches!(
                config.validate(),
                Err(FeedbackServiceError::Configuration(_))
            ));
        }
    }

    #[test]
    fn downstream_warning_details_do_not_expose_provider_diagnostics() {
        let warning = downstream_warning(
            "feedback_transcription_failed",
            "Audio transcription did not complete.",
            &TranscriptionError::Provider(
                "provider response containing sensitive diagnostics".into(),
            ),
        );

        assert_eq!(warning.detail, "Audio transcription did not complete.");
        assert!(!warning.detail.contains("sensitive diagnostics"));
    }

    fn input() -> CreateFeedbackInput {
        CreateFeedbackInput {
            project_id: "example".into(),
            kind: FeedbackKind::Bug,
            priority: FeedbackPriority::High,
            title: "Save does not work".into(),
            description: "The save button leaves the form open.".into(),
            context: FeedbackContext {
                page_url: "https://example.test/orders/one".into(),
                route_name: Some("order-edit".into()),
                release_id: Some("release-1".into()),
                environment: Some("review".into()),
                request_id: Some("request-1".into()),
                user_agent: None,
                viewport: None,
                client_subject: Some("client-1".into()),
            },
            tags: BTreeSet::new(),
        }
    }

    fn release_binding() -> FeedbackReleaseBinding {
        let release_digest = "a".repeat(64);
        FeedbackReleaseBinding {
            release_id: format!("minco.{}", &release_digest[..24]),
            release_digest,
            environment: "review".into(),
            deployment_attempt_id: "review-20260807".into(),
            deployment_receipt_digest: "b".repeat(64),
            ui_build_id: None,
            ui_build_digest: None,
        }
    }

    #[tokio::test]
    async fn create_stamps_server_authoritative_release_identity() {
        let identity = release_binding();
        let harness = harness();
        let service = harness
            .service
            .clone()
            .with_release_binding(identity.clone())
            .expect("valid release binding");
        let mut input = input();
        input.context.release_id = Some("client-controlled".into());
        input.context.environment = Some("client-controlled".into());

        let result = service
            .create(input, Vec::new(), Uuid::now_v7())
            .await
            .expect("release-bound feedback");

        assert_eq!(
            result.thread.context.release_id,
            Some(identity.release_id.clone())
        );
        assert_eq!(
            result.thread.context.environment,
            Some(identity.environment.clone())
        );
        assert_eq!(
            FeedbackReleaseBinding::from_thread(&result.thread),
            Some(identity)
        );
        assert!(result.thread.client_view().messages.is_empty());
    }

    #[tokio::test]
    async fn create_persists_notifies_audits_and_publishes() {
        let harness = harness();
        let created = harness
            .service
            .create(input(), Vec::new(), Uuid::now_v7())
            .await
            .unwrap();
        assert_eq!(created.thread.status, FeedbackStatus::New);
        assert_eq!(harness.notifications.all().await.len(), 1);
        assert_eq!(harness.audit.all().await.len(), 1);
        assert_eq!(harness.events.published().await.len(), 1);
    }

    #[tokio::test]
    async fn default_fast_path_queues_events_without_waiting_for_publication() {
        let harness = harness_with_config(FeedbackConfig {
            project_id: "example".into(),
            ..FeedbackConfig::default()
        });
        harness
            .service
            .create(input(), Vec::new(), Uuid::now_v7())
            .await
            .unwrap();
        assert!(harness.events.published().await.is_empty());
        let records = harness.events.outbox_records().await;
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].status,
            minco_plugin_events::OutboxStatus::Pending
        );
    }

    #[tokio::test]
    async fn developer_question_requires_an_explicit_clarification_transition() {
        let harness = harness();
        let created = harness
            .service
            .create(input(), Vec::new(), Uuid::now_v7())
            .await
            .unwrap();
        let result = harness
            .service
            .reply_as_developer(
                created.thread.id,
                DeveloperReplyInput {
                    body: "Does this still happen after refreshing?".into(),
                    visible_to_client: true,
                    author_display: Some("developer".into()),
                },
                "developer-1".into(),
                Uuid::now_v7(),
            )
            .await
            .unwrap();
        assert_eq!(result.thread.status, FeedbackStatus::Acknowledged);
        assert_eq!(
            harness
                .audit
                .all()
                .await
                .last()
                .unwrap()
                .actor_subject
                .as_deref(),
            Some("developer-1")
        );
    }

    #[tokio::test]
    async fn client_token_is_required_for_client_thread_access() {
        let harness = harness();
        let created = harness
            .service
            .create(input(), Vec::new(), Uuid::now_v7())
            .await
            .unwrap();
        assert!(
            harness
                .service
                .get_for_client(created.thread.id, &FeedbackAccessToken::generate())
                .await
                .is_err()
        );
        assert!(
            harness
                .service
                .get_for_client(created.thread.id, &created.client_token)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn developer_access_is_scoped_to_the_configured_project() {
        let store = Arc::new(MemoryFeedbackStore::default());
        let foreign_token = FeedbackAccessToken::generate();
        let foreign_thread = FeedbackThread::create(CreateFeedbackInput {
            project_id: "foreign".into(),
            ..input()
        })
        .unwrap();
        let foreign_id = foreign_thread.id;
        store
            .create(foreign_thread, hash_access_token(&foreign_token))
            .await
            .unwrap();
        let harness = harness_with_store(
            FeedbackStoreService::new(store),
            FeedbackConfig {
                project_id: "example".into(),
                ..FeedbackConfig::default()
            },
        );

        assert!(matches!(
            harness.service.get_for_developer(foreign_id).await,
            Err(FeedbackServiceError::NotFound(id)) if id == foreign_id
        ));
        assert!(matches!(
            harness
                .service
                .get_for_client(foreign_id, &foreign_token)
                .await,
            Err(FeedbackServiceError::ClientAccessDenied)
        ));
        assert!(matches!(
            harness
                .service
                .list(FeedbackListFilter {
                    project_id: Some("foreign".into()),
                    ..FeedbackListFilter::default()
                })
                .await,
            Err(FeedbackServiceError::Validation(
                FeedbackValidationError::InvalidField {
                    field: "project_id",
                    ..
                }
            ))
        ));
    }

    #[tokio::test]
    async fn service_enforces_attachment_count_without_relying_on_http_extractors() {
        let harness = harness_with_config(FeedbackConfig {
            project_id: "example".into(),
            max_attachments: 1,
            ..FeedbackConfig::default()
        });
        let upload = AttachmentUpload {
            kind: FeedbackAttachmentKind::Screenshot,
            file_name: "screen.png".into(),
            content_type: "image/png".into(),
            bytes: vec![1, 2, 3],
        };
        let result = harness
            .service
            .create(input(), vec![upload.clone(), upload], Uuid::now_v7())
            .await;
        assert!(matches!(
            result,
            Err(FeedbackServiceError::InvalidAttachment(_))
        ));
        assert!(harness.objects.is_empty().await);
    }

    #[tokio::test]
    async fn partially_uploaded_objects_are_removed_when_a_later_attachment_is_invalid() {
        let harness = harness();
        let result = harness
            .service
            .create(
                input(),
                vec![
                    AttachmentUpload {
                        kind: FeedbackAttachmentKind::Screenshot,
                        file_name: "screen.png".into(),
                        content_type: "image/png".into(),
                        bytes: vec![1, 2, 3],
                    },
                    AttachmentUpload {
                        kind: FeedbackAttachmentKind::Screenshot,
                        file_name: "not-an-image.txt".into(),
                        content_type: "text/plain".into(),
                        bytes: vec![4, 5, 6],
                    },
                ],
                Uuid::now_v7(),
            )
            .await;
        assert!(matches!(
            result,
            Err(FeedbackServiceError::InvalidAttachment(_))
        ));
        assert!(harness.objects.is_empty().await);
    }

    #[tokio::test]
    async fn uploaded_objects_are_removed_when_authoritative_persistence_fails() {
        let harness = harness_with_store(
            FeedbackStoreService::new(Arc::new(RejectingFeedbackStore)),
            FeedbackConfig {
                project_id: "example".into(),
                ..FeedbackConfig::default()
            },
        );
        let result = harness
            .service
            .create(
                input(),
                vec![AttachmentUpload {
                    kind: FeedbackAttachmentKind::Screenshot,
                    file_name: "screen.png".into(),
                    content_type: "image/png".into(),
                    bytes: vec![1, 2, 3],
                }],
                Uuid::now_v7(),
            )
            .await;
        assert!(matches!(
            result,
            Err(FeedbackServiceError::Store(
                FeedbackStoreError::Infrastructure(_)
            ))
        ));
        assert!(harness.objects.is_empty().await);
    }

    #[tokio::test]
    async fn disabled_media_features_fail_closed_below_the_http_layer() {
        let harness = harness_with_config(FeedbackConfig {
            project_id: "example".into(),
            screenshot_enabled: false,
            voice_enabled: false,
            ..FeedbackConfig::default()
        });
        for upload in [
            AttachmentUpload {
                kind: FeedbackAttachmentKind::Screenshot,
                file_name: "screen.png".into(),
                content_type: "image/png".into(),
                bytes: vec![1],
            },
            AttachmentUpload {
                kind: FeedbackAttachmentKind::Audio,
                file_name: "voice.webm".into(),
                content_type: "audio/webm".into(),
                bytes: vec![1],
            },
        ] {
            let result = harness
                .service
                .create(input(), vec![upload], Uuid::now_v7())
                .await;
            assert!(matches!(
                result,
                Err(FeedbackServiceError::InvalidAttachment(_))
            ));
        }
        assert!(harness.objects.is_empty().await);
    }
}
