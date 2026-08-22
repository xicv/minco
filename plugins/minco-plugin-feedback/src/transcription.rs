//! Feedback-facing compatibility types backed by shared interaction providers.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{fmt, sync::Arc};

#[derive(Clone, PartialEq, Eq)]
pub struct TranscriptionRequest {
    pub bytes: Vec<u8>,
    pub file_name: String,
    pub content_type: String,
    pub language: Option<String>,
    pub prompt: Option<String>,
}

impl fmt::Debug for TranscriptionRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TranscriptionRequest")
            .field("size_bytes", &self.bytes.len())
            .field("file_name", &"[REDACTED]")
            .field("content_type", &self.content_type)
            .field("language", &self.language)
            .field("prompt", &self.prompt.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

impl From<TranscriptionRequest> for minco_interaction::TranscriptionRequest {
    fn from(value: TranscriptionRequest) -> Self {
        Self {
            bytes: value.bytes,
            file_name: value.file_name,
            content_type: value.content_type,
            language: value.language,
            prompt: value.prompt,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub provider: String,
    pub model: String,
}

impl From<minco_interaction::TranscriptionResult> for TranscriptionResult {
    fn from(value: minco_interaction::TranscriptionResult) -> Self {
        Self {
            text: value.text,
            provider: value.provider,
            model: value.model,
        }
    }
}

/// Stable feedback-facing name for an audio transcription request.
pub type AudioInput = TranscriptionRequest;

/// Stable feedback-facing name for a completed transcript.
pub type Transcript = TranscriptionResult;

#[async_trait]
pub trait FeedbackTranscriber: Send + Sync + fmt::Debug {
    async fn transcribe(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionResult, TranscriptionError>;
}

#[derive(Clone)]
pub struct TranscriptionService(pub Arc<dyn FeedbackTranscriber>);

impl fmt::Debug for TranscriptionService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("TranscriptionService").finish()
    }
}

impl TranscriptionService {
    pub fn new(transcriber: Arc<dyn FeedbackTranscriber>) -> Self {
        Self(transcriber)
    }

    pub async fn transcribe(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        self.0.transcribe(request).await
    }
}

#[derive(Debug, Default)]
pub struct DisabledTranscriber;

#[async_trait]
impl FeedbackTranscriber for DisabledTranscriber {
    async fn transcribe(
        &self,
        _request: TranscriptionRequest,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        Err(TranscriptionError::NotConfigured)
    }
}

#[cfg(feature = "openai-transcription")]
#[derive(Clone)]
pub struct OpenAiTranscriber(minco_interaction::OpenAiTranscriber);

#[cfg(feature = "openai-transcription")]
impl fmt::Debug for OpenAiTranscriber {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("OpenAiTranscriber")
            .field(&self.0)
            .finish()
    }
}

#[cfg(feature = "openai-transcription")]
impl OpenAiTranscriber {
    pub fn new(api_key: impl Into<String>) -> Result<Self, TranscriptionError> {
        minco_interaction::OpenAiTranscriber::new(api_key)
            .map(Self)
            .map_err(Into::into)
    }

    pub fn from_env(variable: &str) -> Result<Self, TranscriptionError> {
        minco_interaction::OpenAiTranscriber::from_env(variable)
            .map(Self)
            .map_err(Into::into)
    }

    pub fn with_options(
        api_key: impl Into<String>,
        endpoint: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, TranscriptionError> {
        minco_interaction::OpenAiTranscriber::with_options(api_key, endpoint, model)
            .map(Self)
            .map_err(Into::into)
    }
}

#[cfg(feature = "openai-transcription")]
#[async_trait]
impl FeedbackTranscriber for OpenAiTranscriber {
    async fn transcribe(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        minco_interaction::Transcriber::transcribe(&self.0, request.into())
            .await
            .map(Into::into)
            .map_err(Into::into)
    }
}

#[cfg(feature = "command-transcription")]
#[derive(Clone)]
pub struct CommandTranscriber {
    program: Arc<std::path::PathBuf>,
    arguments: Arc<Vec<String>>,
    provider: Arc<str>,
    model: Arc<str>,
    timeout: std::time::Duration,
}

#[cfg(feature = "command-transcription")]
impl fmt::Debug for CommandTranscriber {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommandTranscriber")
            .field("program", &self.program)
            .field("argument_count", &self.arguments.len())
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("timeout", &self.timeout)
            .finish()
    }
}

#[cfg(feature = "command-transcription")]
impl CommandTranscriber {
    #[must_use]
    pub fn new(program: impl Into<std::path::PathBuf>) -> Self {
        Self {
            program: Arc::new(program.into()),
            arguments: Arc::new(Vec::new()),
            provider: "command".into(),
            model: "local".into(),
            timeout: std::time::Duration::from_mins(2),
        }
    }

    #[must_use]
    pub fn with_arguments<I, S>(mut self, arguments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arguments = Arc::new(arguments.into_iter().map(Into::into).collect());
        self
    }

    #[must_use]
    pub fn with_identity(mut self, provider: impl Into<String>, model: impl Into<String>) -> Self {
        self.provider = provider.into().into();
        self.model = model.into().into();
        self
    }

    #[must_use]
    pub const fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[cfg(feature = "command-transcription")]
#[async_trait]
impl FeedbackTranscriber for CommandTranscriber {
    async fn transcribe(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        let transcriber = minco_interaction::CommandTranscriber::new(self.program.as_ref().clone())
            .with_arguments(self.arguments.iter().cloned())
            .with_identity(self.provider.to_string(), self.model.to_string())
            .with_timeout(self.timeout);
        minco_interaction::Transcriber::transcribe(&transcriber, request.into())
            .await
            .map(Into::into)
            .map_err(Into::into)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TranscriptionError {
    #[error("voice transcription provider is not configured")]
    NotConfigured,
    #[error("missing transcription API key: {0}")]
    MissingApiKey(String),
    #[error("invalid audio: {0}")]
    InvalidAudio(String),
    #[error("transcription provider failed: {0}")]
    Provider(String),
}

impl From<minco_interaction::TranscriptionError> for TranscriptionError {
    fn from(value: minco_interaction::TranscriptionError) -> Self {
        match value {
            minco_interaction::TranscriptionError::NotConfigured => Self::NotConfigured,
            minco_interaction::TranscriptionError::MissingApiKey(value) => {
                Self::MissingApiKey(value)
            }
            minco_interaction::TranscriptionError::InvalidAudio(value) => Self::InvalidAudio(value),
            minco_interaction::TranscriptionError::Provider(value) => Self::Provider(value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn compatibility_types_delegate_without_exposing_audio() {
        let request = TranscriptionRequest {
            bytes: b"secret-audio".to_vec(),
            file_name: "voice.webm".into(),
            content_type: "audio/webm".into(),
            language: None,
            prompt: Some("private prompt".into()),
        };
        let debug = format!("{request:?}");
        assert!(!debug.contains("secret-audio"));
        assert!(!debug.contains("private prompt"));
        assert!(matches!(
            DisabledTranscriber.transcribe(request).await,
            Err(TranscriptionError::NotConfigured)
        ));
    }
}
