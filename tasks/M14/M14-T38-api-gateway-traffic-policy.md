---
id: M14-T38
title: Add AWS-native HTTP traffic and compression controls
milestone: M14
status: active
priority: high
area: plan/aws/http/cost
depends_on: [M14-T37]
operations: []
owned_paths:
  - crates/minco-plan/src/lib.rs
  - crates/minco-plan/src/traffic.rs
  - crates/minco-plan/README.md
  - crates/minco-http/src/lib.rs
  - crates/minco-http/src/middleware.rs
  - crates/minco-http/README.md
  - extensions/minco-aws-lambda/src/lib.rs
  - docs/how-to/api-traffic-policy.md
  - docs/how-to/http-compression.md
  - tasks/M14/M14-T38-api-gateway-traffic-policy.md
checks:
  - cargo test -p minco-plan -p minco-http -p minco-aws-lambda --locked
  - cargo clippy -p minco-plan -p minco-http -p minco-aws-lambda --all-targets --all-features --locked -- -D warnings
  - cargo fmt --check -- crates/minco-plan/src/lib.rs crates/minco-plan/src/traffic.rs crates/minco-http/src/lib.rs crates/minco-http/src/middleware.rs extensions/minco-aws-lambda/src/lib.rs
  - scripts/docs/check-snippets.sh
  - scripts/docs/check-links.sh
---

# M14-T38 - Add AWS-native HTTP traffic and compression controls

## Goal

Add thin, explicit traffic-efficiency controls around Minco's existing API
Gateway HTTP API ingress. Coarse overload protection must use AWS-native stage
and route throttling before Lambda invocation. Normal dynamic response
compression must reuse the existing Axum/Tower runtime rather than introducing
Nginx, CloudFront, Redis, a counter database, an always-on worker, provisioned
capacity, or another fixed-cost control plane.

## Research boundary

Current AWS API Gateway HTTP APIs use token-bucket throttling and support both
stage defaults and individual route overrides. AWS documents these settings as
best-effort targets rather than hard ceilings. AWS SAM exposes the same
`DefaultRouteSettings` and `RouteSettings` properties on
`AWS::Serverless::HttpApi`, while `AWS::ApiGatewayV2::Stage` exposes equivalent
settings for Minco's candidate stage. The provider represents the burst field as
an int32 and the steady-state rate as a double, so the public Rust budget mirrors
that provider boundary instead of accepting a wider integer and failing later in
CloudFormation.

Nginx's gzip module compresses responses, supports a minimum response size, and
defaults to compression level 1. Minco already enabled Tower HTTP gzip in its
standard runtime, but Tower's generic predicate admitted known bodies from 32
bytes and the repository did not prove the Lambda binary transport boundary.
The hardened policy uses fastest-level gzip only for eligible known-size bodies
of at least 1 KiB, preserves content negotiation and content-type exclusions,
and provides a typed per-response opt-out for BREACH-sensitive representations.
The official Rust `lambda_http` adapter treats a response carrying
`Content-Encoding` as binary and maps it through API Gateway v2's Lambda proxy
response path.

Minco's static-site topology already enables CloudFront automatic Brotli and
gzip plus HTTP/2 and HTTP/3. AWS documents provider-side
`minimumCompressionSize` for API Gateway REST APIs; the HTTP API/SAM resource
Minco deploys exposes no equivalent compression property. Adding CloudFront in
front of every dynamic API solely for compression would add topology, request
cost, forwarding and cache-policy risk, so it remains an explicit future
profile rather than a hidden default.

The implementation takes the useful Laravel idea of named, reviewable traffic
limits but deliberately does not copy Laravel's cache-backed request middleware.
Minco's default production path should spend no Lambda/database work on traffic
that the managed ingress can reject first, while spending bounded Lambda CPU
only where dynamic response compression has a credible byte benefit.

## Acceptance

- a typed serializable `HttpTrafficPolicy` supports one optional default budget
  plus deterministic operation-ID overrides;
- every budget rejects non-finite/non-positive request rates and non-positive
  burst values, and the burst type cannot exceed API Gateway's int32 field;
- operation overrides fail closed when the operation ID is absent from the
  reviewed `DeploymentPlan` routes;
- duplicate method/path route keys fail closed instead of silently overwriting
  one SAM route setting;
