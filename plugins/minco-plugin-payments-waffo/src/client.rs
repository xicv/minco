use crate::{
    CheckoutSession, CreateCheckoutSessionRequest, ReqwestWaffoTransport, SecretValue,
    WaffoApiError, WaffoConfiguration, WaffoEnvironment, WaffoError, WaffoResponse, WaffoTransport,
    WaffoTransportRequest, WaffoTransportResponse, WaffoWebhook,
    graphql::validate_read_only_graphql,
    signing::{RequestSigner, canonical_request},
};
use chrono::Utc;
use minco_plugin_idempotency::{
    BeginOutcome, IdempotencyError, IdempotencyKey, IdempotencyService, RequestFingerprint,
};
use reqwest::StatusCode;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fmt, sync::Arc};

pub const CHECKOUT_CREATE_SESSION_PATH: &str = "/v1/actions/checkout/create-session";
pub const ISSUE_SESSION_TOKEN_PATH: &str = "/v1/actions/auth/issue-session-token";
pub const ADD_WEBHOOK_PATH: &str = "/v1/actions/store/add-webhook";
pub const GRAPHQL_PATH: &str = "/v1/graphql";

/// Signed, server-side Waffo Pancake client. It performs no hidden retries.
#[derive(Clone)]
pub struct WaffoClient {
    configuration: Arc<WaffoConfiguration>,
    transport: Arc<dyn WaffoTransport>,
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
            .field("transport", &self.transport)
            .finish_non_exhaustive()
    }
}

impl WaffoClient {
    pub(super) fn new(
        configuration: Arc<WaffoConfiguration>,
        private_key: &SecretValue,
        idempotency: Arc<IdempotencyService>,
    ) -> Result<Self, WaffoError> {
        let transport = Arc::new(ReqwestWaffoTransport::new(configuration.request_timeout())?);
        Self::with_transport(configuration, private_key, idempotency, transport)
    }

    pub(super) fn with_transport(
        configuration: Arc<WaffoConfiguration>,
        private_key: &SecretValue,
        idempotency: Arc<IdempotencyService>,
        transport: Arc<dyn WaffoTransport>,
    ) -> Result<Self, WaffoError> {
        Ok(Self {
            configuration,
            transport,
            signer: RequestSigner::from_pem(private_key.expose())?,
            idempotency,
        })
    }

    #[cfg(test)]
    fn with_signer(
        configuration: Arc<WaffoConfiguration>,
        signer: RequestSigner,
        idempotency: Arc<IdempotencyService>,
        transport: Arc<dyn WaffoTransport>,
    ) -> Self {
        Self {
            configuration,
            transport,
            signer,
            idempotency,
        }
    }

    /// Execute a generic action in test mode only.
    ///
    /// Production callers must use a typed reviewed method. This prevents a
    /// configuration flag from turning an arbitrary string into a production
    /// mutation surface.
    pub async fn action_value(
        &self,
        path: &str,
        body: &Value,
        idempotency_key: &str,
    ) -> Result<WaffoResponse<Value>, WaffoError> {
        validate_action_path(path)?;
        validate_idempotency_key(idempotency_key)?;
        validate_request_body_size(body, self.configuration.request_max_bytes())?;
        if self.configuration.environment() == WaffoEnvironment::Production {
            return Err(WaffoError::GenericProductionActionDisabled);
        }
        self.execute_action(path, body, idempotency_key).await
    }

