# Incrementally adopting Minco

Published baseline: `0.6.0`
Current workspace version: `0.6.0`
Workspace release state: `published`

Minco is designed so an application can adopt one boundary at a time. Do not
start with `features = ["full"]`; select the smallest capability that closes a
real application problem and retain ordinary Rust composition around it.

## Recommended sequence

### 1. Contract tooling only

```toml
[dependencies]
minco = { version = "0.6.0", default-features = false, features = ["contract"] }
```

Adopt canonical OpenAPI, stable operation IDs, Problem Details and deterministic
bindings without changing the runtime. Run contract check/sync in CI.

### 2. Provider-neutral composition

Use `minco-core` directly or the no-default facade to register a small static
plugin graph. Keep product use cases and ports in application crates. Start
with fake/memory implementations only for tests and local development.

After composition, inspect `ComposedApplication::registration_provenance()` or
`cargo minco inspect --json` to confirm which application/plugin owner supplied
each singleton and ordered contribution. The output is bounded metadata; it
does not serialize the registered adapters or their configuration.

### 3. Exact HTTP policy

```rust
use minco::http::{HttpHeaderPolicy, HttpRuntimeConfig};

let mut headers = HttpHeaderPolicy::default();
headers.allow_request_header_name("x-application-tenant")?;
headers.mark_request_header_name_sensitive("x-application-tenant")?;

let config = HttpRuntimeConfig {
    header_policy: headers,
    ..HttpRuntimeConfig::default()
};
# Ok::<(), minco::http::HttpConfigurationError>(())
```

Minco's built-ins allow `Authorization`, `Content-Type`, `Idempotency-Key` and
`X-Request-Id`, expose the request ID, and redact authorization/cookie/
idempotency values. Cookie-backed sessions call `enable_cookie_csrf`.
HTTP-capable plugins contribute their own exact additions through `HttpModule`;
Feedback headers do not exist when Feedback is absent.

Deployment configuration owns the corresponding ingress projection. Add the
same exact request-header set to `allowed_headers`; Plan validation rejects
wildcards, invalid names and duplicates, then renders the normalized set into
SAM. Runtime and ingress policy should therefore be reviewed together whenever
a plugin changes its header contribution.

### 4. One persistence adapter

Enable exactly one of `sqlx-postgres` or `sqlx-sqlite`. Keep migrations explicit
and use application-owned ports. Do not treat DynamoDB as a relational drop-in.
For Lambda/PostgreSQL, record:

```text
maximum function concurrency × pool maximum <= database connection budget
```

### 5. Select one runtime

For HTTP Lambda:

```toml
features = ["contract", "http", "aws-lambda"]
```

For an SQS worker:

```toml
features = ["aws-worker"]
```

The worker returns partial-batch failures, defaults to sequential processing,
bounds optional concurrency, and fails forward for FIFO ordering. The
event-source mapping must explicitly set
`FunctionResponseTypes: [ReportBatchItemFailures]`; Minco does not create a
queue or schedule.

### 6. Add plugins only with provider and failure policy

For every selected plugin, record its stability, implementation, persistence,
IAM, retention, cost, retry/DLQ, health and conformance evidence. Memory
implementations are reference/test adapters, not production durability.

## Facade compatibility matrix

| Need | Feature | Default | Provider/runtime dependencies |
|---|---|---:|---|
| Typed plugin graph | always present | yes | none |
| OpenAPI tools | `contract` | yes | none |
| Axum/Tower conventions | `http` | yes | Axum/Tower only |
| Health/observability/idempotency | `default-plugins` | yes | none |
| All official plugin contracts | `official-plugins` | no | provider-neutral |
| PostgreSQL | `sqlx-postgres` | no | SQLx/PostgreSQL |
| SQLite | `sqlx-sqlite` | no | SQLx/SQLite |
| AWS provider adapters | `aws-adapters` | no | selected AWS SDK clients |
| HTTP Lambda | `aws-lambda` | no | Lambda HTTP plus SSM SDK |
| SQS Lambda worker | `aws-worker` | no | Lambda runtime/events, no AWS SDK |
| Plan/release/test support | `plan`, `release`, `test` | no | selected tooling |

Static validation rejects SQLx, Lambda and AWS feature leakage into the default
facade. Verify the resolved graph in the consuming application with
`cargo tree -e features`.

## Stability policy

- `stable`: the declared bounded contract and required gates pass. Pre-1.0
  minor releases may still contain documented breaking changes.
- `beta`: opt-in, usable with pinned versions, but provider or API boundaries
  may change after measured adoption.
