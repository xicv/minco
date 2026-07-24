use axum::Router;
use http::{HeaderName, HeaderValue, Method, StatusCode, header};
use std::time::Duration;
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

#[derive(Debug, Clone)]
pub struct HttpRuntimeConfig {
    pub allowed_origins: Vec<String>,
    pub timeout: Duration,
    pub max_request_body_bytes: usize,
    pub compression: bool,
}

impl Default for HttpRuntimeConfig {
    fn default() -> Self {
        Self {
            allowed_origins: vec!["http://127.0.0.1:3000".into()],
            timeout: Duration::from_secs(15),
            max_request_body_bytes: 1024 * 1024,
            compression: true,
        }
    }
}

pub fn apply_standard_middleware(
    router: Router,
    config: &HttpRuntimeConfig,
) -> Result<Router, http::header::InvalidHeaderValue> {
    let origins = config
        .allowed_origins
        .iter()
        .map(|origin| HeaderValue::from_str(origin))
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
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            HeaderName::from_static("idempotency-key"),
            HeaderName::from_static("x-minco-subject"),
            HeaderName::from_static("x-minco-permissions"),
            REQUEST_ID_HEADER.clone(),
        ])
        .expose_headers([REQUEST_ID_HEADER.clone()]);

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
        .layer(SetSensitiveRequestHeadersLayer::new([
            header::AUTHORIZATION,
            header::COOKIE,
        ]))
        .layer(cors)
        .layer(TraceLayer::new_for_http());
    Ok(if config.compression {
        router.layer(CompressionLayer::new())
    } else {
        router
    })
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
}