    async fn execute_action(
        &self,
        path: &str,
        body: &Value,
        idempotency_key: &str,
    ) -> Result<WaffoResponse<Value>, WaffoError> {
        validate_action_path(path)?;
        validate_idempotency_key(idempotency_key)?;
        self.ensure_write_allowed()?;
        validate_request_body_size(body, self.configuration.request_max_bytes())?;

        let body_fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(body).map_err(WaffoError::RequestEncoding)?)
        );
        let scope = json!({
            "provider": "waffo",
            "providerEnvironment": self.configuration.environment().as_str(),
            "canonicalApiOrigin": canonical_api_origin(self.configuration.api_base_url()),
            "merchantId": self.configuration.merchant_id(),
            "providerIdempotencyKey": idempotency_key,
            "canonicalPath": path,
            "bodyFingerprint": body_fingerprint,
        });
        let fingerprint =
            RequestFingerprint::from_serializable(&scope).map_err(WaffoError::Idempotency)?;
        let key = local_idempotency_key(&scope)?;
        let contains_ephemeral_secret = path == ISSUE_SESSION_TOKEN_PATH;
        match self
            .idempotency
            .begin(key, fingerprint)
            .await
            .map_err(WaffoError::Idempotency)?
        {
            BeginOutcome::Replay(record) if !contains_ephemeral_secret => {
                serde_json::from_value(record.response).map_err(WaffoError::InvalidResponse)
            }
            BeginOutcome::Replay(_) => Err(WaffoError::SensitiveResponseReplayUnavailable),
            BeginOutcome::Conflict => Err(WaffoError::IdempotencyConflict),
            BeginOutcome::InProgress { .. } => Err(WaffoError::IdempotencyInProgress),
            BeginOutcome::Started(lease) => {
                match self.post_signed(path, body, Some(idempotency_key)).await {
                    Ok(response) => {
                        if contains_ephemeral_secret {
                            let released = self
                                .idempotency
                                .abort(&lease)
                                .await
                                .map_err(WaffoError::Idempotency)?;
                            if !released {
                                return Err(WaffoError::Idempotency(
                                    IdempotencyError::InvalidLease,
                                ));
                            }
                        } else {
                            let stored = serde_json::to_value(&response)
                                .map_err(WaffoError::RequestEncoding)?;
                            self.idempotency
                                .complete(lease, stored)
                                .await
                                .map_err(WaffoError::Idempotency)?;
                        }
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

    /// Create a hosted checkout session and retain all ordered provider warnings.
    pub async fn create_checkout_session(
        &self,
        request: &CreateCheckoutSessionRequest,
        idempotency_key: &str,
    ) -> Result<WaffoResponse<CheckoutSession>, WaffoError> {
        request.validate()?;
        let response = self
            .execute_action(
                CHECKOUT_CREATE_SESSION_PATH,
                &serde_json::to_value(request).map_err(WaffoError::RequestEncoding)?,
                idempotency_key,
            )
            .await?;
        decode_data(response)
    }

    /// Register the configured standard HTTP webhook.
    pub async fn add_configured_http_webhook(
        &self,
        idempotency_key: &str,
    ) -> Result<WaffoResponse<WaffoWebhook>, WaffoError> {
        let store_id = self
            .configuration
            .store_id()
            .ok_or(WaffoError::MissingWebhookRegistrationConfiguration)?;
        let url = self
            .configuration
            .webhook_url()
            .ok_or(WaffoError::MissingWebhookRegistrationConfiguration)?;
        if self.configuration.webhook_events().is_empty() {
            return Err(WaffoError::MissingWebhookRegistrationConfiguration);
        }
        let body = json!({
            "storeId": store_id,
            "channel": "http",
            "url": url.as_str(),
            "events": self.configuration.webhook_events(),
            "testMode": self.configuration.environment().webhook_test_mode(),
        });
        let response = self
            .execute_action(ADD_WEBHOOK_PATH, &body, idempotency_key)
            .await?;
        let decoded = decode_data::<AddWebhookResponse>(response)?;
        Ok(WaffoResponse {
            data: decoded.data.webhook,
            warnings: decoded.warnings,
        })
    }

    /// Execute a read-only GraphQL query using the same RSA request signature.
    pub async fn graphql_query(
        &self,
        query: &str,
        variables: Value,
    ) -> Result<WaffoResponse<Value>, WaffoError> {
        validate_read_only_graphql(query)?;
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

    pub(crate) async fn typed_action(
        &self,
        path: &'static str,
        body: &Value,
        idempotency_key: &str,
    ) -> Result<WaffoResponse<Value>, WaffoError> {
        self.execute_action(path, body, idempotency_key).await
    }

    async fn post_signed(
        &self,
        path: &str,
        body: &Value,
        idempotency_key: Option<&str>,
    ) -> Result<WaffoResponse<Value>, WaffoError> {
        if path != GRAPHQL_PATH {
            validate_action_path(path)?;
        }
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
        let request = WaffoTransportRequest {
            url,
            canonical_path: path.to_owned(),
            merchant_id: self.configuration.merchant_id().to_owned(),
            timestamp,
            signature,
            idempotency_key: idempotency_key.map(str::to_owned),
            body: encoded,
            response_max_bytes: self.configuration.response_max_bytes(),
        };
        let response = self.transport.send(request).await?;
        parse_response(response, self.configuration.response_max_bytes())
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
#[serde(rename_all = "camelCase")]
struct ApiEnvelope {
    #[serde(default)]
    data: Option<Value>,
    #[serde(default)]
    errors: Vec<WaffoApiError>,
    #[serde(default)]
    warnings: Vec<WaffoApiError>,
}

fn parse_response(
    response: WaffoTransportResponse,
    max_bytes: usize,
) -> Result<WaffoResponse<Value>, WaffoError> {
    if response
        .declared_content_length
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(WaffoError::ResponseTooLarge);
    }
    let mut bytes = Vec::new();
    for chunk in response.chunks {
        let next_length = bytes
            .len()
            .checked_add(chunk.len())
            .ok_or(WaffoError::ResponseTooLarge)?;
        if next_length > max_bytes {
            return Err(WaffoError::ResponseTooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        return Err(WaffoError::EmptyResponse);
    }
    let envelope =
        serde_json::from_slice::<ApiEnvelope>(&bytes).map_err(WaffoError::InvalidResponse)?;
    if !(200..300).contains(&response.status) || !envelope.errors.is_empty() {
        let mut errors = envelope.errors;
        if errors.is_empty() {
            errors.push(WaffoApiError::fallback(fallback_status_message(
                response.status,
            )));
        }
        return Err(WaffoError::Api {
            status: response.status,
            count: errors.len(),
            errors,
        });
    }
    Ok(WaffoResponse {
        data: envelope.data.unwrap_or(Value::Null),
        warnings: envelope.warnings,
    })
}

fn fallback_status_message(status: u16) -> String {
    StatusCode::from_u16(status)
        .ok()
        .and_then(|status| status.canonical_reason())
        .map_or_else(|| "provider request failed".into(), str::to_owned)
}

fn decode_data<T: DeserializeOwned>(
    response: WaffoResponse<Value>,
) -> Result<WaffoResponse<T>, WaffoError> {
    if response.data.is_null() {
        return Err(WaffoError::MissingResponseData);
    }
    Ok(WaffoResponse {
        data: serde_json::from_value(response.data).map_err(WaffoError::InvalidResponse)?,
        warnings: response.warnings,
    })
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

pub fn validate_action_path(path: &str) -> Result<(), WaffoError> {
    if !path.starts_with("/v1/actions/")
        || path.len() > 256
        || path.contains(['?', '#', '%', '\\'])
        || path.contains("//")
        || path
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte == b' ')
        || path.split('/').skip(1).any(|segment| {
            segment.is_empty()
                || matches!(segment, "." | "..")
                || !segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
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

fn canonical_api_origin(url: &url::Url) -> String {
    url.origin().ascii_serialization()
}

fn local_idempotency_key(scope: &Value) -> Result<IdempotencyKey, WaffoError> {
    let bytes = serde_json::to_vec(scope).map_err(WaffoError::RequestEncoding)?;
    IdempotencyKey::parse(format!("waffo-{:x}", Sha256::digest(bytes)))
        .map_err(WaffoError::Idempotency)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FakeWaffoTransport, RawWaffoConfiguration, signing::RequestSigner};
    use aws_lc_rs::{rsa, rsa::KeySize};
    use chrono::TimeDelta;
    use minco_plugin_idempotency::{IdempotencyService, MemoryIdempotencyStore};

    const MERCHANT_ID: &str = "MER_0123456789ABCDEFGHIJKL";

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
            "merchant_id": MERCHANT_ID,
            "private_key": "env:WAFFO_PRIVATE_KEY"
        }))
        .unwrap();
        Arc::new(WaffoConfiguration::try_from(raw).unwrap())
    }

    #[test]
    fn idempotency_keys_and_paths_are_strict() {
        assert!(validate_idempotency_key("order_2026-08-06_01").is_ok());
        assert!(validate_idempotency_key("contains.dot").is_err());
        assert!(validate_idempotency_key(&"a".repeat(257)).is_err());
        assert!(validate_action_path(CHECKOUT_CREATE_SESSION_PATH).is_ok());
        for path in [
            "/v1/actions/../graphql",
            "/v1/actions/%2e%2e/graphql",
            "/v1/actions//checkout",
            "/v1/actions/checkout?x=1",
            "https://evil.example/v1/actions/a",
        ] {
            assert!(validate_action_path(path).is_err(), "accepted {path}");
        }
    }

    #[test]
    fn full_scope_changes_local_idempotency_key() {
        let first = json!({"provider":"waffo","environment":"test","origin":"https://a","merchant":MERCHANT_ID,"key":"same","path":"/a","body":"1"});
        let second = json!({"provider":"waffo","environment":"test","origin":"https://b","merchant":MERCHANT_ID,"key":"same","path":"/a","body":"1"});
        assert_ne!(
            local_idempotency_key(&first).unwrap(),
            local_idempotency_key(&second).unwrap()
        );
    }

    #[test]
    fn response_preserves_ordered_errors_warnings_locations_and_paths() {
        let response = WaffoTransportResponse::json(422, &json!({
            "errors": [
                {"message":"outer","layer":"graphql","aiHint":"untrusted one","locations":[{"line":1,"column":2}],"path":["checkout"]},
                {"message":"inner","layer":"domain","aiHint":"untrusted two"}
            ],
            "warnings": [{"message":"warning"}]
        })).unwrap();
        let error = parse_response(response, 4096).unwrap_err();
        let WaffoError::Api { errors, count, .. } = error else {
            panic!("expected provider error");
        };
        assert_eq!(count, 2);
        assert_eq!(errors[0].message, "outer");
        assert_eq!(errors[1].message, "inner");
        assert_eq!(errors[0].locations[0].column, 2);
        assert_eq!(errors[0].path, ["checkout"]);
    }

    #[test]
    fn success_preserves_multiple_warnings_and_typed_data_requirements() {
        let response = WaffoTransportResponse::json(
            200,
            &json!({
                "data": {"value": 42},
                "warnings": [{"message":"first"},{"message":"second"}]
            }),
        )
        .unwrap();
        let parsed = parse_response(response, 4096).unwrap();
        assert_eq!(parsed.data["value"], 42);
        assert_eq!(
            parsed
                .warnings
                .iter()
                .map(|warning| warning.message.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );

        let data_only = parse_response(
            WaffoTransportResponse::json(200, &json!({"data":{"value":7}})).unwrap(),
            4096,
        )
        .unwrap();
        assert!(data_only.warnings.is_empty());
        assert!(matches!(
            decode_data::<Value>(WaffoResponse {
                data: Value::Null,
                warnings: vec![]
            }),
            Err(WaffoError::MissingResponseData)
        ));
    }

    #[test]
    fn http_error_without_provider_errors_gets_a_bounded_fallback() {
        let error = parse_response(
            WaffoTransportResponse::json(503, &json!({"data":null})).unwrap(),
            4096,
        )
        .unwrap_err();
        let WaffoError::Api { status, errors, .. } = error else {
            panic!("expected provider error");
        };
        assert_eq!(status, 503);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].message, "Service Unavailable");
    }

    #[test]
    fn response_distinguishes_empty_malformed_and_oversized_streams() {
        assert!(matches!(
            parse_response(
                WaffoTransportResponse {
                    status: 200,
                    declared_content_length: Some(0),
                    chunks: vec![]
                },
                16
            ),
            Err(WaffoError::EmptyResponse)
        ));
        assert!(matches!(
            parse_response(
                WaffoTransportResponse {
                    status: 200,
                    declared_content_length: None,
                    chunks: vec![b"{".to_vec()]
                },
                16
            ),
            Err(WaffoError::InvalidResponse(_))
        ));
        assert!(matches!(
            parse_response(
                WaffoTransportResponse {
                    status: 200,
                    declared_content_length: None,
                    chunks: vec![vec![b'a'; 12], vec![b'b'; 12]]
                },
                16
            ),
            Err(WaffoError::ResponseTooLarge)
        ));
        assert!(matches!(
            parse_response(
                WaffoTransportResponse {
                    status: 200,
                    declared_content_length: Some(17),
                    chunks: vec![],
                },
                16,
            ),
            Err(WaffoError::ResponseTooLarge)
        ));
    }

    #[tokio::test]
    async fn fake_transport_preserves_provider_key_and_replays_full_response() {
        let fake = Arc::new(FakeWaffoTransport::default());
        fake.enqueue(
            CHECKOUT_CREATE_SESSION_PATH,
            WaffoTransportResponse::json(
                200,
                &json!({
                    "data": {"ok": true},
                    "warnings": [{"message":"keep me","path":["data"]}]
                }),
            ),
        );
        let signer =
            RequestSigner::from_key_pair(rsa::KeyPair::generate(KeySize::Rsa2048).unwrap());
        let client = WaffoClient::with_signer(configuration(), signer, idempotency(), fake.clone());
        let first = client
            .action_value(CHECKOUT_CREATE_SESSION_PATH, &json!({}), "caller_key")
            .await
            .unwrap();
        let replay = client
            .action_value(CHECKOUT_CREATE_SESSION_PATH, &json!({}), "caller_key")
            .await
            .unwrap();
        assert_eq!(first, replay);
        assert_eq!(first.warnings[0].message, "keep me");
        assert_eq!(fake.captured().len(), 1);
        assert_eq!(
            fake.captured()[0].idempotency_key.as_deref(),
            Some("caller_key")
        );
    }

    #[tokio::test]
    async fn authenticated_checkout_uses_official_token_action_and_redacts_secrets() {
        let fake = Arc::new(FakeWaffoTransport::default());
        fake.enqueue(
            ISSUE_SESSION_TOKEN_PATH,
            WaffoTransportResponse::json(
                200,
                &json!({
                    "data": {"token":"secret.jwt.value","expiresAt":"2026-08-09T01:05:00Z"},
                    "warnings": [{"message":"token warning"}]
                }),
            ),
        );
        fake.enqueue(
            CHECKOUT_CREATE_SESSION_PATH,
            WaffoTransportResponse::json(
                200,
                &json!({
                    "data": {
                        "sessionId":"session-1",
                        "checkoutUrl":"https://checkout.waffo.ai/session-1?theme=dark",
                        "expiresAt":"2026-08-09T01:30:00Z"
                    },
                    "warnings": [{"message":"session warning"}]
                }),
            ),
        );
        let client = WaffoClient::with_signer(
            configuration(),
            RequestSigner::from_key_pair(rsa::KeyPair::generate(KeySize::Rsa2048).unwrap()),
            idempotency(),
            fake.clone(),
        );
        let keys = crate::AuthenticatedCheckoutIdempotencyKeys {
            issue_session_token: "token_key".into(),
            create_checkout_session: "checkout_key".into(),
        };

        let response =
            crate::Checkout::authenticated("PROD_0123456789ABCDEFGHIJKL", "AUD", "buyer-42")
                .buyer_email("buyer@example.com")
                .create(&client, &keys)
                .await
                .unwrap();

        assert_eq!(
            response
                .warnings
                .iter()
                .map(|warning| warning.message.as_str())
                .collect::<Vec<_>>(),
            ["token warning", "session warning"]
        );
        assert!(
            response
                .data
                .expose_sensitive_checkout_url()
                .contains("?theme=dark#token=secret.jwt.value")
        );
        let debug = format!("{:?}", response.data);
        assert!(!debug.contains("secret.jwt.value"));
        assert!(debug.contains("[REDACTED]"));
        let captured = fake.captured();
        assert_eq!(captured.len(), 2);
        assert_eq!(captured[0].canonical_path, ISSUE_SESSION_TOKEN_PATH);
        assert_eq!(captured[1].canonical_path, CHECKOUT_CREATE_SESSION_PATH);
        assert_eq!(captured[0].idempotency_key.as_deref(), Some("token_key"));
        assert_eq!(captured[1].idempotency_key.as_deref(), Some("checkout_key"));
        assert!(String::from_utf8_lossy(&captured[0].body).contains("buyerIdentity"));
        assert!(!String::from_utf8_lossy(&captured[1].body).contains("buyerIdentity"));
    }

    #[tokio::test]
    async fn issue_session_token_never_persists_the_bearer_token() {
        let fake = Arc::new(FakeWaffoTransport::default());
        fake.enqueue(
            ISSUE_SESSION_TOKEN_PATH,
            WaffoTransportResponse::json(
                200,
                &json!({
                    "data": {"token":"must-not-be-stored","expiresAt":"2026-08-09T01:05:00Z"}
                }),
            ),
        );
        let idempotency = idempotency();
        let client = WaffoClient::with_signer(
            configuration(),
            RequestSigner::from_key_pair(rsa::KeyPair::generate(KeySize::Rsa2048).unwrap()),
            Arc::clone(&idempotency),
            fake,
        );
        let request =
            crate::IssueSessionTokenRequest::for_product("PROD_0123456789ABCDEFGHIJKL", "buyer-42");
        let body = serde_json::to_value(&request).unwrap();

        let response = client
            .issue_session_token(&request, "sensitive_token_key")
            .await
            .unwrap();
        assert_eq!(response.data.expose_sensitive(), "must-not-be-stored");

        let body_fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&body).unwrap()));
        let scope = json!({
            "provider": "waffo",
            "providerEnvironment": client.configuration.environment().as_str(),
            "canonicalApiOrigin": canonical_api_origin(client.configuration.api_base_url()),
            "merchantId": client.configuration.merchant_id(),
            "providerIdempotencyKey": "sensitive_token_key",
            "canonicalPath": ISSUE_SESSION_TOKEN_PATH,
            "bodyFingerprint": body_fingerprint,
        });
        let key = local_idempotency_key(&scope).unwrap();
        assert!(
            idempotency.get(&key).await.unwrap().is_none(),
            "short-lived session bearer tokens must never enter generic idempotency persistence"
        );
    }

    #[tokio::test]
    async fn issue_session_token_retry_returns_to_the_provider_without_local_replay() {
        let fake = Arc::new(FakeWaffoTransport::default());
        for token in ["first-secret", "second-secret"] {
            fake.enqueue(
                ISSUE_SESSION_TOKEN_PATH,
                WaffoTransportResponse::json(
                    200,
                    &json!({
                        "data": {"token":token,"expiresAt":"2026-08-09T01:05:00Z"}
                    }),
                ),
            );
        }
        let client = WaffoClient::with_signer(
            configuration(),
            RequestSigner::from_key_pair(rsa::KeyPair::generate(KeySize::Rsa2048).unwrap()),
            idempotency(),
            fake.clone(),
        );
        let request =
            crate::IssueSessionTokenRequest::for_product("PROD_0123456789ABCDEFGHIJKL", "buyer-42");

        let first = client
            .issue_session_token(&request, "same_provider_key")
            .await
            .unwrap();
        let second = client
            .issue_session_token(&request, "same_provider_key")
            .await
            .unwrap();

        assert_eq!(first.data.expose_sensitive(), "first-secret");
        assert_eq!(second.data.expose_sensitive(), "second-secret");
        let captured = fake.captured();
        assert_eq!(captured.len(), 2);
        assert!(
            captured
                .iter()
                .all(|request| request.idempotency_key.as_deref() == Some("same_provider_key"))
        );
    }

    #[tokio::test]
    async fn issue_session_token_fails_closed_for_a_preexisting_sensitive_replay() {
        let idempotency = idempotency();
        let client = WaffoClient::with_signer(
            configuration(),
            RequestSigner::from_key_pair(rsa::KeyPair::generate(KeySize::Rsa2048).unwrap()),
            Arc::clone(&idempotency),
            Arc::new(FakeWaffoTransport::default()),
        );
        let request =
            crate::IssueSessionTokenRequest::for_product("PROD_0123456789ABCDEFGHIJKL", "buyer-42");
        let body = serde_json::to_value(&request).unwrap();
        let body_fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&body).unwrap()));
        let scope = json!({
            "provider": "waffo",
            "providerEnvironment": client.configuration.environment().as_str(),
            "canonicalApiOrigin": canonical_api_origin(client.configuration.api_base_url()),
            "merchantId": client.configuration.merchant_id(),
            "providerIdempotencyKey": "legacy_sensitive_key",
            "canonicalPath": ISSUE_SESSION_TOKEN_PATH,
            "bodyFingerprint": body_fingerprint,
        });
        let fingerprint = RequestFingerprint::from_serializable(&scope).unwrap();
        let key = local_idempotency_key(&scope).unwrap();
        let BeginOutcome::Started(lease) = idempotency.begin(key, fingerprint).await.unwrap()
        else {
            panic!("expected fresh legacy claim");
        };
        idempotency
            .complete(
                lease,
                json!({
                    "data": {"token":"legacy-secret","expiresAt":"2026-08-09T01:05:00Z"},
                    "warnings": []
                }),
            )
            .await
            .unwrap();

        let error = client
            .issue_session_token(&request, "legacy_sensitive_key")
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            WaffoError::SensitiveResponseReplayUnavailable
        ));
        assert_eq!(error.code(), "waffo.sensitive_response_replay_unavailable");
    }

    #[tokio::test]
    async fn authenticated_checkout_rejects_an_unsafe_provider_checkout_url() {
        let fake = Arc::new(FakeWaffoTransport::default());
        fake.enqueue(
            ISSUE_SESSION_TOKEN_PATH,
            WaffoTransportResponse::json(
                200,
                &json!({
                    "data": {"token":"secret.jwt.value","expiresAt":"2026-08-09T01:05:00Z"}
                }),
            ),
        );
        fake.enqueue(
            CHECKOUT_CREATE_SESSION_PATH,
            WaffoTransportResponse::json(
                200,
                &json!({
                    "data": {
                        "sessionId":"session-1",
                        "checkoutUrl":"javascript:alert(document.domain)",
                        "expiresAt":"2026-08-09T01:30:00Z"
                    }
                }),
            ),
        );
        let client = WaffoClient::with_signer(
            configuration(),
            RequestSigner::from_key_pair(rsa::KeyPair::generate(KeySize::Rsa2048).unwrap()),
            idempotency(),
            fake,
        );
        let keys = crate::AuthenticatedCheckoutIdempotencyKeys {
            issue_session_token: "unsafe_url_token_key".into(),
            create_checkout_session: "unsafe_url_checkout_key".into(),
        };

        let error =
            crate::Checkout::authenticated("PROD_0123456789ABCDEFGHIJKL", "AUD", "buyer-42")
                .create(&client, &keys)
                .await
                .unwrap_err();

        assert!(matches!(
            error,
            WaffoError::InvalidConfiguration(
                "provider checkout_url must be an absolute HTTPS URL without credentials or a fragment"
            )
        ));
    }

    #[test]
    fn debug_output_never_contains_private_material() {
        let signer =
            RequestSigner::from_key_pair(rsa::KeyPair::generate(KeySize::Rsa2048).unwrap());
        let client = WaffoClient::with_signer(
            configuration(),
            signer,
            idempotency(),
            Arc::new(FakeWaffoTransport::default()),
        );
        let rendered = format!("{client:?}");
        assert!(rendered.contains("RSA-PKCS1-SHA256"));
        assert!(!rendered.contains("PRIVATE KEY"));
    }
}
