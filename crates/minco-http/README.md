# minco-http

Axum and Tower delivery conventions for Minco applications.

It provides:

- exact-origin CORS configuration;
- conditional request headers and browser-visible resource metadata;
- request IDs and propagation;
- request-body and timeout limits;
- sensitive-header handling;
- tracing and optional compression;
- provider-neutral principals;
- RFC 9457-style Problem Details responses; and
- typed `Retry-After`, bearer challenge, deprecation, sunset, and link metadata.

```rust
use axum::{Router, routing::get};
use minco_http::{HttpConfigurationError, HttpRuntimeConfig, apply_standard_middleware};

let router = Router::new().route("/health/live", get(|| async { "ok" }));
let router = apply_standard_middleware(router, &HttpRuntimeConfig::default())?;
# Ok::<(), HttpConfigurationError>(())
```

Response metadata composes with any Axum response without changing application
authorization or lifecycle policy:

```rust
use axum::response::IntoResponse;
use http::StatusCode;
use minco_http::{ApiFailure, ApiResponseMetadata, BearerChallenge};

let response = ApiResponseMetadata::new()
    .bearer_challenge(BearerChallenge::InvalidToken)
    .wrap(ApiFailure::new(
        StatusCode::UNAUTHORIZED,
        "invalid_token",
        "Invalid access token",
        "The access token is missing, expired, or invalid.",
        "request-1",
    ))
    .into_response();

assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
```

See [Serve browser and native clients from one API](../../docs/how-to/mobile-api.md)
for authentication, retries, compatibility, and device-integrity guidance.
