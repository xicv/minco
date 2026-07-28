# minco-test

In-process HTTP test utilities for Minco and Axum applications.

`TestClient` calls an `axum::Router` directly as a Tower service, avoiding a
socket while exercising the real router and middleware stack. The crate also
captures deterministic command evidence for quality and deployment gates.

`FixtureSequence` supplies stable test identities without an ORM, clock,
randomness or global state:

```rust
use minco_test::FixtureSequence;

let mut fixtures = FixtureSequence::new("orders")?;
let order_id = fixtures.next("order")?.stable_id;
assert_eq!(order_id, "orders:order:00000001");
# Ok::<(), minco_test::FixtureError>(())
```

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
