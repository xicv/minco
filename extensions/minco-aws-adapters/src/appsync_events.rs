use crate::AwsAdapterError;
use async_trait::async_trait;
use aws_credential_types::provider::{ProvideCredentials, SharedCredentialsProvider};
use aws_sigv4::{
    http_request::{SignableBody, SignableRequest, SigningSettings, sign},
    sign::v4,
};
use minco_plugin_realtime::{
    RealtimeChannel, RealtimeError, RealtimePublication, RealtimePublisher,
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};
use url::Url;

#[derive(Clone)]
pub struct AppSyncEventsPublisher {
    client: reqwest::Client,
    credentials: SharedCredentialsProvider,
    endpoint: Url,
    namespace: String,
    region: String,
    request_timeout: Duration,
}

impl std::fmt::Debug for AppSyncEventsPublisher {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AppSyncEventsPublisher")
            .field("endpoint", &self.endpoint)
            .field("namespace", &self.namespace)
            .field("region", &self.region)
            .field("request_timeout", &self.request_timeout)
            .finish_non_exhaustive()
    }
}

impl AppSyncEventsPublisher {
    pub fn new(
        client: reqwest::Client,
        credentials: SharedCredentialsProvider,
        endpoint: impl AsRef<str>,
        namespace: impl Into<String>,
        region: impl Into<String>,
    ) -> Result<Self, AwsAdapterError> {
        let region = region.into();
        validate_region(&region)?;
        let endpoint = validate_endpoint(endpoint.as_ref(), &region)?;
        let namespace = namespace.into();
        let parsed_namespace = RealtimeChannel::parse(namespace.clone())
            .map_err(|error| AwsAdapterError::InvalidConfiguration(error.to_string()))?;
        if parsed_namespace.as_str().contains('/') {
            return Err(AwsAdapterError::InvalidConfiguration(
                "AppSync Events namespace must be one portable channel segment".into(),
            ));
        }
        if !parsed_namespace
            .as_str()
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
            || !parsed_namespace
                .as_str()
                .bytes()
                .next_back()
                .is_some_and(|byte| byte.is_ascii_alphanumeric())
        {
            return Err(AwsAdapterError::InvalidConfiguration(
                "AppSync Events namespace must start and end with an ASCII alphanumeric character"
                    .into(),
            ));
        }
        Ok(Self {
            client,
            credentials,
            endpoint,
            namespace,
            region,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        })
    }

    pub fn with_request_timeout(mut self, timeout: Duration) -> Result<Self, AwsAdapterError> {
        if !(MIN_REQUEST_TIMEOUT..=MAX_REQUEST_TIMEOUT).contains(&timeout) {
            return Err(AwsAdapterError::InvalidConfiguration(
                "AppSync Events request timeout must be between 100 milliseconds and 60 seconds"
                    .into(),
            ));
        }
        self.request_timeout = timeout;
        Ok(self)
    }
}

#[derive(Serialize)]
struct PublishRequest {
    channel: String,
    events: [String; 1],
}

#[derive(Deserialize)]
struct PublishResponse {
    #[serde(default)]
    successful: Vec<PublishResult>,
    #[serde(default)]
    failed: Vec<PublishResult>,
}

#[derive(Deserialize)]
struct PublishResult {
    index: usize,
}

const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_APPSYNC_EVENT_BYTES: usize = 240 * 1024;
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MIN_REQUEST_TIMEOUT: Duration = Duration::from_millis(100);
const MAX_REQUEST_TIMEOUT: Duration = Duration::from_mins(1);