- `planned`: do not depend on it; the task exists to make scope and acceptance
  explicit.

Catalog `kind` distinguishes true plugins from adapters and runtimes. Catalog
stability is validated against runtime descriptors for true plugins.

## Upgrade notes: `0.3.0` to `0.3.1`

- A Feedback configuration with `max_attachments = 0` is now enforced as a
  text-only profile in both the widget and multipart request validation.
- PostgreSQL-only consumers no longer resolve SQLite SQLx packages, and
  SQLite-only consumers no longer resolve PostgreSQL SQLx packages. Memory and
  no-default consumers remain SQLx-free.
- Public Rust APIs and serialized contracts are unchanged. The multi-runtime
  Plan IR redesign is not part of this patch.

Applications moving from `0.3.1` to the `0.4.0` family must use the dedicated
[`0.3.1` to `0.4.0` guide](0.3.1-to-0.4.0.md); it covers Plan IR schema 2,
typed configuration, database/dev/generator lifecycle, deployment receipts,
hosted verification and the four new crates.

The published `0.5.0` release is documented separately in the
[`0.4.0` to `0.5.0` guide](0.4.0-to-0.5.0.md). It covers the opt-in resource
wire contract, pagination/concurrency/idempotency behavior, zero-idle cost
evidence and local/hosted qualification split.

Applications adopting the published `0.6.0` plugin metadata and conformance
APIs must use the
[`0.5.0` to `0.6.0` guide](0.5.0-to-0.6.0.md). Update the exact lock-step
family together.

## Upgrade notes: `0.2.0` to `0.3.0`

- Plugin registration provenance is retained after composition. Normal chained
  `context.services().insert(...)` and
  `context.contributions().push(...)` call sites are unchanged, but the
  context accessors now return owner-bound registrar views. Code with explicit
  mutable-collection type annotations must accept the registrar type.
- `ServiceError::Duplicate` now carries a
  `DuplicateServiceRegistration` payload with the Rust type and both owners.

## Earlier upgrade notes: `0.1.1` to `0.2.0`

- `apply_standard_middleware` and generated router helpers now return
  `HttpConfigurationError`, not only `InvalidHeaderValue`.
- `HttpRuntimeConfig` adds `header_policy`; use `..Default::default()` or set it
  explicitly.
- Feedback request headers moved from global middleware to the installed
  Feedback `HttpModule`.
- OpenAPI object schemas must be closed, or explicitly declare both an
  `additionalProperties` value policy and
  `x-minco-open-object.rationale`.
- A required `Idempotency-Key` and `x-minco-idempotent: true` must agree in both
  directions for mutating operations. Path-level and locally referenced
  parameters are effective. `x-minco-auth` must agree with effective OpenAPI
  `security`; permission-scoped metadata never replaces use-case authorization.
- Error/default responses must resolve to `application/problem+json`.
- Catalog entries now include `kind` and facade `feature`.
- The published `0.2.0` family contains 24 packages versus the immutable
  14-package `0.1.1` family. `minco-aws-worker` is new and opt-in.

Run contract checks before compiling, then the facade no-default/default/
all-feature matrix and application tests. Promotion still uses the exact built
artifact; it never rebuilds source.

## Evidence-led upgrade workflow

Before changing the pinned Minco version, capture:

```text
cargo minco upgrade report --json
cargo minco contract diff --against <reviewed-revision> --json
```

The upgrade report inventories the consuming application's Rust/CLI versions,
facade features, typed configuration metadata, selected and linked plugins,
and serialized manifest/contract/deployment schema versions. It intentionally
omits configuration defaults, values and secret-reference names. Its
`review_required` assessment means release notes and migration guidance are
still authoritative.

Run the same reports after the dependency and feature edit, then compare the
schema-1 output. Resolve every `breaking` or `uncertain` contract item in the
actual request/response direction. Follow with contract checks, compilation,
the facade feature matrix, application tests and deployment-plan validation.
See [`../reference/compatibility.md`](../reference/compatibility.md) for the
bounded classifications and report limitations.

## Operational boundary

Local compiler, contract, Rustack, package and SAM evidence is not a live
deployment. AWS deployment, SES delivery, CloudFront creation, load/soak,
product data migration, rollback rehearsal and physical operational proof
remain separate approvals and evidence.

The dated
[`CGSP and GarmentIQ validation`](two-application-validation-2026-08-03.md)
shows how those evidence states are kept separate for real applications. It
also records why native product quality is not automatically Minco adoption.
