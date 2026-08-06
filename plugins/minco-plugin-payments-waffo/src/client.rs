use crate::{
    CheckoutSession, CreateCheckoutSessionRequest, SecretValue, WaffoApiError, WaffoConfiguration,
    WaffoEnvironment, WaffoError, WaffoWebhook,
    graphql::validate_graphql_query,
    signing::{RequestSigner, canonical_request},
};
use chrono::Utc;
use minco_plugin_idempotency::{
    BeginOutcome, IdempotencyKey, IdempotencyService, RequestFingerprint,
};
use reqwest::{StatusCode, header};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fmt, sync::Arc};

pub const CHECKOUT_CREATE_SESSION_PATH: &str = "/v1/actions/checkout/create-session";
pub const ADD_WEBHOOK_PATH: &str = "/v1/actions/store/add-webhook";
pub const GRAPHQL_PATH: &str = "/v1/graphql";

/// Signed, server-side Waffo Pancake client. It performs no hidden retries.
#[derive(Clone)]
pub struct WaffoClient {
    configuration: Arc<WaffoConfiguration>,
    http: reqwest::Client,
    signer: RequestSigner,
    idempotency: Arc<IdempotencyService>,
}

impl fmt::Debug for WaffoClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WaffoClient")
            .field("environment", &self.configuration.environment())
            .field("api_base_url", &self.configuration.api_base_url())
            .field("signer", &self.signer)
            .finish_non_exhaustive()
    }
}

