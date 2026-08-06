use crate::{WaffoEnvironment, WaffoError, signing::decode_pem};
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
    public_key_der: Arc<Vec<u8>>,
    tolerance: Duration,
    max_body_bytes: usize,
}

impl fmt::Debug for WaffoWebhookVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WaffoWebhookVerifier")
            .field("environment", &self.environment)
            .field("algorithm", &"RSA-PKCS1-SHA256")
            .field("tolerance", &self.tolerance)
            .field("max_body_bytes", &self.max_body_bytes)
            .finish_non_exhaustive()
    }
}

impl WaffoWebhookVerifier {
    pub(super) fn from_pem(
        environment: WaffoEnvironment,
        public_key: &str,
        tolerance: Duration,
        max_body_bytes: usize,
    ) -> Result<Self, WaffoError> {
        let (_, der) = decode_pem(public_key, &["PUBLIC KEY", "RSA PUBLIC KEY"])
            .map_err(|()| WaffoError::InvalidPublicKey)?;
        Self::from_der(environment, &der, tolerance, max_body_bytes)
    }

    fn from_der(
        environment: WaffoEnvironment,
        public_key: &[u8],
        tolerance: Duration,
        max_body_bytes: usize,
    ) -> Result<Self, WaffoError> {
        let public_key =
            rsa::PublicKey::from_der(public_key).map_err(|_| WaffoError::InvalidPublicKey)?;
        Ok(Self {
            environment,
            public_key_der: Arc::new(public_key.as_ref().to_vec()),
            tolerance,
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
        let tolerance_milliseconds = u64::try_from(self.tolerance.as_millis()).unwrap_or(u64::MAX);
        if timestamp.abs_diff(now_milliseconds) > tolerance_milliseconds {
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

        Ok(VerifiedWaffoWebhook {
            timestamp_milliseconds: timestamp,
            delivery_dedupe_key: digest_key("waffo-delivery", &[event.id.as_bytes()]),
            event_dedupe_key: digest_key(
                "waffo-event",
                &[event.event_type.as_bytes(), event.event_id.as_bytes()],
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
            &public_key,
            Duration::from_mins(5),
            1024,
        )
        .unwrap();
        let timestamp = 1_700_000_000_000_i64;
        let body = serde_json::to_vec(&json!({
            "id": "delivery-1",
            "timestamp": "2026-08-06T01:29:49.000Z",
            "eventType": "order.completed",
            "eventId": "ORD_ABC123",
            "storeId": "STO_ABC123",
            "storeName": "Test Store",
            "mode": "test",
            "data": { "status": "completed" }
        }))
        .unwrap();
        let header = signed_header(&key_pair, timestamp, &body);

        let verified_event = verifier.verify_at(&header, &body, timestamp).unwrap();

        assert!(verified_event.delivery_dedupe_key.starts_with("waffo-delivery-"));
        assert!(verified_event.event_dedupe_key.starts_with("waffo-event-"));
        assert_ne!(verified_event.delivery_dedupe_key, verified_event.event_dedupe_key);
    }

    #[test]
    fn rejects_tampering_and_stale_delivery() {
        let key_pair = KeyPair::generate(KeySize::Rsa2048).unwrap();
        let public_key = key_pair.public_key().as_ref().to_vec();
        let verifier = WaffoWebhookVerifier::from_der(
            WaffoEnvironment::Test,
            &public_key,
            Duration::from_mins(5),
            1024,
        )
        .unwrap();
        let timestamp = 1_700_000_000_000_i64;
        let body = br#"{"id":"a","timestamp":"2026-08-06T01:29:49.000Z","eventType":"order.completed","eventId":"b","storeId":"c","storeName":"Test Store","mode":"test","data":{}}"#;
        let header = signed_header(&key_pair, timestamp, body);

        assert!(matches!(
            verifier.verify_at(&header, b"{}", timestamp),
            Err(WaffoError::InvalidWebhookSignature)
        ));
        assert!(matches!(
            verifier.verify_at(&header, body, timestamp + 301_000),
            Err(WaffoError::WebhookTimestampOutsideTolerance)
        ));
    }
}
