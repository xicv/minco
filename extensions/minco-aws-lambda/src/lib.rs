//! Native Lambda HTTP runtime, API Gateway principal mapping and SSM configuration loading.
#![forbid(unsafe_code)]

use anyhow::{Context, Result};
use axum::{Router, extract::Request, middleware::Next, response::Response};
use http::Uri;
use lambda_http::{RequestExt, request::RequestContext};
use minco_http::Principal;
use std::collections::{BTreeMap, BTreeSet};
use tower::ServiceExt;

pub async fn run_router(router: Router) -> Result<()> {
    let service = lambda_http::service_fn(move |request: lambda_http::Request| {
        route_request(router.clone(), request)
    });
    lambda_http::run(service)
        .await
        .map_err(|error| anyhow::anyhow!("Lambda HTTP runtime failed: {error}"))
}

async fn route_request(
    router: Router,
    mut request: lambda_http::Request,
) -> std::result::Result<Response, std::convert::Infallible> {
    strip_api_gateway_stage_from_uri(&mut request);
    router.oneshot(request).await
}

fn strip_api_gateway_stage_from_uri(request: &mut lambda_http::Request) -> bool {
    let Some(RequestContext::ApiGatewayV2(context)) = request.request_context_ref() else {
        return false;
    };
    let Some(stage) = context.stage.as_deref() else {
        return false;
    };
    if stage.is_empty() || stage == "$default" {
        return false;
    }

    // API Gateway's original `rawPath` is the routing source of truth. `lambda_http`
    // may already have prepended the named stage while constructing the URI, including
    // for raw paths that only resemble the stage prefix.
    let raw_path = request.raw_http_path().to_owned();
    let source_path = if raw_path.is_empty() {
        request.uri().path()
    } else {
        &raw_path
    };
    let prefix = format!("/{stage}");
    let normalized_path = if source_path == prefix {
        "/"
    } else if let Some(suffix) = source_path.strip_prefix(&prefix) {
        if suffix.starts_with('/') {
            suffix
        } else {
            source_path
        }
    } else {
        source_path
    };

    if normalized_path == request.uri().path() {
        return false;
    }
    let normalized_path_and_query = match request.uri().query() {
        Some(query) => format!("{normalized_path}?{query}"),
        None => normalized_path.to_owned(),
    };
    let Ok(path_and_query) = normalized_path_and_query.parse() else {
        return false;
    };
    let mut parts = request.uri().clone().into_parts();
    parts.path_and_query = Some(path_and_query);
    let Ok(uri) = Uri::from_parts(parts) else {
        return false;
    };
    *request.uri_mut() = uri;
    true
}

pub async fn inject_api_gateway_principal(mut request: Request, next: Next) -> Response {
    if let Some(principal) = principal_from_request_context(request.request_context_ref()) {
        request.extensions_mut().insert(principal);
    }
    next.run(request).await
}

#[must_use]
pub fn principal_from_request_context(context: Option<&RequestContext>) -> Option<Principal> {
    let RequestContext::ApiGatewayV2(context) = context? else {
        return None;
    };
    let authorizer = context.authorizer.as_ref()?;
    let value = serde_json::to_value(authorizer).ok()?;
    let claims = value
        .pointer("/jwt/claims")
        .or_else(|| value.get("claims"))?
        .as_object()?;
    principal_from_claims(claims)
}

fn principal_from_claims(claims: &serde_json::Map<String, serde_json::Value>) -> Option<Principal> {
    let subject = claims.get("sub")?.as_str()?.trim();
    if subject.is_empty() {
        return None;
    }
    let claims = claims
        .iter()
        .filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_owned())))
        .collect::<BTreeMap<_, _>>();
    let mut permissions = BTreeSet::new();
    for claim in ["scope", "permissions", "custom:permissions"] {
        if let Some(value) = claims.get(claim) {
            permissions.extend(
                value
                    .split([',', ' '])
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
            );
        }
    }
    Some(Principal {
        subject: subject.to_owned(),
        permissions,
        claims,
    })
}

