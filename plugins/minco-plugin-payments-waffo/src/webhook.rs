use crate::{WaffoEnvironment, WaffoError, identifier::validate_short_id, signing::decode_pem};
use aws_lc_rs::{rsa, signature};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fmt, sync::Arc, time::Duration};

/// Environment marker carried by Waffo's HTTP webhook envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaffoWebhookMode {
    Test,
    #[serde(rename = "prod")]
    Production,
}

impl WaffoWebhookMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Test => "test",
            Self::Production => "prod",
        }
    }
}

/// Signed HTTP webhook envelope delivered by Waffo Pancake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaffoWebhookEvent {
    pub id: String,
    pub timestamp: String,
    pub event_type: String,
    pub event_id: String,
    pub store_id: String,
    pub store_name: String,
    pub mode: WaffoWebhookMode,
    pub data: Value,
}

/// Verified event plus bounded keys for delivery-level and semantic deduplication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedWaffoWebhook {
    pub timestamp_milliseconds: i64,
    pub delivery_dedupe_key: String,
    pub event_dedupe_key: String,
    pub event: WaffoWebhookEvent,
}

/// Raw-body RSA verifier for standard Waffo HTTP webhooks.
#[derive(Clone)]
pub struct WaffoWebhookVerifier {
    environment: WaffoEnvironment,
    expected_store_id: String,
    public_key_der: Arc<Vec<u8>>,
    past_tolerance: Duration,
    future_tolerance: Duration,
    max_body_bytes: usize,
}

impl fmt::Debug for WaffoWebhookVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WaffoWebhookVerifier")
            .field("environment", &self.environment)
            .field("expected_store_id", &self.expected_store_id)
            .field("algorithm", &"RSA-PKCS1-SHA256")
            .field("past_tolerance", &self.past_tolerance)
            .field("future_tolerance", &self.future_tolerance)
            .field("max_body_bytes", &self.max_body_bytes)
            .finish_non_exhaustive()
    }
}

impl WaffoWebhookVerifier {
    pub(super) fn from_pem(
        environment: WaffoEnvironment,
        expected_store_id: &str,
        public_key: &str,
        past_tolerance: Duration,
        future_tolerance: Duration,
        max_body_bytes: usize,
    ) -> Result<Self, WaffoError> {
        let (_, der) = decode_pem(public_key, &["PUBLIC KEY", "RSA PUBLIC KEY"])
            .map_err(|()| WaffoError::InvalidPublicKey)?;
        Self::from_der(
            environment,
            expected_store_id,
            &der,
            past_tolerance,
            future_tolerance,
            max_body_bytes,
        )
    }

    fn from_der(
        environment: WaffoEnvironment,
        expected_store_id: &str,
        public_key: &[u8],
        past_tolerance: Duration,
        future_tolerance: Duration,
        max_body_bytes: usize,
    ) -> Result<Self, WaffoError> {
        validate_short_id(expected_store_id, "STO")
            .map_err(|()| WaffoError::MissingWebhookVerificationConfiguration)?;
        let public_key =
            rsa::PublicKey::from_der(public_key).map_err(|_| WaffoError::InvalidPublicKey)?;
        Ok(Self {
            environment,
            expected_store_id: expected_store_id.to_owned(),
            public_key_der: Arc::new(public_key.as_ref().to_vec()),
            past_tolerance,
            future_tolerance,
            max_body_bytes,
        })
    }

    /// Verify the signature over the untouched body before deserializing the event.
    pub fn verify(
        &self,
        signature_header: &str,
        raw_body: &[u8],
    ) -> Result<VerifiedWaffoWebhook, WaffoError> {
        self.verify_at(signature_header, raw_body, Utc::now().timestamp_millis())
    }

