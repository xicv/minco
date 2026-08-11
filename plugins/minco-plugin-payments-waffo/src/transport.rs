use crate::{WaffoError, WaffoTransportFailure};
use async_trait::async_trait;
use reqwest::header;
use serde::Serialize;
use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
    sync::Mutex,
    time::Duration,
};
use url::Url;

/// One fully prepared provider request. Diagnostics deliberately omit headers and body.
pub struct WaffoTransportRequest {
    pub(crate) url: Url,
    pub(crate) canonical_path: String,
    pub(crate) merchant_id: String,
    pub(crate) timestamp: i64,
    pub(crate) signature: String,
    pub(crate) idempotency_key: Option<String>,
    pub(crate) body: Vec<u8>,
    pub(crate) response_max_bytes: usize,
}

impl fmt::Debug for WaffoTransportRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WaffoTransportRequest")
            .field("origin", &self.url.origin().ascii_serialization())
            .field("canonical_path", &self.canonical_path)
            .field("merchant_id", &self.merchant_id)
            .field("timestamp", &self.timestamp)
            .field("signature", &"[REDACTED]")
            .field(
                "idempotency_key_configured",
                &self.idempotency_key.is_some(),
            )
            .field("body_bytes", &self.body.len())
            .field("response_max_bytes", &self.response_max_bytes)
            .finish()
    }
}

impl WaffoTransportRequest {
    /// Exact HTTPS endpoint selected by the validated client configuration.
    pub const fn url(&self) -> &Url {
        &self.url
    }

    pub fn canonical_path(&self) -> &str {
        &self.canonical_path
    }

    pub fn merchant_id(&self) -> &str {
        &self.merchant_id
    }

    /// Unix timestamp included in the canonical request and provider header.
    pub const fn timestamp(&self) -> i64 {
        self.timestamp
    }

    /// RSA signature for the exact prepared request.
    ///
    /// A custom transport needs this value for the provider header, but must
    /// not include it in logs, errors, metrics or retained request evidence.
    pub fn signature(&self) -> &str {
        &self.signature
    }

    pub fn idempotency_key(&self) -> Option<&str> {
        self.idempotency_key.as_deref()
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// Maximum response bytes the transport may retain for this request.
    pub const fn response_max_bytes(&self) -> usize {
        self.response_max_bytes
    }
}

/// A bounded provider response represented as chunks so tests cover stream limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaffoTransportResponse {
    pub status: u16,
    pub declared_content_length: Option<u64>,
    pub chunks: Vec<Vec<u8>>,
}

impl WaffoTransportResponse {
    pub fn json(status: u16, value: &impl Serialize) -> Result<Self, WaffoError> {
        let body = serde_json::to_vec(value).map_err(WaffoError::RequestEncoding)?;
        Ok(Self {
            status,
            declared_content_length: Some(body.len() as u64),
            chunks: vec![body],
        })
    }
}

/// Injectable network boundary. Minco installs no process-global transport.
#[async_trait]
pub trait WaffoTransport: Send + Sync + fmt::Debug {
    async fn send(
        &self,
        request: WaffoTransportRequest,
    ) -> Result<WaffoTransportResponse, WaffoError>;
}

/// HTTPS transport used only by explicitly constructed production clients.
#[derive(Debug, Clone)]
pub struct ReqwestWaffoTransport {
    client: reqwest::Client,
}