pub async fn load_secure_parameter(name: &str) -> Result<String> {
    if name.trim().is_empty() {
        anyhow::bail!("SSM parameter name is empty");
    }
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let response = aws_sdk_ssm::Client::new(&config)
        .get_parameter()
        .name(name)
        .with_decryption(true)
        .send()
        .await
        .with_context(|| format!("failed to load SSM parameter {name}"))?;
    response
        .parameter
        .and_then(|parameter| parameter.value)
        .filter(|value| !value.is_empty())
        .with_context(|| format!("SSM parameter {name} has no value"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        http::{HeaderValue, StatusCode, header},
        routing::get,
    };

    fn gateway_request(uri: &str, stage: Option<&str>) -> lambda_http::Request {
        let uri = uri.parse::<Uri>().expect("request URI is valid");
        let path = uri.path();
        let query = uri.query().unwrap_or_default();
        let event = serde_json::json!({
            "version": "2.0",
            "routeKey": "GET /health/live",
            "rawPath": path,
            "rawQueryString": query,
            "headers": {
                "host": "example.execute-api.invalid"
            },
            "requestContext": {
                "accountId": "123456789012",
                "apiId": "api-id",
                "domainName": "example.execute-api.invalid",
                "domainPrefix": "example",
                "http": {
                    "method": "GET",
                    "path": path,
                    "protocol": "HTTP/1.1",
                    "sourceIp": "127.0.0.1",
                    "userAgent": "minco-test"
                },
                "requestId": "request-id",
                "routeKey": "GET /health/live",
                "stage": stage,
                "time": "30/Jul/2026:09:06:25 +0000",
                "timeEpoch": 1_785_402_385_000_u64
            },
            "isBase64Encoded": false
        });
        lambda_http::request::from_str(&event.to_string()).expect("API Gateway v2 event is valid")
    }

    #[test]
    fn absent_gateway_context_is_anonymous() {
        assert!(principal_from_request_context(None).is_none());
    }

    #[test]
    fn non_gateway_requests_are_not_rewritten() {
        let mut request = http::Request::builder()
            .uri("/candidate/health/live")
            .body(lambda_http::Body::Empty)
            .expect("request is valid");

        assert!(!strip_api_gateway_stage_from_uri(&mut request));
        assert_eq!(request.uri().path(), "/candidate/health/live");
    }

    #[test]
    fn maps_locked_cognito_permission_attributes() {
        let claims = serde_json::json!({
            "sub": "smoke-user",
            "custom:permissions": "orders.create orders.read",
            "aud": "client-id"
        });
        let principal =
            principal_from_claims(claims.as_object().expect("claims")).expect("principal");
        assert_eq!(principal.subject, "smoke-user");
        assert!(principal.permissions.contains("orders.create"));
        assert!(principal.permissions.contains("orders.read"));
    }

    #[test]
    fn strips_the_exact_named_stage_before_axum_routing() {
        let mut request = gateway_request(
            "https://example.execute-api.invalid/candidate/health/live?probe=1",
            Some("candidate"),
        );

        assert!(strip_api_gateway_stage_from_uri(&mut request));
        assert_eq!(request.uri().path(), "/health/live");
        assert_eq!(request.uri().query(), Some("probe=1"));
        assert_eq!(
            request.uri().authority().map(http::uri::Authority::as_str),
            Some("example.execute-api.invalid")
        );
    }

    #[test]
    fn named_stage_normalization_is_boundary_safe() {
        let mut root = gateway_request("/candidate?probe=1", Some("candidate"));
        assert!(strip_api_gateway_stage_from_uri(&mut root));
        assert_eq!(
            root.uri()
                .path_and_query()
                .map(http::uri::PathAndQuery::as_str),
            Some("/?probe=1")
        );

        let mut different_prefix = gateway_request("/candidate-v2/health/live", Some("candidate"));
        assert_eq!(
            different_prefix.uri().path(),
            "/candidate/candidate-v2/health/live"
        );
        assert!(strip_api_gateway_stage_from_uri(&mut different_prefix));
        assert_eq!(different_prefix.uri().path(), "/candidate-v2/health/live");

        let mut unprefixed = gateway_request("/health/live", Some("candidate"));
        assert_eq!(unprefixed.uri().path(), "/candidate/health/live");
        assert!(strip_api_gateway_stage_from_uri(&mut unprefixed));
        assert_eq!(unprefixed.uri().path(), "/health/live");

        let mut default_stage = gateway_request("/health/live", Some("$default"));
        assert!(!strip_api_gateway_stage_from_uri(&mut default_stage));
        assert_eq!(default_stage.uri().path(), "/health/live");
    }

    #[tokio::test]
    async fn named_stage_is_removed_before_axum_route_matching() {
        let router = Router::new().route("/health/live", get(|| async { StatusCode::NO_CONTENT }));
        let request = gateway_request("/candidate/health/live", Some("candidate"));
        assert_eq!(request.uri().path(), "/candidate/health/live");

        let response = route_request(router, request)
            .await
            .expect("router service is infallible");

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn compressed_response_uses_lambda_binary_transport() {
        let router = minco_http::apply_standard_middleware(
            Router::new().route(
                "/payload",
                get(|| async { "minco-lambda-compression-".repeat(128) }),
            ),
            &minco_http::HttpRuntimeConfig::default(),
        )
        .expect("standard HTTP middleware is valid");
        let mut request = gateway_request("/payload", Some("$default"));
        request.headers_mut().insert(
            header::ACCEPT_ENCODING,
            HeaderValue::from_static("gzip"),
        );

        let response = route_request(router, request)
            .await
            .expect("router service is infallible");
        assert_eq!(
            response.headers().get(header::CONTENT_ENCODING),
            Some(&HeaderValue::from_static("gzip"))
        );

        let response = lambda_http::IntoResponse::into_response(response).await;
        match response.body() {
            lambda_http::Body::Binary(bytes) => {
                assert!(bytes.starts_with(&[0x1f, 0x8b]));
            }
            body => panic!("compressed Lambda response was not binary: {body:?}"),
        }
    }
}