- traffic-policy rendering is supported only for API Gateway HTTP API ingress;
- the same effective policy is rendered into the `$default` and `candidate`
  stages so hosted verification does not exercise a less-protected topology;
- tests parse the rendered YAML and verify the settings under both actual stage
  resources rather than treating matching output strings alone as proof;
- the existing `render_sam*` functions remain unthrottled when no traffic policy
  is selected;
- `HttpRuntimeConfig::default()` retains negotiated response compression without
  changing the public configuration shape;
- dynamic compression offers fastest-level gzip only when the client advertises
  it and the known response size is at least 1 KiB;
- Tower HTTP's existing exclusions for gRPC, images and Server-Sent Events and
  its no-recompression behavior remain in force;
- `DisableResponseCompression` lets an application opt one response out without
  disabling compression for unrelated responses;
- focused tests prove gzip framing, `Content-Encoding`,
  `Vary: Accept-Encoding`, the minimum-size boundary, global and per-response
  opt-outs, and Lambda's binary-body conversion;
- CloudFront static-site Brotli/gzip remains unchanged and no dynamic CloudFront
  distribution is introduced;
- the implementation adds no AWS resource, schedule, fixed compute, runtime
  service, Redis/cache requirement, or application request counter;
- documentation states that API Gateway throttling is best-effort, can still
  incur API Gateway request charges, remains subject to account-level AWS
  throttling limits, and is not a hard spend cap or per-user authorization
  control; and
- documentation states the BREACH boundary and explains why request-body
  decompression and dynamic Brotli/zstd are not implicit defaults.

## Non-goals

- per-user, per-IP, per-tenant, or per-credential distributed rate limiting;
- API keys, usage plans, billing quotas, WAF rules, or bot management;
- automatic traffic limits inferred from Lambda concurrency or database size;
- pretending provider-generated `429` bodies use Minco Problem Details;
- introducing a breaking field into the frozen public `DeploymentPlan` schema;
- enabling detailed API Gateway metrics by default;
- adding Nginx, an always-on reverse proxy, or CloudFront in front of every API;
- generic request-body decompression without decompressed-size and compression-
  bomb controls;
- enabling dynamic Brotli, deflate, or zstd without measured ARM64 Lambda
  artifact, CPU, latency and transfer evidence;
- replacing Minco's direct object-transfer path with compressed Lambda uploads;
- running, dispatching, editing, or adding GitHub Actions; or
- making unrelated formatting changes.

## Compatibility decision

`DeploymentPlan` is a post-1.0 public serialized contract. Adding a new required
field to it or `DeploymentConfig` would break downstream Rust struct literals
and serialized consumers. This task therefore adds an explicit sidecar
`HttpTrafficPolicy` and traffic-aware SAM rendering entry points while leaving
all existing rendering APIs unchanged. A future Plan schema revision can absorb
this policy only behind its own compatibility task.

`HttpRuntimeConfig` is also a public Rust struct. The compression hardening keeps
its existing `compression: bool` field and changes only the implementation
behind the enabled state. The threshold constant and response opt-out marker are
additive APIs. Applications that require a different codec or predicate can
turn off the standard layer and compose an explicit Tower layer without a
serialized migration.

## Evidence

Research used the current AWS API Gateway HTTP API throttling documentation, AWS
SAM `AWS::Serverless::HttpApi` reference, API Gateway v2/CloudFormation stage
route-setting schemas, API Gateway REST API compression documentation, CloudFront
compressed-file behavior, Nginx gzip documentation, HTTP content-coding
semantics, Tower HTTP 0.7 source, and the official Rust Lambda HTTP 1.3 response
adapter. The review checked the exact Minco SAM renderer, static CloudFront
settings, Lambda adapter, Tower feature selection, canonical operation-ID
uniqueness, placeholder/fake-code patterns, and the actual M14 task dependency.
No AWS account operation, provider mutation, deployment, release, registry
action, or GitHub workflow is authorized by this task.

The source environment used to prepare this draft does not contain `cargo`,
`rustc`, `rustfmt`, `clippy-driver`, or `sam`, and direct shell access to GitHub
is unavailable. Source changes are therefore made through the GitHub connector.
Compilation, tests, rustfmt, Clippy, documentation checks and SAM validation must
remain `NOT RUN` until the exact branch is qualified in a Rust-capable local
environment; they must not be represented as passes.
