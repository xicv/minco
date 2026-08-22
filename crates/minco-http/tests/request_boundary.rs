use std::{
    convert::Infallible,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use axum::{
    Json, Router,
    body::Body,
    routing::{get, post},
};
use http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use minco_contract::{ContractAuthorizationAlternative, ContractAuthorizationPolicy};
use minco_contract::{ContractValidate, ContractValidationErrors};
use minco_http::{
    HttpRuntimeConfig, MAX_REQUEST_ID_BYTES, Principal, ProblemDetails, REQUEST_ID_HEADER,
    ValidatedJson, ValidatedPath, ValidatedQuery, apply_standard_middleware, authorize_operation,
    is_valid_request_id,
};
use serde::{Deserialize, Deserializer, Serialize};
use tower::ServiceExt as _;
use uuid::{Uuid, Version};

static JSON_DESERIALIZATIONS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CountedInput {
    name: String,
}

#[derive(Debug, Serialize)]
struct ExactlyOnceInput {
    name: String,
}

impl<'de> Deserialize<'de> for ExactlyOnceInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            name: String,
        }

        JSON_DESERIALIZATIONS.fetch_add(1, Ordering::SeqCst);
        let input = Input::deserialize(deserializer)?;
        Ok(Self { name: input.name })
    }
}

impl ContractValidate for CountedInput {
    fn validate_contract(&self, errors: &mut ContractValidationErrors) {
        if self.name.chars().count() < 2 {
            errors.at_field("name", |errors| {
                errors.add("must contain at least 2 characters");
            });
        }
    }
}

impl ContractValidate for ExactlyOnceInput {
    fn validate_contract(&self, _errors: &mut ContractValidationErrors) {}
}

#[derive(Debug, Deserialize)]
struct QueryInput {
    limit: u16,
}

impl ContractValidate for QueryInput {
    fn validate_contract(&self, errors: &mut ContractValidationErrors) {
        if self.limit == 0 {
            errors.at_field("limit", |errors| errors.add("must be at least 1"));
        }
    }
}

#[derive(Debug, Deserialize)]
struct PathInput {
    item_id: Uuid,
}

impl ContractValidate for PathInput {
    fn validate_contract(&self, _errors: &mut ContractValidationErrors) {}
}

fn app() -> Router {
    Router::new()
        .route(
            "/json",
            post(|ValidatedJson(input): ValidatedJson<CountedInput>| async move { Json(input) }),
        )
        .route(
            "/counted",
            post(
                |ValidatedJson(input): ValidatedJson<ExactlyOnceInput>| async move { Json(input) },
            ),
        )
        .route(
            "/native-json",
            post(|Json(input): Json<CountedInput>| async move { Json(input) }),
        )
        .route(
            "/query",
            get(
                |ValidatedQuery(input): ValidatedQuery<QueryInput>| async move {
                    input.limit.to_string()
                },
            ),
        )
        .route(
            "/path/{item_id}",
            get(
                |ValidatedPath(input): ValidatedPath<PathInput>| async move {
                    input.item_id.to_string()
                },
            ),
        )
}

fn standard_app(router: Router, body_limit: usize, timeout: Duration) -> Router {
    apply_standard_middleware(
        router,
        &HttpRuntimeConfig {
            max_request_body_bytes: body_limit,
            timeout,
            compression: false,
            ..HttpRuntimeConfig::default()
        },
    )
    .unwrap()
}

async fn problem(response: http::Response<Body>) -> ProblemDetails {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).expect("Problem Details JSON")
}

