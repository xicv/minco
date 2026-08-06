use minco_plugin_idempotency::IdempotencyError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One provider error returned in Waffo Pancake's standard response envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaffoApiError {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
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
    #[error("action path must be a relative Waffo /v1/actions path without a query or fragment")]
    InvalidActionPath,
    #[error("production Waffo writes are disabled by configuration")]
    ProductionWritesDisabled,
    #[error("Waffo request body exceeded the configured safety bound")]
    RequestBodyTooLarge,
    #[error("Waffo idempotency key was already used for a different request")]
    IdempotencyConflict,
    #[error("the matching Waffo request is already in progress")]
    IdempotencyInProgress,
    #[error("Minco idempotency state could not be updated")]
    Idempotency(#[source] IdempotencyError),
    #[error("Waffo request body could not be encoded")]
    RequestEncoding(#[source] serde_json::Error),
    #[error("Waffo request failed before a response was received")]
    Transport(#[source] reqwest::Error),
    #[error("Waffo response exceeded the configured safety bound")]
    ResponseTooLarge,
    #[error("Waffo response did not match the documented JSON envelope")]
    InvalidResponse(#[source] serde_json::Error),
    #[error("Waffo API returned HTTP {status}: {message}")]
    Api {
        status: u16,
        message: String,
        #[allow(dead_code)]
        layer: Option<String>,
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
    #[error("store, URL, events, and public key must be configured for this webhook command")]
    MissingWebhookConfiguration,
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
            Self::ProductionWritesDisabled => "waffo.production_writes_disabled",
            Self::RequestBodyTooLarge => "waffo.request_body_too_large",
            Self::IdempotencyConflict => "waffo.idempotency_conflict",
            Self::IdempotencyInProgress => "waffo.idempotency_in_progress",
            Self::Idempotency(_) => "waffo.idempotency_state",
            Self::RequestEncoding(_) => "waffo.request_encoding",
            Self::Transport(_) => "waffo.transport",
            Self::ResponseTooLarge => "waffo.response_too_large",
            Self::InvalidResponse(_) => "waffo.invalid_response",
            Self::Api { .. } => "waffo.api",
            Self::MissingResponseData => "waffo.missing_response_data",
            Self::InvalidWebhookSignatureHeader => "waffo.invalid_webhook_signature_header",
            Self::WebhookTimestampOutsideTolerance => "waffo.webhook_timestamp_outside_tolerance",
            Self::InvalidWebhookSignature => "waffo.invalid_webhook_signature",
            Self::WebhookBodyTooLarge => "waffo.webhook_body_too_large",
            Self::InvalidWebhookPayload(_) => "waffo.invalid_webhook_payload",
            Self::WebhookEnvironmentMismatch => "waffo.webhook_environment_mismatch",
            Self::MissingWebhookConfiguration => "waffo.missing_webhook_configuration",
        }
    }
}