    fn verify_at(
        &self,
        signature_header: &str,
        raw_body: &[u8],
        now_milliseconds: i64,
    ) -> Result<VerifiedWaffoWebhook, WaffoError> {
        if raw_body.len() > self.max_body_bytes {
            return Err(WaffoError::WebhookBodyTooLarge);
        }
        let (timestamp, signature_bytes) = parse_signature_header(signature_header)?;
        let past_tolerance = u64::try_from(self.past_tolerance.as_millis()).unwrap_or(u64::MAX);
        let future_tolerance = u64::try_from(self.future_tolerance.as_millis()).unwrap_or(u64::MAX);
        let outside_window = if timestamp <= now_milliseconds {
            timestamp.abs_diff(now_milliseconds) > past_tolerance
        } else {
            timestamp.abs_diff(now_milliseconds) > future_tolerance
        };
        if outside_window {
            return Err(WaffoError::WebhookTimestampOutsideTolerance);
        }

        let prefix = format!("{timestamp}.");
        let mut signed_message = Vec::with_capacity(prefix.len() + raw_body.len());
        signed_message.extend_from_slice(prefix.as_bytes());
        signed_message.extend_from_slice(raw_body);
        signature::UnparsedPublicKey::new(
            &signature::RSA_PKCS1_2048_8192_SHA256,
            self.public_key_der.as_slice(),
        )
        .verify(&signed_message, &signature_bytes)
        .map_err(|_| WaffoError::InvalidWebhookSignature)?;

        let event = serde_json::from_slice::<WaffoWebhookEvent>(raw_body)
            .map_err(WaffoError::InvalidWebhookPayload)?;
        validate_event(&event)?;
        let expected_mode = match self.environment {
            WaffoEnvironment::Test => WaffoWebhookMode::Test,
            WaffoEnvironment::Production => WaffoWebhookMode::Production,
        };
        if event.mode != expected_mode {
            return Err(WaffoError::WebhookEnvironmentMismatch);
        }
        if event.store_id != self.expected_store_id {
            return Err(WaffoError::WebhookStoreMismatch);
        }

        let mode = event.mode.as_str().as_bytes();
        let store = event.store_id.as_bytes();

        Ok(VerifiedWaffoWebhook {
            timestamp_milliseconds: timestamp,
            delivery_dedupe_key: digest_key(
                "waffo-delivery",
                &[b"waffo", mode, store, event.id.as_bytes()],
            ),
            event_dedupe_key: digest_key(
                "waffo-event",
                &[
                    b"waffo",
                    mode,
                    store,
                    event.event_type.as_bytes(),
                    event.event_id.as_bytes(),
                ],
            ),
            event,
        })
    }
}

fn parse_signature_header(value: &str) -> Result<(i64, Vec<u8>), WaffoError> {
    let mut timestamp = None;
    let mut signature_value = None;
    for part in value.split(',').map(str::trim) {
        if let Some(value) = part.strip_prefix("t=") {
            if timestamp.replace(value).is_some() {
                return Err(WaffoError::InvalidWebhookSignatureHeader);
            }
        } else if let Some(value) = part.strip_prefix("v1=")
            && signature_value.replace(value).is_some()
        {
            return Err(WaffoError::InvalidWebhookSignatureHeader);
        }
    }
    let timestamp = timestamp
        .ok_or(WaffoError::InvalidWebhookSignatureHeader)?
        .parse::<i64>()
        .map_err(|_| WaffoError::InvalidWebhookSignatureHeader)?;
    let signature = STANDARD
        .decode(signature_value.ok_or(WaffoError::InvalidWebhookSignatureHeader)?)
        .map_err(|_| WaffoError::InvalidWebhookSignatureHeader)?;
    if signature.is_empty() {
        return Err(WaffoError::InvalidWebhookSignatureHeader);
    }
    Ok((timestamp, signature))
}

fn validate_event(event: &WaffoWebhookEvent) -> Result<(), WaffoError> {
    for value in [
        event.id.as_str(),
        event.event_type.as_str(),
        event.event_id.as_str(),
        event.store_id.as_str(),
        event.store_name.as_str(),
    ] {
        if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
            return Err(WaffoError::InvalidConfiguration(
                "webhook identifiers must be bounded printable strings",
            ));
        }
    }
    if !event.timestamp.ends_with('Z')
        || chrono::DateTime::parse_from_rfc3339(&event.timestamp).is_err()
    {
        return Err(WaffoError::InvalidConfiguration(
            "webhook timestamp must be an ISO 8601 UTC value",
        ));
    }
    validate_short_id(&event.store_id, "STO").map_err(|()| {
        WaffoError::InvalidConfiguration(
            "webhook store_id must be a STO_ short ID with a 22-character base62 suffix",
        )
    })?;
    Ok(())
}

