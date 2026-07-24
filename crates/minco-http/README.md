# minco-http

Axum and Tower delivery conventions for Minco applications.

It provides:

- exact-origin CORS configuration;
- request IDs and propagation;
- request-body and timeout limits;
- sensitive-header handling;
- tracing and optional compression;
- provider-neutral principals;
- RFC 9457-style Problem Details responses.

```rust
use axum::{Router, routing::get};
use minco_http::{HttpRuntimeConfig, apply_standard_middleware};

let router = Router::new().route("/health/live", get(|| async { "ok" }));
let router = apply_standard_middleware(router, &HttpRuntimeConfig::default())?;
# Ok::<(), http::header::InvalidHeaderValue>(())
```
