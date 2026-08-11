use async_trait::async_trait;
use chrono::Utc;
use hmac::{Hmac, KeyInit, Mac};
use minco_plugin_notifications::{
    Notification, NotificationChannel, NotificationError, NotificationSink,
};
use sha2::Sha256;
use std::{fmt::Write as _, net::IpAddr, sync::Arc, time::Duration};

type HmacSha256 = Hmac<Sha256>;
const MAX_WEBHOOK_PAYLOAD_BYTES: usize = 1_048_576;

#[derive(Clone)]
pub struct SignedWebhookNotificationSink {
    client: reqwest::Client,
    endpoint: reqwest::Url,
    secret: Arc<[u8]>,
}

impl std::fmt::Debug for SignedWebhookNotificationSink {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SignedWebhookNotificationSink")
            .field("endpoint", &"[REDACTED WEBHOOK URL]")
            .field("secret", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

impl SignedWebhookNotificationSink {
    pub async fn new(
        endpoint: impl AsRef<str>,
        secret: impl Into<Vec<u8>>,
    ) -> Result<Self, NotificationError> {
        Self::build(endpoint.as_ref(), secret.into(), false).await
    }

    async fn build(
        endpoint: &str,
        secret: Vec<u8>,
        allow_loopback_http: bool,
    ) -> Result<Self, NotificationError> {
        let endpoint = reqwest::Url::parse(endpoint)
            .map_err(|_| NotificationError::Delivery("webhook URL is invalid".into()))?;
        let host = endpoint
            .host_str()
            .ok_or_else(|| NotificationError::Delivery("webhook URL requires a host".into()))?;
        let local_http = allow_loopback_http
            && endpoint.scheme() == "http"
            && matches!(host, "127.0.0.1" | "::1" | "localhost");
        if (!local_http && endpoint.scheme() != "https")
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.fragment().is_some()
            || (!allow_loopback_http
                && (host.eq_ignore_ascii_case("localhost")
                    || host.strip_suffix(".local").is_some()
                    || host.parse::<std::net::IpAddr>().is_ok()))
            || secret.len() < 32
        {
            return Err(NotificationError::Delivery(
                "webhook requires HTTPS, a DNS host, no credentials/fragment, and a 32-byte secret"
                    .into(),
            ));
        }
        let port = endpoint
            .port_or_known_default()
            .ok_or_else(|| NotificationError::Delivery("webhook URL requires a port".into()))?;
        let addresses = tokio::net::lookup_host((host, port))
            .await
            .map_err(|error| {
                NotificationError::Delivery(format!("webhook DNS resolution failed: {error}"))
            })?
            .collect::<Vec<_>>();
        if addresses.is_empty()
            || (!allow_loopback_http
                && addresses
                    .iter()
                    .any(|address| !is_public_address(address.ip())))
        {
            return Err(NotificationError::Delivery(
                "webhook DNS must resolve only to public addresses".into(),
            ));
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .resolve_to_addrs(host, &addresses)
            .build()
            .map_err(|error| {
                NotificationError::Delivery(format!("webhook client setup failed: {error}"))
            })?;
        Ok(Self {
            client,
            endpoint,
            secret: Arc::from(secret),
        })
    }

    #[cfg(test)]
    async fn local_test(endpoint: &str, secret: Vec<u8>) -> Result<Self, NotificationError> {
        Self::build(endpoint, secret, true).await
    }
}

fn is_public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let [first, second, _, _] = address.octets();
            !(address.is_private()
                || address.is_loopback()
                || address.is_link_local()
                || address.is_unspecified()
                || address.is_multicast()
                || address.is_broadcast()
                || address.is_documentation()
                || first == 0
                || (first == 100 && (64..=127).contains(&second))
                || (first == 198 && (18..=19).contains(&second))
                || first >= 240)
        }
        IpAddr::V6(address) => {
            let first = address.segments()[0];
            (0x2000..=0x3fff).contains(&first) && !address.is_multicast()
        }
    }
}

