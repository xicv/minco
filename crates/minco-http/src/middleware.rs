use crate::response::{DEPRECATION_HEADER, SUNSET_HEADER};
use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, Request, State},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use http::{Extensions, HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Version, header};
use http_body_util::{BodyExt as _, LengthLimitError, Limited};
use std::{
    collections::BTreeMap,
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use thiserror::Error;
use tower_http::{
    compression::{
        CompressionLayer, CompressionLevel, DefaultPredicate, Predicate, predicate::SizeAbove,
    },
    cors::{AllowOrigin, CorsLayer},
    request_id::PropagateRequestIdLayer,
    sensitive_headers::SetSensitiveRequestHeadersLayer,
    trace::TraceLayer,
};

use crate::{ApiFailure, request_id_from_headers};

pub static REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");
pub static CSRF_HEADER: HeaderName = HeaderName::from_static("x-minco-csrf");

/// Minimum known response size eligible for Minco's negotiated gzip layer.
///
/// Unknown-length streaming responses remain eligible and are still filtered by
/// Tower HTTP's content-type predicate. The threshold avoids spending Lambda CPU
/// and Lambda proxy base64 overhead on tiny bodies that commonly grow after gzip.
pub const RESPONSE_COMPRESSION_MIN_BYTES: u64 = 1024;

/// Response extension that opts one response out of dynamic compression.
///
/// Use this for a response that combines secrets with attacker-controlled
/// reflection, or for another response whose application protocol requires an
/// unencoded representation. Global compression remains enabled for other
/// eligible responses.
#[derive(Debug, Clone, Copy, Default)]
pub struct DisableResponseCompression;

/// Exact browser-request, response-exposure, and diagnostic-redaction policy.
///
/// Header names are normalized by `http::HeaderName`, de-duplicated
/// deterministically, and never accept the wildcard token. Applications own the
/// baseline policy; installed HTTP plugins add only their exact requirements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpHeaderPolicy {
    allowed_request: BTreeMap<String, HeaderName>,
    exposed_response: BTreeMap<String, HeaderName>,
    sensitive_request: BTreeMap<String, HeaderName>,
}

impl Default for HttpHeaderPolicy {
    fn default() -> Self {
        let mut policy = Self::empty();
        for name in [
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::IF_MATCH,
            header::IF_NONE_MATCH,
            HeaderName::from_static("idempotency-key"),
            REQUEST_ID_HEADER.clone(),
        ] {
            policy
                .allow_request_header(name)
                .expect("built-in header is valid");
        }
        for name in [
            header::ETAG,
            header::LINK,
            header::LOCATION,
            header::RETRY_AFTER,
            header::WWW_AUTHENTICATE,
            DEPRECATION_HEADER.clone(),
            REQUEST_ID_HEADER.clone(),
            SUNSET_HEADER.clone(),
        ] {
            policy
                .expose_response_header(name)
                .expect("built-in header is valid");
        }
        for name in [
            header::AUTHORIZATION,
            header::COOKIE,
            HeaderName::from_static("idempotency-key"),
        ] {
            policy
                .mark_request_header_sensitive(name)
                .expect("built-in header is valid");
        }
        policy
    }
}

