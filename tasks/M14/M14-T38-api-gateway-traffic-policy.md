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
  - rustfmt --check --edition 2024 crates/minco-plan/src/lib.rs crates/minco-plan/src/traffic.rs crates/minco-http/src/lib.rs crates/minco-http/src/middleware.rs extensions/minco-aws-lambda/src/lib.rs
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

### Local qualification (2026-08-19)

The branch was qualified locally in a dedicated JJ workspace created from the
exact remote PR head `c8e81111baa087852ae06aba8c82cf1ef2759204`
(`agent/api-gateway-traffic-policy@origin`), base `origin/main`
`7e0ee6a863f479f41003613fd29ac17bcb712b32`, on macOS 26.5.2 (build 25F84,
arm64, Darwin 25.5.0) with rustc 1.97.1 (8bab26f4f), cargo 1.97.1 (c980f4866),
clippy 0.1.97, rustfmt 1.9.0-stable, and SAM CLI 1.164.0, all selected by
`rust-toolchain.toml`. Results against the exact final local tree:

- `cargo check -p minco-plan -p minco-http -p minco-aws-lambda --all-targets --all-features --locked` — PASS
- `cargo test -p minco-plan -p minco-http -p minco-aws-lambda --locked` — PASS (138 tests passed, 0 failed, 1 pre-existing ignored)
- `cargo test -p minco-plan -p minco-http -p minco-aws-lambda --all-targets --all-features --locked` — PASS
- `cargo clippy -p minco-plan -p minco-http -p minco-aws-lambda --all-targets --all-features --locked -- -D warnings` — PASS
- `rustfmt --check --edition 2024 crates/minco-plan/src/lib.rs crates/minco-plan/src/traffic.rs crates/minco-http/src/lib.rs crates/minco-http/src/middleware.rs extensions/minco-aws-lambda/src/lib.rs` — PASS
- `cargo doc -p minco-plan -p minco-http -p minco-aws-lambda --all-features --no-deps --locked` — PASS
- `scripts/docs/check-snippets.sh` — PASS (350 fenced blocks)
- `scripts/docs/check-links.sh` — PASS (2022 internal, 14 external, 485 canonical pages)
- `scripts/docs/build.sh` — PASS (locked npm install, audit, and VitePress build)
- `cargo semver-checks -p minco-plan -p minco-http -p minco-aws-lambda --baseline-version 1.8.0` — PASS (223 checks per crate, no semver update required; the change is additive)
- `sam validate --template /tmp/minco-traffic-template.yaml --region ap-southeast-2 --lint` — PASS ("is a valid SAM Template"). The exact traffic-aware template (default budget plus one route override on both `$default` and `candidate` stages) was generated through an uncommitted transient crate example driving the public sidecar renderer; the harness was deleted before committing and no AWS API was called.
- `uv run --locked python scripts/deep_review.py` — run for diagnosis; embedded static validation reported status ok with 0 errors/0 warnings and 0 findings. Its `verification/deep-review.json` rewrite was reverted because that evidence file is owned by active M14-T37.

Dependency and ownership state at qualification time:

- M14-T37 is still `active`, so `cargo run -p cargo-minco -- task ready --json`
  does not list M14-T38; this task correctly remains `active` and cannot be
  finished.
- `roadmap/tasks.mmd` is owned by active M14-T37, so task-graph regeneration is
  blocked by the ownership boundary and was not attempted.

### Full workspace gate (2026-08-19, exact head `83e29e10aaf84cb6cd09b6b62f11f2a4bafe7b46`)

`scripts/quality.sh` was executed end-to-end. The operational-evidence receipt
step stops the script when the committed receipt is stale, so every remaining
step was then run individually in the script's original order. Result: 48
step-level passes, including `cargo fmt --all -- --check`, workspace-wide
`cargo check`, `cargo clippy --workspace --all-targets --all-features
--locked -- -D warnings`, the full workspace test suite, `cargo rustdoc`,
workspace `cargo doc`, documentation snippet/link/browser checks, the
generated-apps check, shell portability, SQLx feature isolation, gitleaks and
npm audit. Six failures remain, and each was reproduced identically on a
clean extraction of `origin/main` `7e0ee6a863f479f41003613fd29ac17bcb712b32`,
so they are pre-existing main-state failures rather than regressions of this
branch:

- `validate_operational_evidence.py --check-output` and
  `source_manifest.py --check`: the committed evidence receipts bind the last
  independently reviewed tree, and main's own HEAD already fails both after
  its CI-runner commits. Regenerating the manifest locally only exposes
  EVIDENCE-PROVIDER-021 and PERF-BASELINE findings that require live
  hosted/provider qualification owned by active M14-T37, so the committed
  receipt snapshots were deliberately left untouched.
- `scripts/test/hosted_ci_policy.py` (1 error) and
  `scripts/test/publish_validation.py` (KeyError `env`): workflow-content test
  breakage introduced by main's recent runner-migration commits; fixing it
  requires editing `.github/workflows` content or cross-task test files and
  is outside this task's scope.
- cargo-minco tests `rust_source_authority_matches_the_generated_manifest`
  and `actual_handover_plan_is_read_only_and_deterministic`: the same
  manifest-bound staleness, failing identically on clean main.
- `cargo deny check advisories` and `cargo audit` (RUSTSEC-2026-0258
  vulnerability, RUSTSEC-2026-0253 allowed warning): driven entirely by
  `Cargo.lock`, which this branch does not change.

This branch therefore introduces no new gate failure relative to main. Merge
into main was authorized by the repository owner on 2026-08-19 after this
review; M14-T38 remains `active` because M14-T37 has not closed, and
`scripts/ci/local-release.sh` remains NOT RUN as a release-level gate outside
this task's boundary.

Branch-head note: JJ's working-copy bookmark tracking moved the remote head
sideways from `83e29e10aaf84cb6cd09b6b62f11f2a4bafe7b46` to
`79c958c2b8decfccb0fc1364ef6e93bcd1f218ba` while restoring evidence-file side
effects; both commits carry an identical tree, no history was lost, and this
evidence commit was then rebased onto `79c958c2` so all further updates are
strict fast-forwards.
