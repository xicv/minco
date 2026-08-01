//! In-process HTTP test utilities and deterministic command evidence.
#![forbid(unsafe_code)]

use axum::{Router, body::Body};
use http::{HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::BTreeMap, process::Output};
use tower::ServiceExt;

mod plugin_conformance;

pub use plugin_conformance::{
    ADAPTER_CONFORMANCE_PROFILE, ConformanceAssurance, ConformanceDiagnostic, ConformanceStatus,
    PLUGIN_CONFORMANCE_PROFILE, PluginConformance, PluginConformanceReport,
    RUNTIME_CONFORMANCE_PROFILE,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FixtureIdentity {
    pub namespace: String,
    pub kind: String,
    pub ordinal: u64,
    pub stable_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureError {
    message: String,
}

impl std::fmt::Display for FixtureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for FixtureError {}

/// Produces deterministic identities without coupling fixtures to an ORM,
/// database, wall clock, random-number generator, or global process state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureSequence {
    namespace: String,
    next_ordinal: u64,
}

impl FixtureSequence {
    pub fn new(namespace: impl Into<String>) -> Result<Self, FixtureError> {
        let namespace = namespace.into();
        validate_fixture_part(&namespace, "fixture namespace")?;
        Ok(Self {
            namespace,
            next_ordinal: 1,
        })
    }

    pub fn next(&mut self, kind: &str) -> Result<FixtureIdentity, FixtureError> {
        validate_fixture_part(kind, "fixture kind")?;
        let ordinal = self.next_ordinal;
        let next_ordinal = ordinal.checked_add(1).ok_or_else(|| FixtureError {
            message: "fixture sequence exhausted its ordinal range".into(),
        })?;
        let identity = FixtureIdentity {
            namespace: self.namespace.clone(),
            kind: kind.to_owned(),
            ordinal,
            stable_id: format!("{}:{kind}:{ordinal:08}", self.namespace),
        };
        self.next_ordinal = next_ordinal;
        Ok(identity)
    }

    pub fn build<T>(
        &mut self,
        kind: &str,
        builder: impl FnOnce(FixtureIdentity) -> T,
    ) -> Result<T, FixtureError> {
        self.next(kind).map(builder)
    }
}

fn validate_fixture_part(value: &str, label: &str) -> Result<(), FixtureError> {
    let valid = !value.is_empty()
        && value.len() <= 64
        && value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && value
            .as_bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.contains("--");
    if valid {
        Ok(())
    } else {
        Err(FixtureError {
            message: format!(
                "{label} must be a lowercase kebab-case identifier of at most 64 bytes"
            ),
        })
    }
}

#[derive(Debug, Clone)]
pub struct TestClient {
    router: Router,
    default_headers: HeaderMap,
}

impl TestClient {
    #[must_use]
    pub fn new(router: Router) -> Self {
        Self {
            router,
            default_headers: HeaderMap::new(),
        }
    }

    #[must_use]
    pub fn with_header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.default_headers.insert(name, value);
        self
    }

    pub async fn get(&self, uri: &str) -> TestResponse {
        self.request(Method::GET, uri, HeaderMap::new(), Body::empty())
            .await
    }

    pub async fn json<T: Serialize>(&self, method: Method, uri: &str, body: &T) -> TestResponse {
        let body = serde_json::to_vec(body).expect("test request serialization must succeed");
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        self.request(method, uri, headers, Body::from(body)).await
    }

    pub async fn request(
        &self,
        method: Method,
        uri: &str,
        request_headers: HeaderMap,
        body: Body,
    ) -> TestResponse {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .body(body)
            .expect("valid test request");
        let mut headers = self.default_headers.clone();
        headers.extend(request_headers);
        *request.headers_mut() = headers;
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("Axum Router uses an infallible service error");
        let status = response.status();
        let headers = response.headers().clone();
        let body = response
            .into_body()
            .collect()
            .await
            .expect("response body collection must succeed")
            .to_bytes()
            .to_vec();
        TestResponse {
            status,
            headers,
            body,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl TestResponse {
    pub fn json<T: DeserializeOwned>(&self) -> serde_json::Result<T> {
        serde_json::from_slice(&self.body)
    }

    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn assert_status(&self, expected: StatusCode) {
        assert_eq!(self.status, expected, "response body: {}", self.text());
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandEvidence {
    pub command: Vec<String>,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub environment: BTreeMap<String, String>,
}

impl CommandEvidence {
    #[must_use]
    pub fn from_output(command: Vec<String>, output: &Output) -> Self {
        Self {
            command,
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            environment: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::Json,
        routing::{get, post},
    };

    #[tokio::test]
    async fn client_exercises_router_without_a_socket() {
        let client = TestClient::new(Router::new().route("/health", get(|| async { "ok" })));
        let response = client.get("/health").await;
        response.assert_status(StatusCode::OK);
        assert_eq!(response.text(), "ok");
    }

    #[tokio::test]
    async fn json_requests_set_the_content_type() {
        let router = Router::new().route(
            "/echo",
            post(|Json(value): Json<serde_json::Value>| async move { Json(value) }),
        );
        let response = TestClient::new(router)
            .json(Method::POST, "/echo", &serde_json::json!({"ok": true}))
            .await;
        response.assert_status(StatusCode::OK);
        assert_eq!(response.json::<serde_json::Value>().unwrap()["ok"], true);
    }
}
