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
            .field("audio", &"[REDACTED]")
            .field("size_bytes", &self.bytes.len())
            .field("file_name", &"[REDACTED]")
            .field("content_type", &self.content_type)
            .field("language", &self.language)
            .field("prompt", &self.prompt.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub provider: String,
    pub model: String,
}

pub type AudioInput = TranscriptionRequest;
pub type Transcript = TranscriptionResult;

#[async_trait]
pub trait Transcriber: Send + Sync + fmt::Debug {
    async fn transcribe(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionResult, TranscriptionError>;
}

#[derive(Clone)]
pub struct TranscriptionService(pub Arc<dyn Transcriber>);

impl fmt::Debug for TranscriptionService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("TranscriptionService").finish()
    }
}

impl TranscriptionService {
    pub fn new(transcriber: Arc<dyn Transcriber>) -> Self {
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
impl Transcriber for DisabledTranscriber {
    async fn transcribe(
        &self,
        _request: TranscriptionRequest,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        Err(TranscriptionError::NotConfigured)
    }
}

#[cfg(feature = "openai-transcription")]
#[derive(Clone)]
pub struct OpenAiTranscriber {
    client: reqwest::Client,
    api_key: Arc<str>,
    endpoint: Arc<str>,
    model: Arc<str>,
}

#[cfg(feature = "openai-transcription")]
impl fmt::Debug for OpenAiTranscriber {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiTranscriber")
            .field("client", &self.client)
            .field("endpoint", &"[CONFIGURED]")
            .field("model", &self.model)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

#[cfg(feature = "openai-transcription")]
impl OpenAiTranscriber {
    pub fn new(api_key: impl Into<String>) -> Result<Self, TranscriptionError> {
        Self::with_options(
            api_key,
            "https://api.openai.com/v1/audio/transcriptions",
            "gpt-4o-mini-transcribe",
        )
    }

    pub fn from_env(variable: &str) -> Result<Self, TranscriptionError> {
        let api_key = std::env::var(variable)
            .map_err(|_| TranscriptionError::MissingApiKey(variable.to_owned()))?;
        Self::new(api_key)
    }

    pub fn with_options(
        api_key: impl Into<String>,
        endpoint: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, TranscriptionError> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(TranscriptionError::MissingApiKey(
                "configured OpenAI API key".into(),
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_mins(1))
            .build()
            .map_err(|error| TranscriptionError::Provider(error.to_string()))?;
        Ok(Self {
            client,
            api_key: api_key.into(),
            endpoint: endpoint.into().into(),
            model: model.into().into(),
        })
    }
}

#[cfg(feature = "openai-transcription")]
#[derive(Debug, Deserialize)]
struct OpenAiTranscriptionResponse {
    text: String,
}

#[cfg(feature = "openai-transcription")]
#[async_trait]
impl Transcriber for OpenAiTranscriber {
    async fn transcribe(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        validate_audio(&request)?;
        let part = reqwest::multipart::Part::bytes(request.bytes)
            .file_name(request.file_name)
            .mime_str(&request.content_type)
            .map_err(|error| TranscriptionError::InvalidAudio(error.to_string()))?;
        let mut form = reqwest::multipart::Form::new()
            .text("model", self.model.to_string())
            .part("file", part);
        if let Some(language) = request.language.filter(|value| !value.trim().is_empty()) {
            form = form.text("language", language);
        }
        if let Some(prompt) = request.prompt.filter(|value| !value.trim().is_empty()) {
            form = form.text("prompt", prompt);
        }
        let response = self
            .client
            .post(self.endpoint.as_ref())
            .bearer_auth(self.api_key.as_ref())
            .multipart(form)
            .send()
            .await
            .map_err(|error| TranscriptionError::Provider(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(TranscriptionError::Provider(format!(
                "OpenAI transcription returned {status}: {}",
                truncate_text(&detail, 500)
            )));
        }
        let payload = response
            .json::<OpenAiTranscriptionResponse>()
            .await
            .map_err(|error| TranscriptionError::Provider(error.to_string()))?;
        if payload.text.trim().is_empty() {
            return Err(TranscriptionError::Provider(
                "transcription provider returned empty text".into(),
            ));
        }
        Ok(TranscriptionResult {
            text: payload.text,
            provider: "openai".into(),
            model: self.model.to_string(),
        })
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

    fn rendered_arguments(&self, input: &std::path::Path) -> Vec<std::ffi::OsString> {
        let input = input.as_os_str().to_os_string();
        let input_text = input.to_string_lossy();
        let mut replaced = false;
        let mut arguments = self
            .arguments
            .iter()
            .map(|argument| {
                if argument.contains("{input}") {
                    replaced = true;
                    std::ffi::OsString::from(argument.replace("{input}", &input_text))
                } else {
                    std::ffi::OsString::from(argument)
                }
            })
            .collect::<Vec<_>>();
        if !replaced {
            arguments.push(input);
        }
        arguments
    }
}

#[cfg(feature = "command-transcription")]
#[async_trait]
impl Transcriber for CommandTranscriber {
    async fn transcribe(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        validate_audio(&request)?;
        if self.timeout.is_zero() {
            return Err(TranscriptionError::Provider(
                "command transcription timeout must be greater than zero".into(),
            ));
        }
        let directory =
            tempfile::tempdir().map_err(|error| TranscriptionError::Provider(error.to_string()))?;
        let extension = safe_audio_extension(&request.file_name, &request.content_type);
        let input = directory
            .path()
            .join(format!("interaction-audio.{extension}"));
        tokio::fs::write(&input, request.bytes)
            .await
            .map_err(|error| TranscriptionError::Provider(error.to_string()))?;
        let mut command = tokio::process::Command::new(self.program.as_ref());
        command
            .args(self.rendered_arguments(&input))
            .kill_on_drop(true)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let output = tokio::time::timeout(self.timeout, command.output())
            .await
            .map_err(|_| {
                TranscriptionError::Provider(format!(
                    "transcription command timed out after {} seconds",
                    self.timeout.as_secs()
                ))
            })?
            .map_err(|error| TranscriptionError::Provider(error.to_string()))?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr);
            return Err(TranscriptionError::Provider(format!(
                "transcription command exited with {}: {}",
                output.status,
                truncate_text(&detail, 500)
            )));
        }
        let text = String::from_utf8(output.stdout)
            .map_err(|error| TranscriptionError::Provider(error.to_string()))?;
        let text = text.trim().to_owned();
        if text.is_empty() {
            return Err(TranscriptionError::Provider(
                "transcription command returned empty stdout".into(),
            ));
        }
        Ok(TranscriptionResult {
            text,
            provider: self.provider.to_string(),
            model: self.model.to_string(),
        })
    }
}

#[cfg(any(feature = "openai-transcription", feature = "command-transcription"))]
fn validate_audio(request: &TranscriptionRequest) -> Result<(), TranscriptionError> {
    if request.bytes.is_empty() {
        Err(TranscriptionError::InvalidAudio("audio is empty".into()))
    } else {
        Ok(())
    }
}

#[cfg(feature = "command-transcription")]
fn safe_audio_extension(file_name: &str, content_type: &str) -> String {
    let extension = std::path::Path::new(file_name)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 10
                && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
        });
    extension.map_or_else(
        || match content_type.to_ascii_lowercase().as_str() {
            "audio/mpeg" | "audio/mp3" => "mp3".into(),
            "audio/mp4" | "audio/x-m4a" => "m4a".into(),
            "audio/ogg" => "ogg".into(),
            "audio/wav" | "audio/x-wav" => "wav".into(),
            _ => "webm".into(),
        },
        str::to_ascii_lowercase,
    )
}

#[cfg(any(feature = "openai-transcription", feature = "command-transcription"))]
fn truncate_text(value: &str, maximum: usize) -> String {
    value.chars().take(maximum).collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn disabled_transcriber_fails_explicitly_and_debug_hides_audio() {
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

    #[cfg(feature = "command-transcription")]
    #[test]
    fn command_arguments_are_direct_and_bounded() {
        let input = std::path::Path::new("/tmp/example.webm");
        let arguments = CommandTranscriber::new("whisper")
            .with_arguments(["--file", concat!("{", "input", "}")])
            .rendered_arguments(input);
        assert_eq!(arguments[1], std::ffi::OsString::from("/tmp/example.webm"));
        assert_eq!(safe_audio_extension("voice", "audio/ogg"), "ogg");
    }
}