#[async_trait]
impl NotificationSink for SignedWebhookNotificationSink {
    async fn send(&self, notification: Notification) -> Result<(), NotificationError> {
        if notification.channel != NotificationChannel::Webhook {
            return Err(NotificationError::Delivery(
                "signed-webhook adapter accepts only webhook notifications".into(),
            ));
        }
        if notification.recipient != self.endpoint.as_str()
            || notification.title.trim().is_empty()
            || notification.title.len() > 200
            || notification.title.chars().any(char::is_control)
            || notification.body.len() > MAX_WEBHOOK_PAYLOAD_BYTES
            || notification
                .link
                .as_deref()
                .is_some_and(|link| link.len() > 2048 || link.chars().any(char::is_control))
        {
            return Err(NotificationError::Delivery(
                "webhook notification fields exceed the configured delivery boundary".into(),
            ));
        }
        let payload = serde_json::to_vec(&notification)
            .map_err(|error| NotificationError::Delivery(error.to_string()))?;
        if payload.len() > MAX_WEBHOOK_PAYLOAD_BYTES {
            return Err(NotificationError::Delivery(
                "serialized webhook payload exceeds the delivery boundary".into(),
            ));
        }
        let timestamp = Utc::now().timestamp().to_string();
        let signature = signature(&self.secret, &timestamp, &payload)?;

        let response = self
            .client
            .post(self.endpoint.clone())
            .header("content-type", "application/json")
            .header("x-minco-webhook-id", notification.id.to_string())
            .header("x-minco-webhook-timestamp", &timestamp)
            .header("x-minco-webhook-signature", format!("v1={signature}"))
            .body(payload)
            .send()
            .await
            .map_err(|_| NotificationError::Delivery("webhook request failed".into()))?;
        if !response.status().is_success() {
            return Err(NotificationError::Delivery(format!(
                "webhook returned HTTP {}",
                response.status()
            )));
        }
        Ok(())
    }
}

fn signature(secret: &[u8], timestamp: &str, payload: &[u8]) -> Result<String, NotificationError> {
    let mut mac = HmacSha256::new_from_slice(secret)
        .map_err(|_| NotificationError::Delivery("webhook HMAC key is invalid".into()))?;
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(payload);
    let bytes = mac.finalize().into_bytes();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}")
            .map_err(|_| NotificationError::Delivery("webhook signature failed".into()))?;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Bytes, http::HeaderMap, routing::post};
    use std::sync::Mutex;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn endpoint_and_signature_contract_fail_closed() {
        assert!(
            SignedWebhookNotificationSink::new("http://127.0.0.1/hook", vec![7; 32],)
                .await
                .is_err()
        );
        assert_eq!(
            signature(&[7; 32], "1", b"{}").unwrap().len(),
            64,
            "HMAC-SHA256 is lowercase hexadecimal"
        );
    }

    #[tokio::test]
    async fn debug_and_validation_do_not_expose_webhook_capabilities() {
        let sink = SignedWebhookNotificationSink::local_test(
            "http://127.0.0.1:9/hook/path-token?query-token=value",
            vec![7; 32],
        )
        .await
        .unwrap();
        let debug = format!("{sink:?}");
        assert!(!debug.contains("path-token"));
        assert!(!debug.contains("query-token"));
        assert!(!debug.contains("127.0.0.1"));

        assert!(
            sink.send(Notification::new(
                "feedback.created",
                NotificationChannel::Webhook,
                "http://127.0.0.1:9/hook/path-token?query-token=value",
                "x".repeat(201),
                "body",
            ))
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn webhook_sends_the_exact_signed_json_body() {
        let captured = Arc::new(Mutex::new(None::<(HeaderMap, Vec<u8>)>));
        let state = captured.clone();
        let app = Router::new().route(
            "/hook",
            post(move |headers: HeaderMap, body: Bytes| {
                let state = state.clone();
                async move {
                    *state.lock().unwrap() = Some((headers, body.to_vec()));
                    "ok"
                }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let endpoint = format!("http://{address}/hook");
        let sink = SignedWebhookNotificationSink::local_test(&endpoint, vec![7; 32])
            .await
            .unwrap();
        sink.send(Notification::new(
            "feedback.created",
            NotificationChannel::Webhook,
            endpoint,
            "Feedback",
            "Created",
        ))
        .await
        .unwrap();

        let (headers, body) = captured.lock().unwrap().clone().unwrap();
        let timestamp = headers["x-minco-webhook-timestamp"].to_str().unwrap();
        let expected = format!("v1={}", signature(&[7; 32], timestamp, &body).unwrap());
        assert_eq!(headers["x-minco-webhook-signature"], expected);
    }
}
