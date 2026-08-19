# minco-http

Axum and Tower delivery conventions for Minco applications.

It provides:

- exact-origin CORS configuration;
- conditional request headers and browser-visible resource metadata;
- request IDs and propagation;
- request-body and timeout limits;
- sensitive-header handling;
- tracing and negotiated response compression;
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

## Compression boundary

The standard stack enables negotiated, fastest-level gzip for eligible responses
whose known size is at least 1 KiB. Clients opt in through `Accept-Encoding`,
Tower HTTP adds `Content-Encoding: gzip` and `Vary: Accept-Encoding`, and
`lambda_http` carries the compressed bytes through API Gateway's binary Lambda
proxy response path. Tiny responses, images, gRPC, Server-Sent Events, already
encoded responses, and clients that do not advertise gzip remain uncompressed.

Applications can disable compression globally through
`HttpRuntimeConfig::compression`, or for one response by adding the explicit
marker below. Use the response-level marker when a response combines a secret
with attacker-controlled reflection or otherwise requires an unencoded body.

```rust
use axum::response::{IntoResponse, Response};
use minco_http::DisableResponseCompression;

fn secret_bearing_response() -> Response {
    let mut response = "sensitive response".into_response();
    response.extensions_mut().insert(DisableResponseCompression);
    response
}
```

Minco's static-site topology separately enables CloudFront automatic Brotli and
gzip compression. Dynamic Brotli/zstd, compressed request bodies, and a
CloudFront proxy in front of the API are not implicit defaults. See
[Compress HTTP delivery without adding a proxy](../../docs/how-to/http-compression.md)
for the AWS, performance, and security boundaries.

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
