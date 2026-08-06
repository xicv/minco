// This private helper module exposes selected items to sibling modules through
// the crate root; `pub(super)` is intentional and narrower than public API.
#![allow(clippy::redundant_pub_crate)]

use crate::config::{
    DEFAULT_API_BASE_URL, DEFAULT_REQUEST_MAX_BYTES, DEFAULT_RESPONSE_MAX_BYTES,
    DEFAULT_WEBHOOK_MAX_BYTES,
};
use minco_core::{ConfigurationField, ConfigurationValueKind};
use serde_json::json;

pub(super) fn configuration_fields() -> Vec<ConfigurationField> {
    vec![
        field(
            "environment",
            ConfigurationValueKind::String,
            false,
            false,
            "Waffo API-key environment: test or production",
            Some(json!("test")),
        ),
        field(
            "merchant_id",
            ConfigurationValueKind::String,
            true,
            false,
            "Waffo merchant short ID",
            None,
        ),
        field(
            "private_key",
            ConfigurationValueKind::String,
            true,
            true,
            "Opaque env: or ssm: reference to the unencrypted RSA private key",
            None,
        ),
        field(
            "webhook_public_key",
            ConfigurationValueKind::String,
            false,
            true,
            "Opaque env: or ssm: reference to Waffo's environment-specific webhook public key",
            None,
        ),
        field(
            "store_id",
            ConfigurationValueKind::String,
            false,
            false,
            "Store short ID used by webhook automation",
            None,
        ),
        field(
            "api_base_url",
            ConfigurationValueKind::String,
            false,
            false,
            "HTTPS Waffo API origin; override only for an explicitly trusted compatible endpoint",
            Some(json!(DEFAULT_API_BASE_URL)),
        ),
        field(
            "allow_custom_api_base_url",
            ConfigurationValueKind::Boolean,
            false,
            false,
            "Permit an explicitly configured compatible HTTPS endpoint for test credentials only",
            Some(json!(false)),
        ),
        field(
            "request_timeout_seconds",
            ConfigurationValueKind::Integer,
            false,
            false,
            "Bounded timeout for one provider request",
            Some(json!(30)),
        ),
        field(
            "request_max_bytes",
            ConfigurationValueKind::Integer,
            false,
            false,
            "Maximum provider request body retained in memory",
            Some(json!(DEFAULT_REQUEST_MAX_BYTES)),
        ),
        field(
            "response_max_bytes",
            ConfigurationValueKind::Integer,
            false,
            false,
            "Maximum provider response body retained in memory",
            Some(json!(DEFAULT_RESPONSE_MAX_BYTES)),
        ),
        field(
            "webhook_tolerance_seconds",
            ConfigurationValueKind::Integer,
            false,
            false,
            "Maximum accepted webhook timestamp skew",
            Some(json!(300)),
        ),
        field(
            "webhook_max_bytes",
            ConfigurationValueKind::Integer,
            false,
            false,
            "Maximum raw webhook body accepted for verification",
            Some(json!(DEFAULT_WEBHOOK_MAX_BYTES)),
        ),
        field(
            "allow_production_writes",
            ConfigurationValueKind::Boolean,
            false,
            false,
            "Explicit persisted guard required before production actions can mutate Waffo",
            Some(json!(false)),
        ),
        field(
            "webhook_url",
            ConfigurationValueKind::String,
            false,
            false,
            "HTTPS endpoint registered by the webhook-add CLI command",
            None,
        ),
        field(
            "webhook_events",
            ConfigurationValueKind::StringList,
            false,
            false,
            "Waffo event types registered by the webhook-add CLI command",
            Some(json!([])),
        ),
    ]
}

fn field(
    key: &str,
    kind: ConfigurationValueKind,
    required: bool,
    secret: bool,
    description: &str,
    default: Option<serde_json::Value>,
) -> ConfigurationField {
    ConfigurationField {
        key: key.into(),
        kind,
        required,
        secret,
        description: description.into(),
        default,
    }
}
