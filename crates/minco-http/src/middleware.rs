use crate::response::{DEPRECATION_HEADER, SUNSET_HEADER};
use axum::Router;
use http::{HeaderName, HeaderValue, Method, StatusCode, header};
use std::{collections::BTreeMap, str::FromStr, time::Duration};
use thiserror::Error;
use tower_http::{
    compression::CompressionLayer,
    cors::{AllowOrigin, CorsLayer},
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    sensitive_headers::SetSensitiveRequestHeadersLayer,
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

pub static REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");
pub static CSRF_HEADER: HeaderName = HeaderName::from_static("x-minco-csrf");

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

    let router = router
        .layer(RequestBodyLimitLayer::new(config.max_request_body_bytes))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            config.timeout,
        ))
        .layer(PropagateRequestIdLayer::new(REQUEST_ID_HEADER.clone()))
        .layer(SetRequestIdLayer::new(
            REQUEST_ID_HEADER.clone(),
            MakeRequestUuid,
        ))
        .layer(SetSensitiveRequestHeadersLayer::new(
            config.header_policy.sensitive_request_headers(),
        ))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    Ok(if config.compression {
        router.layer(CompressionLayer::new())
    } else {
        router
    })
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
    use axum::{body::Body, routing::get};
    use tower::ServiceExt;

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
        let allowed = policy.allowed_request_headers();
        for name in [header::IF_MATCH, header::IF_NONE_MATCH] {
            assert!(allowed.contains(&name), "missing request header {name:?}");
        }

        let exposed = policy.exposed_response_headers();
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
            assert!(exposed.contains(&name), "missing response header {name:?}");
        }
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
        for expected in ["if-match", "if-none-match"] {
            assert!(
                allowed
                    .split(',')
                    .map(str::trim)
                    .any(|value| value.eq_ignore_ascii_case(expected)),
                "missing {expected} in {allowed}"
            );
        }

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
        for expected in ["etag", "retry-after", "www-authenticate", "deprecation", "sunset"] {
            assert!(
                exposed
                    .split(',')
                    .map(str::trim)
                    .any(|value| value.eq_ignore_ascii_case(expected)),
                "missing {expected} in {exposed}"
            );
        }
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