#[tokio::test]
async fn valid_json_is_deserialized_exactly_once() {
    JSON_DESERIALIZATIONS.store(0, Ordering::SeqCst);
    let response = app()
        .oneshot(
            Request::post("/counted")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"name":"valid"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(JSON_DESERIALIZATIONS.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn semantic_json_failures_are_bounded_422_problems_with_correlation() {
    let response = app()
        .oneshot(
            Request::post("/json")
                .header(header::CONTENT_TYPE, "application/json")
                .header(&REQUEST_ID_HEADER, "request-42")
                .body(Body::from(r#"{"name":"x"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response.headers()[&REQUEST_ID_HEADER], "request-42");
    let problem = problem(response).await;
    assert_eq!(problem.code, "validation_failed");
    assert_eq!(problem.request_id, "request-42");
    assert_eq!(
        problem.errors.get("name"),
        Some(&vec!["must contain at least 2 characters".to_owned()])
    );
}

#[tokio::test]
async fn json_rejections_use_the_stable_public_taxonomy_without_parser_details() {
    for (body, content_type, expected_status, expected_code) in [
        (
            "{",
            Some("application/json"),
            StatusCode::BAD_REQUEST,
            "invalid_json",
        ),
        (
            r#"{"name":4}"#,
            Some("application/json"),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            r#"{"name":"ok","unknown":true}"#,
            Some("application/json"),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            r#"{"name":null}"#,
            Some("application/json"),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            r#"{"name":"ok"}"#,
            None,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
        ),
    ] {
        let mut request = Request::post("/json").body(Body::from(body)).unwrap();
        if let Some(content_type) = content_type {
            request
                .headers_mut()
                .insert(header::CONTENT_TYPE, content_type.parse().unwrap());
        }
        let response = app().oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected_status);
        let problem = problem(response).await;
        assert_eq!(problem.code, expected_code);
        assert!(!problem.detail.contains("line"));
        assert!(!problem.detail.contains("CountedInput"));
    }
}

#[tokio::test]
async fn query_and_path_rejections_are_stable_and_semantic_query_errors_are_422() {
    let valid_query = app()
        .clone()
        .oneshot(Request::get("/query?limit=2").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(valid_query.status(), StatusCode::OK);

    let invalid_query = app()
        .clone()
        .oneshot(
            Request::get("/query?limit=nope")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(problem(invalid_query).await.code, "invalid_query");

    let semantic_query = app()
        .clone()
        .oneshot(Request::get("/query?limit=0").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(semantic_query.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(problem(semantic_query).await.code, "validation_failed");

    let invalid_path = app()
        .clone()
        .oneshot(
            Request::get("/path/not-a-uuid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(problem(invalid_path).await.code, "invalid_path");

    let valid_path = app()
        .oneshot(
            Request::get(format!("/path/{}", Uuid::now_v7()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(valid_path.status(), StatusCode::OK);
}

#[test]
fn request_id_grammar_is_bounded_ascii_and_correlation_safe() {
    assert!(is_valid_request_id("abc-DEF_123.:trace"));
    assert!(is_valid_request_id(&"a".repeat(MAX_REQUEST_ID_BYTES)));
    for invalid in [
        "",
        "has whitespace",
        "unicode-🦀",
        "control-\u{7f}",
        "slash/not-safe",
        "quote\"not-safe",
        &"a".repeat(MAX_REQUEST_ID_BYTES + 1),
    ] {
        assert!(!is_valid_request_id(invalid), "accepted {invalid:?}");
    }
}

#[tokio::test]
async fn unsafe_request_ids_are_replaced_with_uuid_v7_in_problem_body_and_header() {
    let response = app()
        .oneshot(
            Request::post("/json")
                .header(header::CONTENT_TYPE, "application/json")
                .header(&REQUEST_ID_HEADER, "unsafe request id")
                .body(Body::from("{"))
                .unwrap(),
        )
        .await
        .unwrap();
    let header_id = response.headers()[&REQUEST_ID_HEADER]
        .to_str()
        .unwrap()
        .to_owned();
    let problem = problem(response).await;

    assert_eq!(problem.request_id, header_id);
    assert_eq!(
        Uuid::parse_str(&header_id).unwrap().get_version(),
        Some(Version::SortRand)
    );
}

#[test]
fn application_created_failures_cannot_reflect_an_unsafe_request_id() {
    use axum::response::IntoResponse as _;

    let response = minco_http::ApiFailure::new(
        StatusCode::BAD_REQUEST,
        "application_failure",
        "Application failure",
        "The request was rejected.",
        "unsafe application request id",
    )
    .into_response();
    let response_id = response.headers()[&REQUEST_ID_HEADER].to_str().unwrap();

    assert!(is_valid_request_id(response_id));
    assert_ne!(response_id, "unsafe application request id");
}

#[test]
fn coarse_authorization_preserves_public_access_and_enforces_exact_permissions_and_scope_or() {
    const PUBLIC: ContractAuthorizationPolicy =
        ContractAuthorizationPolicy::new("health", true, &[], &[]);
    const PROTECTED: ContractAuthorizationPolicy = ContractAuthorizationPolicy::new(
        "writeWidget",
        false,
        &["widgets.write", "tenant.access"],
        &[
            ContractAuthorizationAlternative::new(&["widgets:write", "openid"]),
            ContractAuthorizationAlternative::new(&["admin"]),
        ],
    );
    let principal = Principal {
        subject: "user-1".into(),
        permissions: ["widgets.write", "tenant.access"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        claims: std::collections::BTreeMap::new(),
    }
    .with_scopes(["admin"]);

    assert!(authorize_operation(None, &PUBLIC, "request-1").is_ok());
    let unauthenticated = authorize_operation(None, &PROTECTED, "request-1").unwrap_err();
    assert_eq!(unauthenticated.status, StatusCode::UNAUTHORIZED);
    assert_eq!(unauthenticated.code.as_ref(), "unauthenticated");
    assert!(authorize_operation(Some(&principal), &PROTECTED, "request-1").is_ok());

    let missing_permission = Principal {
        subject: "user-permission".into(),
        permissions: std::iter::once("widgets.write")
            .map(str::to_owned)
            .collect(),
        claims: std::collections::BTreeMap::new(),
    }
    .with_scopes(["admin"]);
    assert!(authorize_operation(Some(&missing_permission), &PROTECTED, "request-1").is_err());

    let incomplete_scope_alternative = Principal {
        subject: "user-scope".into(),
        permissions: ["widgets.write", "tenant.access"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        claims: std::collections::BTreeMap::new(),
    }
    .with_scopes(["widgets:write"]);
    assert!(
        authorize_operation(Some(&incomplete_scope_alternative), &PROTECTED, "request-1").is_err()
    );
    let complete_scope_alternative =
        incomplete_scope_alternative.with_scopes(["widgets:write", "openid"]);
    assert!(
        authorize_operation(Some(&complete_scope_alternative), &PROTECTED, "request-1").is_ok()
    );

    let substring_only = Principal {
        subject: "user-2".into(),
        permissions: ["widgets.write.extra", "tenant.access"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        claims: std::collections::BTreeMap::new(),
    }
    .with_scopes(["administrator"]);
    let forbidden =
        authorize_operation(Some(&substring_only), &PROTECTED, "request-1").unwrap_err();
    assert_eq!(forbidden.status, StatusCode::FORBIDDEN);
    assert_eq!(forbidden.code.as_ref(), "forbidden");
}

#[test]
fn unauthenticated_failures_include_the_standard_bearer_challenge() {
    use axum::response::IntoResponse as _;

    const POLICY: ContractAuthorizationPolicy = ContractAuthorizationPolicy::new(
        "readWidget",
        false,
        &[],
        &[ContractAuthorizationAlternative::new(&[])],
    );
    let failure = authorize_operation(None, &POLICY, "request-1").unwrap_err();
    assert_eq!(failure.code.as_ref(), "unauthenticated");
    let response = failure.into_response();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers()[header::WWW_AUTHENTICATE], "Bearer");
}

#[tokio::test]
async fn configured_body_limit_accepts_exact_bytes_and_rejects_one_more() {
    let payload = r#"{"name":"valid"}"#;
    let exact = standard_app(app(), payload.len(), Duration::from_secs(1))
        .oneshot(
            Request::post("/json")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(exact.status(), StatusCode::OK);

    let over = standard_app(app(), payload.len() - 1, Duration::from_secs(1))
        .oneshot(
            Request::post("/json")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(over.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(problem(over).await.code, "payload_too_large");
}

#[tokio::test]
async fn declared_and_streamed_oversize_bodies_share_the_minco_413_boundary() {
    let declared = standard_app(app(), 16, Duration::from_secs(1))
        .oneshot(
            Request::post("/json")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::CONTENT_LENGTH, "17")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(problem(declared).await.code, "payload_too_large");

    let chunks = futures::stream::iter([
        Ok::<_, Infallible>(axum::body::Bytes::from_static(b"{\"name\":")),
        Ok::<_, Infallible>(axum::body::Bytes::from_static(b"\"valid\"}")),
    ]);
    let streamed = standard_app(app(), 8, Duration::from_secs(1))
        .oneshot(
            Request::post("/json")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from_stream(chunks))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(streamed.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(problem(streamed).await.code, "payload_too_large");

    let native_chunks = futures::stream::iter([
        Ok::<_, Infallible>(axum::body::Bytes::from_static(b"{\"name\":")),
        Ok::<_, Infallible>(axum::body::Bytes::from_static(b"\"valid\"}")),
    ]);
    let native = standard_app(app(), 8, Duration::from_secs(1))
        .oneshot(
            Request::post("/native-json")
                .header(header::CONTENT_TYPE, "application/json")
                .header(&REQUEST_ID_HEADER, "native-stream-1")
                .body(Body::from_stream(native_chunks))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(native.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(native.headers()[&REQUEST_ID_HEADER], "native-stream-1");
    let native_problem = problem(native).await;
    assert_eq!(native_problem.code, "payload_too_large");
    assert_eq!(native_problem.request_id, "native-stream-1");
}

#[tokio::test]
async fn malformed_content_length_cannot_disable_the_streamed_limit() {
    let accepted = standard_app(app(), 32, Duration::from_secs(1))
        .oneshot(
            Request::post("/json")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::CONTENT_LENGTH, "malformed")
                .body(Body::from(r#"{"name":"valid"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(accepted.status(), StatusCode::OK);

    let rejected = standard_app(app(), 8, Duration::from_secs(1))
        .oneshot(
            Request::post("/json")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::CONTENT_LENGTH, "malformed")
                .body(Body::from(r#"{"name":"valid"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(problem(rejected).await.code, "payload_too_large");
}

#[tokio::test]
async fn minco_timeout_is_a_correlated_problem_and_fast_handlers_complete() {
    let router = Router::new()
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_millis(30)).await;
                "late"
            }),
        )
        .route("/fast", get(|| async { "ready" }));
    let app = standard_app(router, 128, Duration::from_millis(5));
    let timed_out = app
        .clone()
        .oneshot(
            Request::get("/slow")
                .header(&REQUEST_ID_HEADER, "timeout-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(timed_out.status(), StatusCode::REQUEST_TIMEOUT);
    assert_eq!(timed_out.headers()[&REQUEST_ID_HEADER], "timeout-1");
    assert_eq!(problem(timed_out).await.code, "request_timeout");

    let fast = app
        .oneshot(Request::get("/fast").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(fast.status(), StatusCode::OK);
}

#[tokio::test]
async fn application_owned_408_and_413_responses_are_preserved_byte_for_byte() {
    let router = Router::new().route(
        concat!("/owned/", "{status}"),
        get(
            |axum::extract::Path(status): axum::extract::Path<u16>| async move {
                http::Response::builder()
                    .status(status)
                    .header(header::CONTENT_TYPE, "application/vnd.example+text")
                    .header("x-application-owned", "yes")
                    .body(Body::from(format!("application-{status}")))
                    .unwrap()
            },
        ),
    );
    let app = standard_app(router, 128, Duration::from_secs(1));

    for status in [408, 413] {
        let response = app
            .clone()
            .oneshot(
                Request::get(format!("/owned/{status}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "application/vnd.example+text"
        );
        assert_eq!(response.headers()["x-application-owned"], "yes");
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body, format!("application-{status}").as_bytes());
    }
}

#[tokio::test]
async fn standard_stack_normalizes_request_ids_before_the_handler_observes_them() {
    let router = Router::new().route(
        "/id",
        get(|headers: http::HeaderMap| async move {
            headers
                .get(&REQUEST_ID_HEADER)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("missing")
                .to_owned()
        }),
    );
    let response = standard_app(router, 128, Duration::from_secs(1))
        .oneshot(
            Request::get("/id")
                .header(&REQUEST_ID_HEADER, "unsafe request id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let response_id = response.headers()[&REQUEST_ID_HEADER]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(is_valid_request_id(&response_id));
    assert_ne!(response_id, "unsafe request id");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body, response_id.as_bytes());
}