#[async_trait]
impl RealtimePublisher for AppSyncEventsPublisher {
    async fn publish(&self, publication: &RealtimePublication) -> Result<(), RealtimeError> {
        let event = serde_json::to_string(&publication.envelope)
            .map_err(|_| RealtimeError::Publish("realtime envelope serialization failed".into()))?;
        if event.len() > MAX_APPSYNC_EVENT_BYTES {
            return Err(RealtimeError::InvalidEnvelope(
                "encoded envelope exceeds the AppSync Events provider limit".into(),
            ));
        }
        let body = serde_json::to_vec(&PublishRequest {
            channel: format!("/{}/{}", self.namespace, publication.channel.as_str()),
            events: [event],
        })
        .map_err(|_| {
            RealtimeError::Publish("AppSync Events request serialization failed".into())
        })?;
        let credentials =
            self.credentials.provide_credentials().await.map_err(|_| {
                RealtimeError::Unavailable("AWS credentials are unavailable".into())
            })?;
        let identity = credentials.into();
        let signing_params = v4::SigningParams::builder()
            .identity(&identity)
            .region(&self.region)
            .name("appsync")
            .time(SystemTime::now())
            .settings(SigningSettings::default())
            .build()
            .map_err(|_| RealtimeError::Publish("AppSync Events signing setup failed".into()))?
            .into();
        let signable = SignableRequest::new(
            "POST",
            self.endpoint.as_str(),
            std::iter::once(("content-type", "application/json")),
            SignableBody::Bytes(&body),
        )
        .map_err(|_| RealtimeError::Publish("AppSync Events request is not signable".into()))?;
        let (instructions, _) = sign(signable, &signing_params)
            .map_err(|_| RealtimeError::Publish("AppSync Events request signing failed".into()))?
            .into_parts();
        let mut request = self
            .client
            .post(self.endpoint.clone())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .timeout(self.request_timeout);
        for (name, value) in instructions.headers() {
            request = request.header(name, value);
        }
        let mut response = request
            .body(body)
            .send()
            .await
            .map_err(|_| RealtimeError::Unavailable("AppSync Events request failed".into()))?;
        let status = response.status();
        if !status.is_success() {
            let message = format!("AppSync Events returned HTTP {}", status.as_u16());
            return if status.as_u16() == 408 || status.as_u16() == 429 || status.is_server_error() {
                Err(RealtimeError::Unavailable(message))
            } else {
                Err(RealtimeError::Rejected(message))
            };
        }
        let mut encoded = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| RealtimeError::Unavailable("AppSync Events response failed".into()))?
        {
            if encoded.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(RealtimeError::Rejected(
                    "AppSync Events response exceeded the bounded response limit".into(),
                ));
            }
            encoded.extend_from_slice(&chunk);
        }
        let result = serde_json::from_slice::<PublishResponse>(&encoded).map_err(|_| {
            RealtimeError::Rejected("AppSync Events returned an invalid response".into())
        })?;
        if !result.failed.is_empty()
            || result.successful.len() != 1
            || result.successful[0].index != 0
        {
            return Err(RealtimeError::Rejected(
                "AppSync Events did not accept the event".into(),
            ));
        }
        Ok(())
    }
}

fn validate_region(region: &str) -> Result<(), AwsAdapterError> {
    if !(3..=32).contains(&region.len())
        || region.starts_with('-')
        || region.ends_with('-')
        || !region
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(AwsAdapterError::InvalidConfiguration(
            "AWS region is not a valid lower-ASCII region identifier".into(),
        ));
    }
    Ok(())
}

fn validate_endpoint(value: &str, region: &str) -> Result<Url, AwsAdapterError> {
    let endpoint = Url::parse(value).map_err(|_| {
        AwsAdapterError::InvalidConfiguration("AppSync Events endpoint is not a valid URL".into())
    })?;
    let host = endpoint.host_str().unwrap_or_default();
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback());
    let aws_host = [
        format!(".appsync-api.{region}.amazonaws.com"),
        format!(".appsync-api.{region}.amazonaws.com.cn"),
    ]
    .iter()
    .any(|suffix| host.strip_suffix(suffix).is_some_and(valid_appsync_api_id));
    let transport_allowed =
        (endpoint.scheme() == "https" && aws_host) || (endpoint.scheme() == "http" && loopback);
    if !transport_allowed
        || endpoint.path() != "/event"
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
    {
        return Err(AwsAdapterError::InvalidConfiguration(
            "AppSync Events endpoint must be the regional HTTPS /event data plane URL; loopback HTTP is test-only"
                .into(),
        ));
    }
    Ok(endpoint)
}

