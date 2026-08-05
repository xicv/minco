# Using Minco in an application

The `minco` facade is the normal dependency for an application. It keeps the
provider-neutral kernel always available and exposes contract, HTTP, plugins,
databases, Lambda, planning, release, and test support through explicit Cargo
features.

## 0. Generate a layered application

Install the control plane and create a project:

```bash
cargo install cargo-minco --locked
cargo minco new example-api --database postgres
cd example-api
cp .env.example .env
```

The generator writes real domain, application, adapter, API, local runtime,
Lambda runtime, migration, OpenAPI, test, roadmap, task, and quality-gate files.
JJ with colocated Git is initialized by default; use `--vcs none` only when a
different repository workflow is intentional. Select `--database sqlite` for a
persistent local/single-host SQLite profile.

The generated workspace is a starting point, not hidden runtime magic. Every
file is ordinary Rust/TOML/YAML that can be inspected and changed.

## 1. Add the dependency manually

```bash
cargo add minco
```

The default features are intentionally small:

```text
contract
http
default-plugins
  health
  observability
  idempotency
```

Choose persistence and deployment capabilities explicitly:

```bash
cargo add minco --features sqlx-postgres,aws-lambda,plan,release,test
```

For an SQLite application:

```bash
cargo add minco --features sqlx-sqlite,test
```

To use only the provider-neutral extension kernel:

```bash
cargo add minco --no-default-features
```

## 2. Compose official plugins

```rust
use minco::prelude::*;

fn compose() -> Result<ComposedApplication, PluginError> {
    let mut selection = PluginSelection::default();

    // Runtime selection can disable a plugin that is compiled into the binary.
    selection
        .disabled
        .insert(PluginId::new("idempotency")?);

    minco::compose_defaults(&selection)
}
```

Cargo features decide what is compiled. `PluginSelection` decides which compiled
plugins are installed for a particular application/environment. Application
code can register third-party `Plugin` implementations in the same
`PluginManager` before composition.

## 3. Build the Axum router directly

Minco does not hide Axum. Build normal Axum routers and apply Minco's standard
Tower policy at the delivery boundary:

```rust
use axum::{Router, routing::get};
use minco::http::{HttpRuntimeConfig, apply_standard_middleware};

fn router() -> Result<Router, minco::http::HttpConfigurationError> {
    let app = Router::new().route("/health/live", get(|| async { "ok" }));
    apply_standard_middleware(app, &HttpRuntimeConfig::default())
}
```

Business use cases should live in application/domain crates. HTTP handlers map
contract DTOs to a use case and map the result back to the contract. SQLx and
AWS SDK calls stay in adapters.

Applications extend `HttpRuntimeConfig::header_policy` with exact
application-owned headers. Installed HTTP plugins contribute their own exact
request, exposed and sensitive headers through `HttpModule`; plugin-specific
headers are absent when that plugin is not selected. Wildcard origins and
headers fail configuration.

## 4. Add a project manifest for `cargo minco`

Install the CLI:

```bash
cargo install cargo-minco --locked
```

When assembling a workspace manually, create `minco.toml` in the application
root. Generated projects already contain it. Paths are application-relative;
the CLI contains no Orders-specific path assumptions.

```toml
schema = 1
name = "example-api"
contract = "openapi/openapi.yaml"
generated = "crates/api/src/generated.rs"
deployment_config = "environments/dev.toml"
roadmap = "roadmap/roadmap.yaml"
tasks = "tasks"
plugin_catalog = "plugins/catalog.toml"
quality = "quality.toml"

[architecture]
domain_roots = ["crates/domain"]
application_roots = ["crates/application"]
api_roots = ["crates/api"]

[migrations]
roots = ["migrations/postgres"]

[plugins]
enabled = ["health", "observability", "idempotency"]
disabled = []

[operations.createWidget]
handler = "crates/api/src/widgets/create.rs#create_widget"
application = "crates/application/src/widgets/create.rs#CreateWidget"
adapters = ["postgres"]
tests = ["tests/widgets/create_widget.rs"]
```

Then run:

```bash
cargo minco doctor
cargo minco contract check
cargo minco contract sync
cargo minco inspect --json
cargo minco explain createWidget --json
cargo minco check --with-cargo
cargo minco db plan --set example-api-postgres --json
```

Each migration root also needs a `.minco-migrations.toml` sidecar describing
its stable ID, owner, backend, history table, verification tables and
per-version risk. See
[`../deployment/database-lifecycle.md`](../deployment/database-lifecycle.md).

## 5. Select a database deployment profile

The runtime implementation and deployment profile are separate decisions:

- `sqlx-postgres` supports Neon, self-hosted PostgreSQL, RDS PostgreSQL, and
  Aurora PostgreSQL connection models;
- `sqlx-sqlite` supports local, desktop, and persistent single-process hosts;
- DynamoDB requires a purpose-built access-pattern adapter rather than a false
  relational abstraction.

Use `minco-plan` through the facade's `plan` feature to classify fixed capacity,
scheduled wakeups, database connection pressure, and incomplete rate cards.

## 6. Local and Lambda entrypoints

Build one `Router` in a library crate. The local binary binds it with
`axum::serve`; the Lambda binary passes the same router to
`minco::aws_lambda`. This keeps request behavior shared while leaving runtime
assembly explicit.

SQS-triggered functions use the separate `aws-worker` feature. It does not
depend on the AWS SDK and does not create queues, event-source mappings or
schedules. The mapping must enable `ReportBatchItemFailures`.

## API stability

Published baseline: `0.6.0`
Current workspace version: `1.0.0`
Workspace release state: `candidate`

Pin the published minor line in production applications and follow
`docs/adoption/incremental-adoption.md` plus the versioned upgrade guide before
upgrading. The framework follows lock-step versions across the publishable
crate family during the initial stabilization period. Registry availability
must still be checked independently for every later release.
