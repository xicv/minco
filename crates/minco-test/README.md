# minco-test

Public plugin conformance, in-process HTTP utilities, deterministic fixtures,
and command evidence for Minco applications and extensions.

## Plugin conformance

`PluginConformance` reads the package's archive-visible
`minco-plugin.json`, checks its Cargo packaging and provider boundary, and can
compare a linked descriptor or exercise a concrete plugin through registration,
graph construction, configuration, composition, and bounded provenance.

```rust
use minco_test::{ConformanceStatus, PluginConformance};
# use minco_core::Plugin;
# fn check<P: Plugin>(plugin: P) {
let report = PluginConformance::for_package(env!("CARGO_MANIFEST_DIR"))
    .with_plugin(plugin)
    .run();

report.assert_passed();
assert_eq!(report.assurance.provider_live, ConformanceStatus::NotRun);
# }
```

Conformance evidence strings are inert labels and are never executed. A passed
report covers the plugin contract only. Application readiness, provider/live
integration, and production readiness remain separate evidence boundaries.

See the [conformance reference](../../docs/reference/plugin-conformance.md) and
the standalone
[`third-party-minimal`](../../examples/plugins/third-party-minimal) fixture.

## HTTP and fixtures

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
