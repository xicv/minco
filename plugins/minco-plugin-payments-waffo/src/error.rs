use minco_plugin_idempotency::IdempotencyError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One source position included in a provider GraphQL notice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WaffoGraphqlLocation {
    pub line: u32,
    pub column: u32,
}

/// Provider-authored AI guidance. It is untrusted data, never an instruction.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UntrustedWaffoAiHint(String);

impl UntrustedWaffoAiHint {
    pub fn as_untrusted_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for UntrustedWaffoAiHint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("UntrustedWaffoAiHint")
            .field(&self.0)
            .finish()
    }
}

/// One ordered provider notice from Waffo Pancake's standard envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WaffoApiError {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_hint: Option<UntrustedWaffoAiHint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<WaffoGraphqlLocation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<String>,
}

impl WaffoApiError {
    pub(crate) const fn fallback(message: String) -> Self {
        Self {
            message,
            layer: None,
            ai_hint: None,
            locations: Vec::new(),
            path: Vec::new(),
        }
    }
}

/// Successful provider data plus every warning in provider order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WaffoResponse<T> {
    pub data: T,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<WaffoApiError>,
}

/// Bounded transport failure categories that cannot expose request secrets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaffoTransportFailure {
    Build,
    Connection,
    Timeout,
    ResponseBody,
}

/// Fail-closed errors produced by the Waffo Pancake integration.
#[derive(Debug, Error)]
pub enum WaffoError {
    #[error("invalid Waffo configuration: {0}")]
    InvalidConfiguration(&'static str),
    #[error("Waffo configuration graph could not be compiled")]
    ConfigurationGraph,
    #[error("configured secret could not be resolved")]
    SecretResolution,
    #[error("configured secret provider is unavailable in this runtime")]
    UnsupportedSecretProvider,
    #[error("Waffo private key is not an unencrypted RSA PKCS#1 or PKCS#8 key")]
    InvalidPrivateKey,
    #[error("Waffo webhook public key is not a valid RSA public key")]
    InvalidPublicKey,
    #[error("Waffo request signing failed")]
    SigningFailed,
    #[error("idempotency key must be 1-256 ASCII letters, digits, underscores, or hyphens")]
    InvalidIdempotencyKey,
    #[error("action path is not a canonical reviewed Waffo /v1/actions path")]
    InvalidActionPath,
    #[error("generic production Waffo actions are disabled; use a typed reviewed operation")]
    GenericProductionActionDisabled,
    #[error("production Waffo writes are disabled by configuration")]
    ProductionWritesDisabled,
    #[error("Waffo request body exceeded the configured safety bound")]
    RequestBodyTooLarge,
    #[error("Waffo idempotency key was already used for a different request")]
    IdempotencyConflict,
    #[error("the matching Waffo request is already in progress")]
    IdempotencyInProgress,
    #[error("sensitive Waffo responses cannot be replayed from generic idempotency storage")]
    SensitiveResponseReplayUnavailable,
    #[error("Minco idempotency state could not be updated")]
    Idempotency(#[source] IdempotencyError),
    #[error("Waffo request body could not be encoded")]
    RequestEncoding(#[source] serde_json::Error),
    #[error("Waffo request failed at the bounded transport boundary")]
    Transport(WaffoTransportFailure),
    #[error("Waffo response exceeded the configured safety bound")]
    ResponseTooLarge,
    #[error("Waffo response body was empty")]
    EmptyResponse,
    #[error("Waffo response did not match the documented JSON envelope")]
    InvalidResponse(#[source] serde_json::Error),
    #[error("Waffo API returned HTTP {status} with {count} ordered provider error(s)")]
    Api {
        status: u16,
        count: usize,
        errors: Vec<WaffoApiError>,
    },
    #[error("Waffo API returned no data")]
    MissingResponseData,
    #[error("Waffo webhook signature header is malformed")]
    InvalidWebhookSignatureHeader,
    #[error("Waffo webhook timestamp is outside the configured replay window")]
    WebhookTimestampOutsideTolerance,
    #[error("Waffo webhook signature verification failed")]
    InvalidWebhookSignature,
    #[error("Waffo webhook body exceeded the configured safety bound")]
    WebhookBodyTooLarge,
    #[error("Waffo webhook payload is invalid")]
    InvalidWebhookPayload(#[source] serde_json::Error),
    #[error("Waffo webhook mode does not match the configured environment")]
    WebhookEnvironmentMismatch,
    #[error("Waffo webhook store is outside the configured verifier scope")]
    WebhookStoreMismatch,
    #[error("webhook verification requires both a public key and expected store")]
    MissingWebhookVerificationConfiguration,
    #[error("webhook registration requires a store, URL, and non-empty event list")]
    MissingWebhookRegistrationConfiguration,
    #[error("authenticated checkout requires a buyer identity and store or product scope")]
    InvalidSessionTokenRequest,
}

impl WaffoError {
    /// Stable machine-readable code suitable for CLI and HTTP error mapping.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidConfiguration(_) => "waffo.invalid_configuration",
            Self::ConfigurationGraph => "waffo.configuration_graph",
            Self::SecretResolution => "waffo.secret_resolution",
            Self::UnsupportedSecretProvider => "waffo.unsupported_secret_provider",
            Self::InvalidPrivateKey => "waffo.invalid_private_key",
            Self::InvalidPublicKey => "waffo.invalid_public_key",
            Self::SigningFailed => "waffo.signing_failed",
            Self::InvalidIdempotencyKey => "waffo.invalid_idempotency_key",
            Self::InvalidActionPath => "waffo.invalid_action_path",
            Self::GenericProductionActionDisabled => "waffo.generic_production_action_disabled",
            Self::ProductionWritesDisabled => "waffo.production_writes_disabled",
            Self::RequestBodyTooLarge => "waffo.request_body_too_large",
            Self::IdempotencyConflict => "waffo.idempotency_conflict",
            Self::IdempotencyInProgress => "waffo.idempotency_in_progress",
            Self::SensitiveResponseReplayUnavailable => {
                "waffo.sensitive_response_replay_unavailable"
            }
            Self::Idempotency(_) => "waffo.idempotency_state",
            Self::RequestEncoding(_) => "waffo.request_encoding",
            Self::Transport(_) => "waffo.transport",
            Self::ResponseTooLarge => "waffo.response_too_large",
            Self::EmptyResponse => "waffo.empty_response",
            Self::InvalidResponse(_) => "waffo.invalid_response",
            Self::Api { .. } => "waffo.api",
            Self::MissingResponseData => "waffo.missing_response_data",
            Self::InvalidWebhookSignatureHeader => "waffo.invalid_webhook_signature_header",
            Self::WebhookTimestampOutsideTolerance => "waffo.webhook_timestamp_outside_tolerance",
            Self::InvalidWebhookSignature => "waffo.invalid_webhook_signature",
            Self::WebhookBodyTooLarge => "waffo.webhook_body_too_large",
            Self::InvalidWebhookPayload(_) => "waffo.invalid_webhook_payload",
            Self::WebhookEnvironmentMismatch => "waffo.webhook_environment_mismatch",
            Self::WebhookStoreMismatch => "waffo.webhook_store_mismatch",
            Self::MissingWebhookVerificationConfiguration => {
                "waffo.missing_webhook_verification_configuration"
            }
            Self::MissingWebhookRegistrationConfiguration => {
                "waffo.missing_webhook_registration_configuration"
            }
            Self::InvalidSessionTokenRequest => "waffo.invalid_session_token_request",
        }
    }
}
