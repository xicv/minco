//! In-process HTTP test utilities and deterministic command evidence.
#![forbid(unsafe_code)]

use axum::{body::Body, Router};
use http::{header, HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde::{de::DeserializeOwned, Serialize};
use std::{collections::BTreeMap, process::Output};
use tower::ServiceExt;

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

    pub fn with_header(
        mut self,
        name: HeaderName,
        value: HeaderValue,
    ) -> Self {
        self.default_headers.insert(name, value);
        self
    }

    pub async fn get(&self, uri: &str) -> TestResponse {
        self.request(Method::GET, uri, HeaderMap::new(), Body::empty())
            .await
    }

    pub async fn json<T: Serialize>(
        &self,
        method: Method,
        uri: &str,
        body: &T,
    ) -> TestResponse {
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
    use axum::{extract::Json, routing::{get, post}};

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