impl WaffoClient {
    pub(super) fn new(
        configuration: Arc<WaffoConfiguration>,
        private_key: &SecretValue,
        idempotency: Arc<IdempotencyService>,
    ) -> Result<Self, WaffoError> {
        let signer = RequestSigner::from_pem(private_key.expose())?;
        let http = reqwest::Client::builder()
            .https_only(true)
            .timeout(configuration.request_timeout())
            .user_agent(concat!(
                "minco-plugin-payments-waffo/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .map_err(WaffoError::Transport)?;
        Ok(Self {
            configuration,
            http,
            signer,
            idempotency,
        })
    }

    #[cfg(test)]
    fn with_signer(
        configuration: Arc<WaffoConfiguration>,
        signer: RequestSigner,
        idempotency: Arc<IdempotencyService>,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            configuration,
            signer,
            idempotency,
        }
    }

    /// Execute any documented Waffo action with an explicit idempotency key.
    pub async fn action_value(
        &self,
        path: &str,
        body: &Value,
        idempotency_key: &str,
    ) -> Result<Value, WaffoError> {
        validate_action_path(path)?;
        validate_idempotency_key(idempotency_key)?;
        self.ensure_write_allowed()?;
        validate_request_body_size(body, self.configuration.request_max_bytes())?;

        let request = json!({
            "provider": "waffo",
            "merchantId": self.configuration.merchant_id(),
            "path": path,
            "body": body,
        });
        let fingerprint =
            RequestFingerprint::from_serializable(&request).map_err(WaffoError::Idempotency)?;
        let key = local_idempotency_key(self.configuration.merchant_id(), idempotency_key)?;
        match self
            .idempotency
            .begin(key, fingerprint)
            .await
            .map_err(WaffoError::Idempotency)?
        {
            BeginOutcome::Replay(record) => Ok(record.response),
            BeginOutcome::Conflict => Err(WaffoError::IdempotencyConflict),
            BeginOutcome::InProgress { .. } => Err(WaffoError::IdempotencyInProgress),
            BeginOutcome::Started(lease) => {
                match self.post_signed(path, body, Some(idempotency_key)).await {
                    Ok(response) => {
                        self.idempotency
                            .complete(lease, response.clone())
                            .await
                            .map_err(WaffoError::Idempotency)?;
                        Ok(response)
                    }
                    Err(error) => {
                        self.idempotency
                            .abort(&lease)
                            .await
                            .map_err(WaffoError::Idempotency)?;
                        Err(error)
                    }
                }
            }
        }
    }

    /// Create a hosted checkout session.
    pub async fn create_checkout_session(
        &self,
        request: &CreateCheckoutSessionRequest,
        idempotency_key: &str,
    ) -> Result<CheckoutSession, WaffoError> {
        request.validate()?;
        let data = self
            .action_value(
                CHECKOUT_CREATE_SESSION_PATH,
                &serde_json::to_value(request).map_err(WaffoError::RequestEncoding)?,
                idempotency_key,
            )
            .await?;
        decode_data(data)
    }

    /// Register the configured standard HTTP webhook.
    pub async fn add_configured_http_webhook(
        &self,
        idempotency_key: &str,
    ) -> Result<WaffoWebhook, WaffoError> {
        let store_id = self
            .configuration
            .store_id()
            .ok_or(WaffoError::MissingWebhookConfiguration)?;
        let url = self
            .configuration
            .webhook_url()
            .ok_or(WaffoError::MissingWebhookConfiguration)?;
        if self.configuration.webhook_events().is_empty() {
            return Err(WaffoError::MissingWebhookConfiguration);
        }
        let body = json!({
            "storeId": store_id,
            "channel": "http",
            "url": url.as_str(),
            "events": self.configuration.webhook_events(),
            "testMode": self.configuration.environment().webhook_test_mode(),
        });
        let data = self
            .action_value(ADD_WEBHOOK_PATH, &body, idempotency_key)
            .await?;
        Ok(decode_data::<AddWebhookResponse>(data)?.webhook)
    }

    /// Execute a read-only GraphQL query using the same RSA request signature.
    pub async fn graphql_query(&self, query: &str, variables: Value) -> Result<Value, WaffoError> {
        validate_graphql_query(query)?;
        if !variables.is_object() {
            return Err(WaffoError::InvalidConfiguration(
                "GraphQL variables must be a JSON object",
            ));
        }
        self.post_signed(
            GRAPHQL_PATH,
            &json!({ "query": query, "variables": variables }),
            None,
        )
        .await
    }

    async fn post_signed(
        &self,
        path: &str,
        body: &Value,
        idempotency_key: Option<&str>,
    ) -> Result<Value, WaffoError> {
        let encoded = serde_json::to_vec(body).map_err(WaffoError::RequestEncoding)?;
        if encoded.len() > self.configuration.request_max_bytes() {
            return Err(WaffoError::RequestBodyTooLarge);
        }
        let timestamp = Utc::now().timestamp();
        let canonical = canonical_request("POST", path, timestamp, &encoded);
        let signature = self.signer.sign(canonical.as_bytes())?;
        let url = self
            .configuration
            .api_base_url()
            .join(path.trim_start_matches('/'))
            .map_err(|_| WaffoError::InvalidActionPath)?;
        let mut request = self
            .http
            .post(url)
            .header(header::CONTENT_TYPE, "application/json")
            .header("X-Merchant-Id", self.configuration.merchant_id())
            .header("X-Timestamp", timestamp.to_string())
            .header("X-Signature", signature)
            .body(encoded);
        if let Some(key) = idempotency_key {
            request = request.header("X-Idempotency-Key", key);
        }
        let response = request.send().await.map_err(WaffoError::Transport)?;
        parse_response(response, self.configuration.response_max_bytes()).await
    }

    fn ensure_write_allowed(&self) -> Result<(), WaffoError> {
        if self.configuration.environment() == WaffoEnvironment::Production
            && !self.configuration.production_writes_allowed()
        {
            Err(WaffoError::ProductionWritesDisabled)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Deserialize)]
struct AddWebhookResponse {
    webhook: WaffoWebhook,
}

#[derive(Debug, Deserialize)]
struct ApiEnvelope {
    #[serde(default)]
    data: Option<Value>,
    #[serde(default)]
    errors: Vec<WaffoApiError>,
}

async fn parse_response(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> Result<Value, WaffoError> {
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(WaffoError::ResponseTooLarge);
    }
    let initial_capacity = response
        .content_length()
        .and_then(|length| usize::try_from(length).ok())
        .unwrap_or_default()
        .min(max_bytes);
    let mut bytes = Vec::with_capacity(initial_capacity);
    while let Some(chunk) = response.chunk().await.map_err(WaffoError::Transport)? {
        let next_length = bytes
            .len()
            .checked_add(chunk.len())
            .ok_or(WaffoError::ResponseTooLarge)?;
        if next_length > max_bytes {
            return Err(WaffoError::ResponseTooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    let envelope =
        serde_json::from_slice::<ApiEnvelope>(&bytes).map_err(WaffoError::InvalidResponse)?;
    if !status.is_success() || !envelope.errors.is_empty() {
        let first = envelope
            .errors
            .into_iter()
            .next()
            .unwrap_or_else(|| WaffoApiError {
                message: fallback_status_message(status),
                layer: None,
            });
        return Err(WaffoError::Api {
            status: status.as_u16(),
            message: first.message,
            layer: first.layer,
        });
    }
    Ok(envelope.data.unwrap_or(Value::Null))
}

fn fallback_status_message(status: StatusCode) -> String {
    status
        .canonical_reason()
        .map_or_else(|| "provider request failed".into(), str::to_owned)
}

fn decode_data<T: DeserializeOwned>(data: Value) -> Result<T, WaffoError> {
    if data.is_null() {
        return Err(WaffoError::MissingResponseData);
    }
    serde_json::from_value(data).map_err(WaffoError::InvalidResponse)
}

pub fn validate_idempotency_key(value: &str) -> Result<(), WaffoError> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(WaffoError::InvalidIdempotencyKey);
    }
    Ok(())
}

fn validate_action_path(path: &str) -> Result<(), WaffoError> {
    if !path.starts_with("/v1/actions/")
        || path.len() > 256
        || path.contains('?')
        || path.contains('#')
        || path.contains("//")
        || path
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte == b' ')
    {
        return Err(WaffoError::InvalidActionPath);
    }
    Ok(())
}

fn validate_request_body_size(body: &Value, max_bytes: usize) -> Result<(), WaffoError> {
    let mut sink = BoundedWriter::new(max_bytes);
    serde_json::to_writer(&mut sink, body).map_err(|error| {
        if sink.exceeded() {
            WaffoError::RequestBodyTooLarge
        } else {
            WaffoError::RequestEncoding(error)
        }
    })
}

#[derive(Debug)]
struct BoundedWriter {
    remaining: usize,
    exceeded: bool,
}

impl BoundedWriter {
    const fn new(max_bytes: usize) -> Self {
        Self {
            remaining: max_bytes,
            exceeded: false,
        }
    }

    const fn exceeded(&self) -> bool {
        self.exceeded
    }
}

impl std::io::Write for BoundedWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if buffer.len() > self.remaining {
            self.exceeded = true;
            return Err(std::io::Error::other(
                "request body exceeds configured bound",
            ));
        }
        self.remaining -= buffer.len();
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn local_idempotency_key(
    merchant_id: &str,
    provider_key: &str,
) -> Result<IdempotencyKey, WaffoError> {
    let mut digest = Sha256::new();
    digest.update(merchant_id.as_bytes());
    digest.update([0]);
    digest.update(provider_key.as_bytes());
    IdempotencyKey::parse(format!("waffo-{:x}", digest.finalize())).map_err(WaffoError::Idempotency)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RawWaffoConfiguration, signing::RequestSigner};
    use aws_lc_rs::{rsa, rsa::KeySize};
    use chrono::TimeDelta;
    use minco_plugin_idempotency::{IdempotencyService, MemoryIdempotencyStore};

    fn idempotency() -> Arc<IdempotencyService> {
        Arc::new(
            IdempotencyService::new(
                Arc::new(MemoryIdempotencyStore::default()),
                TimeDelta::minutes(5),
            )
            .unwrap(),
        )
    }

    fn configuration() -> Arc<WaffoConfiguration> {
        let raw = serde_json::from_value::<RawWaffoConfiguration>(json!({
            "merchant_id": "MER_ABC123",
            "private_key": "env:WAFFO_PRIVATE_KEY"
        }))
        .unwrap();
        Arc::new(WaffoConfiguration::try_from(raw).unwrap())
    }

    #[test]
    fn idempotency_keys_are_bounded_to_provider_contract() {
        assert!(validate_idempotency_key("order_2026-08-06_01").is_ok());
        assert!(validate_idempotency_key("contains.dot").is_err());
        assert!(validate_idempotency_key(&"a".repeat(257)).is_err());
    }

    #[test]
    fn local_idempotency_key_is_bounded_and_merchant_scoped() {
        let first = local_idempotency_key("MER_ONE", "same-key").unwrap();
        let second = local_idempotency_key("MER_TWO", "same-key").unwrap();
        assert_ne!(first, second);
        assert!(first.as_str().starts_with("waffo-"));
    }

    #[test]
    fn production_actions_fail_before_network_access() {
        let raw = serde_json::from_value::<RawWaffoConfiguration>(json!({
            "environment": "production",
            "merchant_id": "MER_ABC123",
            "private_key": "env:WAFFO_PRIVATE_KEY"
        }))
        .unwrap();
        let configuration = Arc::new(WaffoConfiguration::try_from(raw).unwrap());
        let signer =
            RequestSigner::from_key_pair(rsa::KeyPair::generate(KeySize::Rsa2048).unwrap());
        let client = WaffoClient::with_signer(configuration, signer, idempotency());

        assert!(matches!(
            client.ensure_write_allowed(),
            Err(WaffoError::ProductionWritesDisabled)
        ));
    }

    #[test]
    fn debug_output_never_contains_private_material() {
        let signer =
            RequestSigner::from_key_pair(rsa::KeyPair::generate(KeySize::Rsa2048).unwrap());
        let client = WaffoClient::with_signer(configuration(), signer, idempotency());
        let rendered = format!("{client:?}");

        assert!(rendered.contains("RSA-PKCS1-SHA256"));
        assert!(!rendered.contains("PRIVATE KEY"));
    }
}