impl HttpHeaderPolicy {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            allowed_request: BTreeMap::new(),
            exposed_response: BTreeMap::new(),
            sensitive_request: BTreeMap::new(),
        }
    }

    pub fn allow_request_header(&mut self, name: HeaderName) -> Result<(), HttpConfigurationError> {
        insert_header(&mut self.allowed_request, name)
    }

    pub fn expose_response_header(
        &mut self,
        name: HeaderName,
    ) -> Result<(), HttpConfigurationError> {
        insert_header(&mut self.exposed_response, name)
    }

    pub fn mark_request_header_sensitive(
        &mut self,
        name: HeaderName,
    ) -> Result<(), HttpConfigurationError> {
        insert_header(&mut self.sensitive_request, name)
    }

    pub fn allow_request_header_name(&mut self, name: &str) -> Result<(), HttpConfigurationError> {
        self.allow_request_header(parse_header(name)?)
    }

    pub fn expose_response_header_name(
        &mut self,
        name: &str,
    ) -> Result<(), HttpConfigurationError> {
        self.expose_response_header(parse_header(name)?)
    }

    pub fn mark_request_header_name_sensitive(
        &mut self,
        name: &str,
    ) -> Result<(), HttpConfigurationError> {
        self.mark_request_header_sensitive(parse_header(name)?)
    }

    /// Enables the application-selected cookie/CSRF request boundary.
    ///
    /// `Cookie` is already marked sensitive and is browser-managed, so only
    /// the exact CSRF header is added to the CORS request set.
    pub fn enable_cookie_csrf(&mut self) -> Result<(), HttpConfigurationError> {
        self.allow_request_header(CSRF_HEADER.clone())?;
        self.mark_request_header_sensitive(CSRF_HEADER.clone())
    }

    pub(crate) fn merge(&mut self, additions: &Self) -> Result<(), HttpConfigurationError> {
        for name in additions.allowed_request.values().cloned() {
            self.allow_request_header(name)?;
        }
        for name in additions.exposed_response.values().cloned() {
            self.expose_response_header(name)?;
        }
        for name in additions.sensitive_request.values().cloned() {
            self.mark_request_header_sensitive(name)?;
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), HttpConfigurationError> {
        for name in self
            .allowed_request
            .values()
            .chain(self.exposed_response.values())
            .chain(self.sensitive_request.values())
        {
            reject_wildcard(name)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn allowed_request_headers(&self) -> Vec<HeaderName> {
        self.allowed_request.values().cloned().collect()
    }

    #[must_use]
    pub fn exposed_response_headers(&self) -> Vec<HeaderName> {
        self.exposed_response.values().cloned().collect()
    }

    #[must_use]
    pub fn sensitive_request_headers(&self) -> Vec<HeaderName> {
        self.sensitive_request.values().cloned().collect()
    }
}

#[derive(Debug, Clone)]
pub struct HttpRuntimeConfig {
    /// Exact origins accepted by the browser API. Wildcards are intentionally unsupported.
    pub allowed_origins: Vec<String>,
    /// Allows cookies and browser authorization credentials for exact configured origins.
    pub allow_credentials: bool,
    pub timeout: Duration,
    pub max_request_body_bytes: usize,
    /// Enables negotiated fastest-level gzip for eligible responses at least 1 KiB.
    pub compression: bool,
    /// Application baseline extended by exact installed-plugin requirements.
    pub header_policy: HttpHeaderPolicy,
}

impl Default for HttpRuntimeConfig {
    fn default() -> Self {
        Self {
            allowed_origins: vec!["http://127.0.0.1:3000".into()],
            allow_credentials: false,
            timeout: Duration::from_secs(15),
            max_request_body_bytes: 1024 * 1024,
            compression: true,
            header_policy: HttpHeaderPolicy::default(),
        }
    }
}

pub fn apply_standard_middleware(
    router: Router,
    config: &HttpRuntimeConfig,
) -> Result<Router, HttpConfigurationError> {
    validate_runtime_config(config)?;
    let origins = config
        .allowed_origins
        .iter()
        .map(|origin| {
            HeaderValue::from_str(origin).map_err(|source| HttpConfigurationError::InvalidOrigin {
                origin: origin.clone(),
                source,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(config.header_policy.allowed_request_headers())
        .expose_headers(config.header_policy.exposed_response_headers());
    let cors = if config.allow_credentials {
        cors.allow_credentials(true)
    } else {
        cors
    };

    // Router layers run request-side in reverse declaration order. Keep this
    // explicit so an untrusted request ID is normalized before propagation,
    // sensitive marking and tracing, while Minco-owned failures remain inside
    // CORS and correlation handling. The body limit wraps the stream and the
    // timeout wraps only the downstream operation future.
    let router = router
        .layer(DefaultBodyLimit::disable())
        .layer(middleware::from_fn_with_state(
            config.timeout,
            enforce_request_timeout,
        ))
        .layer(middleware::from_fn_with_state(
            config.max_request_body_bytes,
            enforce_request_body_limit,
        ))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(SetSensitiveRequestHeadersLayer::new(
            config.header_policy.sensitive_request_headers(),
        ))
        .layer(PropagateRequestIdLayer::new(REQUEST_ID_HEADER.clone()))
        .layer(middleware::from_fn(normalize_request_id));

    Ok(if config.compression {
        let predicate = DefaultPredicate::new()
            .and(SizeAbove::new(RESPONSE_COMPRESSION_MIN_BYTES))
            .and(
                |_status: StatusCode,
                 _version: Version,
                 _headers: &HeaderMap,
                 extensions: &Extensions| {
                    extensions.get::<DisableResponseCompression>().is_none()
                },
            );
        router.layer(
            CompressionLayer::new()
                .quality(CompressionLevel::Fastest)
                .compress_when(predicate),
        )
    } else {
        router
    })
}

async fn normalize_request_id(mut request: Request, next: Next) -> Response {
    let request_id = request_id_from_headers(request.headers());
    request.headers_mut().insert(
        REQUEST_ID_HEADER.clone(),
        HeaderValue::from_str(&request_id).expect("safe request IDs are valid headers"),
    );
    next.run(request).await
}

async fn enforce_request_body_limit(
    State(limit): State<usize>,
    mut request: Request,
    next: Next,
) -> Response {
    let request_id = request_id_from_headers(request.headers());
    let declared_length = request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok());
    if declared_length.is_some_and(|length| length > limit) {
        return payload_too_large(request_id).into_response();
    }

    let body = std::mem::take(request.body_mut());
    let overflowed = Arc::new(AtomicBool::new(false));
    let overflow_observer = Arc::clone(&overflowed);
    let body = Limited::new(body, limit).map_err(move |error| {
        if error.downcast_ref::<LengthLimitError>().is_some() {
            overflow_observer.store(true, Ordering::Release);
        }
        error
    });
    *request.body_mut() = Body::new(body);
    let response = next.run(request).await;
    if overflowed.load(Ordering::Acquire) {
        payload_too_large(request_id).into_response()
    } else {
        response
    }
}

async fn enforce_request_timeout(
    State(timeout): State<Duration>,
    request: Request,
    next: Next,
) -> Response {
    let request_id = request_id_from_headers(request.headers());
    match tokio::time::timeout(timeout, next.run(request)).await {
        Ok(response) => response,
        Err(_) => ApiFailure::new(
            StatusCode::REQUEST_TIMEOUT,
            "request_timeout",
            "Request timeout",
            "The request did not complete within the configured time limit.",
            request_id,
        )
        .into_response(),
    }
}

fn payload_too_large(request_id: String) -> ApiFailure {
    ApiFailure::new(
        StatusCode::PAYLOAD_TOO_LARGE,
        "payload_too_large",
        "Payload too large",
        "Request body exceeds the configured limit.",
        request_id,
    )
}

fn validate_runtime_config(config: &HttpRuntimeConfig) -> Result<(), HttpConfigurationError> {
    if config.allowed_origins.is_empty() {
        return Err(HttpConfigurationError::NoAllowedOrigins);
    }
    if config
        .allowed_origins
        .iter()
        .any(|origin| origin.trim() == "*")
    {
        return Err(HttpConfigurationError::WildcardOrigin);
    }
    if config.timeout.is_zero() {
        return Err(HttpConfigurationError::ZeroTimeout);
    }
    if config.max_request_body_bytes == 0 {
        return Err(HttpConfigurationError::ZeroRequestBodyLimit);
    }
    config.header_policy.validate()
}

fn parse_header(name: &str) -> Result<HeaderName, HttpConfigurationError> {
    HeaderName::from_str(name).map_err(|source| HttpConfigurationError::InvalidHeaderName {
        name: name.to_owned(),
        source,
    })
}

fn insert_header(
    destination: &mut BTreeMap<String, HeaderName>,
    name: HeaderName,
) -> Result<(), HttpConfigurationError> {
    reject_wildcard(&name)?;
    destination.insert(name.as_str().to_owned(), name);
    Ok(())
}

fn reject_wildcard(name: &HeaderName) -> Result<(), HttpConfigurationError> {
    if name.as_str() == "*" {
        Err(HttpConfigurationError::WildcardHeader)
    } else {
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum HttpConfigurationError {
    #[error("HTTP policy requires at least one exact allowed origin")]
    NoAllowedOrigins,
    #[error("wildcard CORS origins are unsupported")]
    WildcardOrigin,
    #[error("wildcard HTTP headers are unsupported")]
    WildcardHeader,
    #[error("HTTP timeout must be greater than zero")]
    ZeroTimeout,
    #[error("HTTP request-body limit must be greater than zero")]
    ZeroRequestBodyLimit,
    #[error("invalid allowed origin {origin:?}: {source}")]
    InvalidOrigin {
        origin: String,
        #[source]
        source: http::header::InvalidHeaderValue,
    },
    #[error("invalid HTTP header name {name:?}: {source}")]
    InvalidHeaderName {
        name: String,
        #[source]
        source: http::header::InvalidHeaderName,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        response::{IntoResponse, Response},
        routing::get,
    };
    use tower::ServiceExt;

    fn large_compressible_body() -> String {
        "minco-response-compression-".repeat(128)
    }

    fn body_of_exact_bytes(len: usize) -> String {
        let body = "a".repeat(len);
        assert_eq!(
            body.len(),
            len,
            "one-byte ASCII repetition must produce an exact byte length"
        );
        body
    }

    fn compression_threshold() -> usize {
        usize::try_from(RESPONSE_COMPRESSION_MIN_BYTES).expect("threshold fits the platform usize")
    }

    fn vary_accepts_encoding(response: &http::Response<axum::body::Body>) -> bool {
        response
            .headers()
            .get_all(header::VARY)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(','))
            .any(|value| value.trim().eq_ignore_ascii_case("accept-encoding"))
    }

    fn gzip_router_with(body: String) -> Router {
        Router::new().route(
            "/",
            get(move || {
                let body = body.clone();
                async move { body }
            }),
        )
    }

    async fn gzip_response(
        router: Router,
        accept_encoding: Option<&str>,
    ) -> http::Response<axum::body::Body> {
        let mut request = http::Request::get("/").body(Body::empty()).unwrap();
        if let Some(value) = accept_encoding {
            request.headers_mut().insert(
                header::ACCEPT_ENCODING,
                HeaderValue::from_str(value).expect("test accept-encoding value is valid"),
            );
        }
        router.oneshot(request).await.unwrap()
    }

    #[tokio::test]
    async fn standard_stack_sets_and_propagates_request_ids() {
        let app = apply_standard_middleware(
            Router::new().route("/", get(|| async { "ok" })),
            &HttpRuntimeConfig::default(),
        )
        .unwrap();
        let response = app
            .oneshot(http::Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(response.headers().contains_key(&REQUEST_ID_HEADER));
    }

    #[tokio::test]
    async fn standard_stack_negotiates_gzip_for_large_responses() {
        let payload = large_compressible_body();
        let original_len = payload.len();
        let app = apply_standard_middleware(
            Router::new().route(
                "/",
                get(move || {
                    let payload = payload.clone();
                    async move { payload }
                }),
            ),
            &HttpRuntimeConfig::default(),
        )
        .unwrap();
        let response = app
            .oneshot(
                http::Request::get("/")
                    .header(header::ACCEPT_ENCODING, "gzip")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.headers().get(header::CONTENT_ENCODING),
            Some(&HeaderValue::from_static("gzip"))
        );
        let varies_by_encoding = response
            .headers()
            .get_all(header::VARY)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(','))
            .any(|value| value.trim().eq_ignore_ascii_case("accept-encoding"));
        assert!(varies_by_encoding);

        let encoded = response.into_body().collect().await.unwrap().to_bytes();
        assert!(encoded.starts_with(&[0x1f, 0x8b]));
        assert!(encoded.len() < original_len);
    }

    #[tokio::test]
    async fn standard_stack_does_not_compress_tiny_responses() {
        let app = apply_standard_middleware(
            Router::new().route("/", get(|| async { "small-response" })),
            &HttpRuntimeConfig::default(),
        )
        .unwrap();
        let response = app
            .oneshot(
                http::Request::get("/")
                    .header(header::ACCEPT_ENCODING, "gzip")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(!response.headers().contains_key(header::CONTENT_ENCODING));
    }

    #[tokio::test]
    async fn one_byte_below_the_threshold_stays_uncompressed() {
        let threshold = compression_threshold();
        let payload = body_of_exact_bytes(threshold - 1);
        let app = apply_standard_middleware(
            gzip_router_with(payload.clone()),
            &HttpRuntimeConfig::default(),
        )
        .unwrap();
        let response = gzip_response(app, Some("gzip")).await;

        assert!(!response.headers().contains_key(header::CONTENT_ENCODING));
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body.len(), threshold - 1);
        assert_eq!(&body[..], payload.as_bytes());
    }

    #[tokio::test]
    async fn responses_at_the_exact_threshold_are_gzip_compressed() {
        let threshold = compression_threshold();
        let payload = body_of_exact_bytes(threshold);
        let app =
            apply_standard_middleware(gzip_router_with(payload), &HttpRuntimeConfig::default())
                .unwrap();
        let response = gzip_response(app, Some("gzip")).await;

        assert_eq!(
            response.headers().get(header::CONTENT_ENCODING),
            Some(&HeaderValue::from_static("gzip"))
        );
        assert!(vary_accepts_encoding(&response));
        let encoded = response.into_body().collect().await.unwrap().to_bytes();
        assert!(encoded.starts_with(&[0x1f, 0x8b]));
    }

    #[tokio::test]
    async fn large_eligible_responses_stay_uncompressed_without_accept_encoding() {
        let payload = body_of_exact_bytes(compression_threshold() * 4);
        let app = apply_standard_middleware(
            gzip_router_with(payload.clone()),
            &HttpRuntimeConfig::default(),
        )
        .unwrap();
        let response = gzip_response(app, None).await;

        assert!(!response.headers().contains_key(header::CONTENT_ENCODING));
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], payload.as_bytes());
    }

    #[tokio::test]
    async fn unsupported_accept_encoding_yields_the_identity_representation() {
        let payload = body_of_exact_bytes(compression_threshold() * 4);
        let app = apply_standard_middleware(
            gzip_router_with(payload.clone()),
            &HttpRuntimeConfig::default(),
        )
        .unwrap();
        let response = gzip_response(app, Some("br")).await;

        assert!(!response.headers().contains_key(header::CONTENT_ENCODING));
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], payload.as_bytes());
    }

    #[tokio::test]
    async fn already_encoded_responses_are_not_recompressed() {
        fn precompressed(payload: Vec<u8>) -> Response {
            let mut response = Response::new(Body::from(payload));
            response
                .headers_mut()
                .insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
            response
        }

        let payload = body_of_exact_bytes(compression_threshold() * 2).into_bytes();
        let app = apply_standard_middleware(
            Router::new().route(
                "/",
                get(move || {
                    let payload = payload.clone();
                    async move { precompressed(payload) }
                }),
            ),
            &HttpRuntimeConfig::default(),
        )
        .unwrap();
        let response = gzip_response(app, Some("gzip")).await;

        assert_eq!(
            response.headers().get(header::CONTENT_ENCODING),
            Some(&HeaderValue::from_static("gzip"))
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            body.len(),
            compression_threshold() * 2,
            "an already encoded response must pass through unchanged"
        );
    }

    #[tokio::test]
    async fn default_content_type_exclusions_remain_composed() {
        fn typed_body(content_type: &'static str) -> Response {
            let mut response =
                Response::new(Body::from(body_of_exact_bytes(compression_threshold() * 2)));
            response
                .headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
            response
        }

        for content_type in ["image/png", "text/event-stream"] {
            let app = apply_standard_middleware(
                Router::new().route("/", get(move || async { typed_body(content_type) })),
                &HttpRuntimeConfig::default(),
            )
            .unwrap();
            let response = gzip_response(app, Some("gzip")).await;

            assert!(
                !response.headers().contains_key(header::CONTENT_ENCODING),
                "{content_type} must stay uncompressed"
            );
        }
    }

    #[tokio::test]
    async fn response_extension_disables_compression_for_one_response() {
        async fn sensitive_response() -> Response {
            let mut response = large_compressible_body().into_response();
            response.extensions_mut().insert(DisableResponseCompression);
            response
        }

        let app = apply_standard_middleware(
            Router::new().route("/", get(sensitive_response)),
            &HttpRuntimeConfig::default(),
        )
        .unwrap();
        let response = app
            .oneshot(
                http::Request::get("/")
                    .header(header::ACCEPT_ENCODING, "gzip")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(!response.headers().contains_key(header::CONTENT_ENCODING));
    }

    #[tokio::test]
    async fn the_response_extension_affects_only_its_own_response() {
        async fn sensitive_response() -> Response {
            let mut response = large_compressible_body().into_response();
            response.extensions_mut().insert(DisableResponseCompression);
            response
        }

        let app = apply_standard_middleware(
            Router::new()
                .route("/sensitive", get(sensitive_response))
                .route("/normal", get(|| async { large_compressible_body() })),
            &HttpRuntimeConfig::default(),
        )
        .unwrap();
        let disabled = app
            .clone()
            .oneshot(
                http::Request::get("/sensitive")
                    .header(header::ACCEPT_ENCODING, "gzip")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let compressed = app
            .oneshot(
                http::Request::get("/normal")
                    .header(header::ACCEPT_ENCODING, "gzip")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(!disabled.headers().contains_key(header::CONTENT_ENCODING));
        assert_eq!(
            compressed.headers().get(header::CONTENT_ENCODING),
            Some(&HeaderValue::from_static("gzip"))
        );
        assert!(
            disabled
                .headers()
                .keys()
                .all(|name| !name.as_str().contains("compression")),
            "the opt-out marker must not leak into response headers: {:?}",
            disabled.headers()
        );
    }

    #[tokio::test]
    async fn runtime_config_can_disable_response_compression_globally() {
        let config = HttpRuntimeConfig {
            compression: false,
            ..HttpRuntimeConfig::default()
        };
        let app = apply_standard_middleware(
            Router::new().route("/", get(|| async { large_compressible_body() })),
            &config,
        )
        .unwrap();
        let response = app
            .oneshot(
                http::Request::get("/")
                    .header(header::ACCEPT_ENCODING, "gzip")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(!response.headers().contains_key(header::CONTENT_ENCODING));
    }

    #[tokio::test]
    async fn credentialed_cors_uses_only_the_exact_configured_origin() {
        let config = HttpRuntimeConfig {
            allowed_origins: vec!["https://client.example".into()],
            allow_credentials: true,
            ..HttpRuntimeConfig::default()
        };
        let app =
            apply_standard_middleware(Router::new().route("/", get(|| async { "ok" })), &config)
                .unwrap();
        let response = app
            .oneshot(
                http::Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/")
                    .header(header::ORIGIN, "https://client.example")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&HeaderValue::from_static("https://client.example"))
        );
        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS),
            Some(&HeaderValue::from_static("true"))
        );
    }

    #[test]
    fn wildcard_origins_and_headers_fail_configuration() {
        let mut config = HttpRuntimeConfig {
            allowed_origins: vec!["*".into()],
            ..HttpRuntimeConfig::default()
        };
        assert!(matches!(
            apply_standard_middleware(Router::new(), &config),
            Err(HttpConfigurationError::WildcardOrigin)
        ));

        config.allowed_origins = vec!["https://client.example".into()];
        assert!(matches!(
            config
                .header_policy
                .allow_request_header(HeaderName::from_static("*")),
            Err(HttpConfigurationError::WildcardHeader)
        ));
    }

    #[test]
    fn default_policy_supports_conditional_requests_and_client_metadata() {
        let policy = HttpHeaderPolicy::default();
        assert_eq!(
            policy
                .allowed_request_headers()
                .iter()
                .map(HeaderName::as_str)
                .collect::<Vec<_>>(),
            [
                "authorization",
                "content-type",
                "idempotency-key",
                "if-match",
                "if-none-match",
                "x-request-id",
            ]
        );
        assert_eq!(
            policy
                .exposed_response_headers()
                .iter()
                .map(HeaderName::as_str)
                .collect::<Vec<_>>(),
            [
                "deprecation",
                "etag",
                "link",
                "location",
                "retry-after",
                "sunset",
                "www-authenticate",
                "x-request-id",
            ]
        );
    }

    #[tokio::test]
    async fn default_cors_applies_the_cross_client_header_policy() {
        let app = apply_standard_middleware(
            Router::new().route("/", get(|| async { "ok" })),
            &HttpRuntimeConfig::default(),
        )
        .unwrap();
        let preflight = app
            .clone()
            .oneshot(
                http::Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/")
                    .header(header::ORIGIN, "http://127.0.0.1:3000")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "PATCH")
                    .header(
                        header::ACCESS_CONTROL_REQUEST_HEADERS,
                        "if-match,if-none-match",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let allowed = preflight
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert_eq!(
            allowed.split(',').map(str::trim).collect::<Vec<_>>(),
            [
                "authorization",
                "content-type",
                "idempotency-key",
                "if-match",
                "if-none-match",
                "x-request-id",
            ]
        );

        let response = app
            .oneshot(
                http::Request::get("/")
                    .header(header::ORIGIN, "http://127.0.0.1:3000")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let exposed = response
            .headers()
            .get(header::ACCESS_CONTROL_EXPOSE_HEADERS)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert_eq!(
            exposed.split(',').map(str::trim).collect::<Vec<_>>(),
            [
                "deprecation",
                "etag",
                "link",
                "location",
                "retry-after",
                "sunset",
                "www-authenticate",
                "x-request-id",
            ]
        );
    }

    #[tokio::test]
    async fn default_policy_does_not_allow_plugin_specific_headers() {
        let app = apply_standard_middleware(
            Router::new().route("/", get(|| async { "ok" })),
            &HttpRuntimeConfig::default(),
        )
        .unwrap();
        let response = app
            .oneshot(
                http::Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/")
                    .header(header::ORIGIN, "http://127.0.0.1:3000")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                    .header(
                        header::ACCESS_CONTROL_REQUEST_HEADERS,
                        "x-minco-feedback-token",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let allowed = response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert!(!allowed.contains("x-minco-feedback-token"), "{allowed}");
    }
}