fn valid_appsync_api_id(value: &str) -> bool {
    (1..=63).contains(&value.len())
        && !value.contains('.')
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .next_back()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_credential_types::{Credentials, provider::SharedCredentialsProvider};
    use axum::{Json, Router, body::Bytes, extract::State, http::HeaderMap, routing::post};
    use minco_plugin_realtime::{
        RealtimeChannel, RealtimeEnvelope, RealtimePublication, RealtimePublisher,
    };
    use serde_json::{Value, json};
    use tokio::sync::oneshot;

    #[derive(Debug)]
    struct CapturedRequest {
        headers: HeaderMap,
        body: Value,
    }

    async fn capture(
        State(sender): State<
            std::sync::Arc<std::sync::Mutex<Option<oneshot::Sender<CapturedRequest>>>>,
        >,
        headers: HeaderMap,
        body: Bytes,
    ) -> Json<Value> {
        let captured = CapturedRequest {
            headers,
            body: serde_json::from_slice(&body).unwrap(),
        };
        sender
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .send(captured)
            .unwrap();
        Json(json!({"successful": [{"identifier": "evt-7", "index": 0}], "failed": []}))
    }

    async fn partial_failure() -> Json<Value> {
        Json(json!({
            "successful": [],
            "failed": [{"identifier": "evt-7", "index": 0, "errorMessage": "provider-private-detail"}]
        }))
    }

    fn publication() -> RealtimePublication {
        RealtimePublication {
            channel: RealtimeChannel::parse("tenant-42/order-7").unwrap(),
            envelope: RealtimeEnvelope {
                id: "evt-7".into(),
                event_type: "order.updated".into(),
                occurred_at: "2026-08-04T03:50:44Z".into(),
                payload: json!({"order_id": "order-7"}),
            },
        }
    }

    #[tokio::test]
    async fn publisher_posts_appsync_shape_with_iam_signature() {
        let (sender, receiver) = oneshot::channel();
        let state = std::sync::Arc::new(std::sync::Mutex::new(Some(sender)));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(
            axum::serve(
                listener,
                Router::new()
                    .route("/event", post(capture))
                    .with_state(state),
            )
            .into_future(),
        );
        let credentials = SharedCredentialsProvider::new(Credentials::new(
            "AKIDEXAMPLE",
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            None,
            None,
            "appsync-events-test",
        ));
        let publisher = AppSyncEventsPublisher::new(
            reqwest::Client::new(),
            credentials,
            format!("http://{address}/event"),
            "orders",
            "ap-southeast-2",
        )
        .unwrap();
        let publication = publication();

        publisher.publish(&publication).await.unwrap();
        let captured = receiver.await.unwrap();

        assert_eq!(captured.body["channel"], "/orders/tenant-42/order-7");
        let encoded = captured.body["events"][0].as_str().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(encoded).unwrap()["id"],
            "evt-7"
        );
        let authorization = captured.headers["authorization"].to_str().unwrap();
        assert!(authorization.starts_with("AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/"));
        assert!(captured.headers.contains_key("x-amz-date"));
        assert_eq!(captured.headers["content-type"], "application/json");
        server.abort();
    }

    #[tokio::test]
    async fn partial_failure_is_rejected_without_exposing_provider_body() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(
            axum::serve(
                listener,
                Router::new().route("/event", post(partial_failure)),
            )
            .into_future(),
        );
        let credentials = SharedCredentialsProvider::new(Credentials::new(
            "AKIDEXAMPLE",
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            None,
            None,
            "appsync-events-test",
        ));
        let publisher = AppSyncEventsPublisher::new(
            reqwest::Client::new(),
            credentials,
            format!("http://{address}/event"),
            "orders",
            "ap-southeast-2",
        )
        .unwrap();

        let error = publisher.publish(&publication()).await.unwrap_err();

        assert!(matches!(error, RealtimeError::Rejected(_)));
        assert!(!error.to_string().contains("provider-private-detail"));
        server.abort();
    }

    #[test]
    fn production_endpoint_requires_exactly_one_appsync_api_id_label() {
        let credentials = SharedCredentialsProvider::new(Credentials::new(
            "AKIDEXAMPLE",
            "secret",
            None,
            None,
            "appsync-events-test",
        ));

        let error = AppSyncEventsPublisher::new(
            reqwest::Client::new(),
            credentials,
            "https://attacker.example.appsync-api.ap-southeast-2.amazonaws.com/event",
            "orders",
            "ap-southeast-2",
        )
        .unwrap_err();

        assert!(matches!(error, AwsAdapterError::InvalidConfiguration(_)));
    }

    #[tokio::test]
    async fn direct_adapter_use_rejects_events_above_the_provider_limit() {
        let credentials = SharedCredentialsProvider::new(Credentials::new(
            "AKIDEXAMPLE",
            "secret",
            None,
            None,
            "appsync-events-test",
        ));
        let publisher = AppSyncEventsPublisher::new(
            reqwest::Client::new(),
            credentials,
            "http://127.0.0.1:9/event",
            "orders",
            "ap-southeast-2",
        )
        .unwrap();
        let mut publication = publication();
        publication.envelope.payload = json!({"content": "x".repeat(240 * 1024)});

        let error = publisher.publish(&publication).await.unwrap_err();

        assert!(matches!(error, RealtimeError::InvalidEnvelope(_)));
    }

    #[tokio::test]
    async fn request_timeout_is_bounded_and_classified_as_unavailable() {
        async fn delayed_response() -> Json<Value> {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            Json(json!({"successful": [{"index": 0}], "failed": []}))
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(
            axum::serve(
                listener,
                Router::new().route("/event", post(delayed_response)),
            )
            .into_future(),
        );
        let credentials = SharedCredentialsProvider::new(Credentials::new(
            "AKIDEXAMPLE",
            "secret",
            None,
            None,
            "appsync-events-test",
        ));
        let publisher = AppSyncEventsPublisher::new(
            reqwest::Client::new(),
            credentials,
            format!("http://{address}/event"),
            "orders",
            "ap-southeast-2",
        )
        .unwrap()
        .with_request_timeout(std::time::Duration::from_millis(100))
        .unwrap();

        let error = publisher.publish(&publication()).await.unwrap_err();

        assert!(matches!(error, RealtimeError::Unavailable(_)));
        server.abort();
    }
}