fn digest_key(prefix: &str, parts: &[&[u8]]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update(part);
        digest.update([0]);
    }
    format!("{prefix}-{}", URL_SAFE_NO_PAD.encode(digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_lc_rs::{
        rand::SystemRandom,
        rsa::{KeyPair, KeySize},
        signature::KeyPair as _,
    };
    use serde_json::json;

    fn signed_header(key_pair: &KeyPair, timestamp: i64, body: &[u8]) -> String {
        let prefix = format!("{timestamp}.");
        let mut message = Vec::with_capacity(prefix.len() + body.len());
        message.extend_from_slice(prefix.as_bytes());
        message.extend_from_slice(body);
        let mut signature_bytes = vec![0_u8; key_pair.public_modulus_len()];
        key_pair
            .sign(
                &signature::RSA_PKCS1_SHA256,
                &SystemRandom::new(),
                &message,
                &mut signature_bytes,
            )
            .unwrap();
        format!("t={timestamp},v1={}", STANDARD.encode(signature_bytes))
    }

    #[test]
    fn verifies_raw_body_and_builds_two_dedupe_scopes() {
        let key_pair = KeyPair::generate(KeySize::Rsa2048).unwrap();
        let public_key = key_pair.public_key().as_ref().to_vec();
        let verifier = WaffoWebhookVerifier::from_der(
            WaffoEnvironment::Test,
            "STO_0123456789ABCDEFGHIJKL",
            &public_key,
            Duration::from_mins(45),
            Duration::from_mins(1),
            1024,
        )
        .unwrap();
        let timestamp = 1_700_000_000_000_i64;
        let body = serde_json::to_vec(&json!({
            "id": "delivery-1",
            "timestamp": "2026-08-06T01:29:49.000Z",
            "eventType": "order.completed",
            "eventId": "ORD_0123456789ABCDEFGHIJKL",
            "storeId": "STO_0123456789ABCDEFGHIJKL",
            "storeName": "Test Store",
            "mode": "test",
            "data": { "status": "completed" }
        }))
        .unwrap();
        let header = signed_header(&key_pair, timestamp, &body);

        let event = verifier.verify_at(&header, &body, timestamp).unwrap();

        assert!(event.delivery_dedupe_key.starts_with("waffo-delivery-"));
        assert!(event.event_dedupe_key.starts_with("waffo-event-"));
        assert_ne!(event.delivery_dedupe_key, event.event_dedupe_key);
    }

    #[test]
    fn rejects_tampering_and_stale_delivery() {
        let key_pair = KeyPair::generate(KeySize::Rsa2048).unwrap();
        let public_key = key_pair.public_key().as_ref().to_vec();
        let verifier = WaffoWebhookVerifier::from_der(
            WaffoEnvironment::Test,
            "STO_0123456789ABCDEFGHIJKL",
            &public_key,
            Duration::from_mins(45),
            Duration::from_mins(1),
            1024,
        )
        .unwrap();
        let timestamp = 1_700_000_000_000_i64;
        let body = br#"{"id":"a","timestamp":"2026-08-06T01:29:49.000Z","eventType":"order.completed","eventId":"b","storeId":"STO_0123456789ABCDEFGHIJKL","storeName":"Test Store","mode":"test","data":{}}"#;
        let header = signed_header(&key_pair, timestamp, body);

        assert!(matches!(
            verifier.verify_at(&header, b"{}", timestamp),
            Err(WaffoError::InvalidWebhookSignature)
        ));
        assert!(matches!(
            verifier.verify_at(&header, body, timestamp + 2_701_000),
            Err(WaffoError::WebhookTimestampOutsideTolerance)
        ));
        assert!(matches!(
            verifier.verify_at(&header, body, timestamp - 61_000),
            Err(WaffoError::WebhookTimestampOutsideTolerance)
        ));
    }

    #[test]
    fn verifier_rejects_cross_store_delivery_and_store_scopes_dedupe_keys() {
        let key_pair = KeyPair::generate(KeySize::Rsa2048).unwrap();
        let public_key = key_pair.public_key().as_ref().to_vec();
        let first = WaffoWebhookVerifier::from_der(
            WaffoEnvironment::Test,
            "STO_0123456789ABCDEFGHIJKL",
            &public_key,
            Duration::from_mins(45),
            Duration::from_mins(1),
            2048,
        )
        .unwrap();
        let second = WaffoWebhookVerifier::from_der(
            WaffoEnvironment::Test,
            "STO_ABCDEFGHIJKLMNOPQRSTUV",
            &public_key,
            Duration::from_mins(45),
            Duration::from_mins(1),
            2048,
        )
        .unwrap();
        let timestamp = 1_700_000_000_000_i64;
        let body = serde_json::to_vec(&json!({
            "id":"same-delivery",
            "timestamp":"2026-08-06T01:29:49.000Z",
            "eventType":"order.completed",
            "eventId":"same-event",
            "storeId":"STO_ABCDEFGHIJKLMNOPQRSTUV",
            "storeName":"Second Store",
            "mode":"test",
            "data":{}
        }))
        .unwrap();
        let header = signed_header(&key_pair, timestamp, &body);

        assert!(matches!(
            first.verify_at(&header, &body, timestamp),
            Err(WaffoError::WebhookStoreMismatch)
        ));
        let verified = second.verify_at(&header, &body, timestamp).unwrap();
        assert!(verified.delivery_dedupe_key.starts_with("waffo-delivery-"));
        assert!(verified.event_dedupe_key.starts_with("waffo-event-"));
    }
}
