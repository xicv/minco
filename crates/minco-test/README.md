# minco-test

In-process HTTP test utilities for Minco and Axum applications.

`TestClient` calls an `axum::Router` directly as a Tower service, avoiding a
socket while exercising the real router and middleware stack. The crate also
captures deterministic command evidence for quality and deployment gates.

```rust
use axum::{Router, routing::get};
use http::StatusCode;
use minco_test::TestClient;

# async fn check() {
let app = Router::new().route("/health", get(|| async { "ok" }));
let response = TestClient::new(app).get("/health").await;
response.assert_status(StatusCode::OK);
# }
```