impl ReqwestWaffoTransport {
    pub(crate) fn new(timeout: Duration) -> Result<Self, WaffoError> {
        let client = reqwest::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(timeout)
            .user_agent(concat!(
                "minco-plugin-payments-waffo/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .map_err(|_| WaffoError::Transport(WaffoTransportFailure::Build))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl WaffoTransport for ReqwestWaffoTransport {
    async fn send(
        &self,
        request: WaffoTransportRequest,
    ) -> Result<WaffoTransportResponse, WaffoError> {
        let mut builder = self
            .client
            .post(request.url)
            .header(header::CONTENT_TYPE, "application/json")
            .header("X-Merchant-Id", request.merchant_id)
            .header("X-Timestamp", request.timestamp.to_string())
            .header("X-Signature", request.signature)
            .body(request.body);
        if let Some(key) = request.idempotency_key {
            builder = builder.header("X-Idempotency-Key", key);
        }
        let mut response = builder.send().await.map_err(|error| {
            WaffoError::Transport(if error.is_timeout() {
                WaffoTransportFailure::Timeout
            } else {
                WaffoTransportFailure::Connection
            })
        })?;
        let status = response.status().as_u16();
        let declared_content_length = response.content_length();
        if declared_content_length.is_some_and(|length| length > request.response_max_bytes as u64)
        {
            return Err(WaffoError::ResponseTooLarge);
        }
        let mut chunks = Vec::new();
        let mut received = 0_usize;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| WaffoError::Transport(WaffoTransportFailure::ResponseBody))?
        {
            received = received
                .checked_add(chunk.len())
                .ok_or(WaffoError::ResponseTooLarge)?;
            if received > request.response_max_bytes {
                return Err(WaffoError::ResponseTooLarge);
            }
            chunks.push(chunk.to_vec());
        }
        Ok(WaffoTransportResponse {
            status,
            declared_content_length,
            chunks,
        })
    }
}

/// Safe request evidence retained by [`FakeWaffoTransport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedWaffoRequest {
    pub canonical_path: String,
    pub merchant_id: String,
    pub idempotency_key: Option<String>,
    pub body: Vec<u8>,
}

/// Deterministic no-network fake with endpoint-specific queued outcomes.
#[derive(Debug, Default)]
pub struct FakeWaffoTransport {
    queued: Mutex<BTreeMap<String, VecDeque<Result<WaffoTransportResponse, WaffoError>>>>,
    captured: Mutex<Vec<CapturedWaffoRequest>>,
}

impl FakeWaffoTransport {
    pub fn enqueue(
        &self,
        canonical_path: impl Into<String>,
        outcome: Result<WaffoTransportResponse, WaffoError>,
    ) {
        self.queued
            .lock()
            .expect("fake Waffo queue lock is not poisoned")
            .entry(canonical_path.into())
            .or_default()
            .push_back(outcome);
    }

    pub fn captured(&self) -> Vec<CapturedWaffoRequest> {
        self.captured
            .lock()
            .expect("fake Waffo capture lock is not poisoned")
            .clone()
    }
}

#[async_trait]
impl WaffoTransport for FakeWaffoTransport {
    async fn send(
        &self,
        request: WaffoTransportRequest,
    ) -> Result<WaffoTransportResponse, WaffoError> {
        let path = request.canonical_path.clone();
        self.captured
            .lock()
            .expect("fake Waffo capture lock is not poisoned")
            .push(CapturedWaffoRequest {
                canonical_path: path.clone(),
                merchant_id: request.merchant_id,
                idempotency_key: request.idempotency_key,
                body: request.body,
            });
        self.queued
            .lock()
            .expect("fake Waffo queue lock is not poisoned")
            .get_mut(&path)
            .and_then(VecDeque::pop_front)
            .unwrap_or(Err(WaffoError::Transport(
                WaffoTransportFailure::Connection,
            )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_transport_disables_automatic_redirects() {
        let transport = ReqwestWaffoTransport::new(Duration::from_secs(1)).unwrap();
        let debug = format!("{transport:?}");

        assert!(
            debug.contains("redirect_policy: \"Policy(None)\""),
            "signed provider requests must not follow redirects: {debug}"
        );
    }

    #[test]
    fn custom_transport_can_read_every_required_signed_request_part() {
        let request = WaffoTransportRequest {
            url: Url::parse("https://api.waffo.ai/v1/graphql").unwrap(),
            canonical_path: "/v1/graphql".into(),
            merchant_id: "MER_0123456789ABCDEFGHIJKL".into(),
            timestamp: 1_786_390_400,
            signature: "base64-signature".into(),
            idempotency_key: Some("query_42".into()),
            body: br#"{"query":"query { viewer { id } }"}"#.to_vec(),
            response_max_bytes: 4096,
        };

        assert_eq!(request.url().as_str(), "https://api.waffo.ai/v1/graphql");
        assert_eq!(request.canonical_path(), "/v1/graphql");
        assert_eq!(request.merchant_id(), "MER_0123456789ABCDEFGHIJKL");
        assert_eq!(request.timestamp(), 1_786_390_400);
        assert_eq!(request.signature(), "base64-signature");
        assert_eq!(request.idempotency_key(), Some("query_42"));
        assert!(request.body().starts_with(br#"{"query"#));
        assert_eq!(request.response_max_bytes(), 4096);
    }
}
