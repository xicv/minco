---
id: M14-T38
title: Add explicit API Gateway HTTP traffic policy
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
  - docs/how-to/api-traffic-policy.md
  - tasks/M14/M14-T38-api-gateway-traffic-policy.md
checks:
  - cargo test -p minco-plan --locked
  - cargo clippy -p minco-plan --all-targets --all-features --locked -- -D warnings
  - cargo fmt --check -- crates/minco-plan/src/lib.rs crates/minco-plan/src/traffic.rs
---

# M14-T38 - Add explicit API Gateway HTTP traffic policy

## Goal

Add a thin, opt-in traffic-protection policy for Minco's existing API Gateway
HTTP API ingress. The policy must use AWS-native stage and route throttling so
excess traffic can be rejected before Lambda invocation without Redis, a
counter database, an always-on worker, provisioned capacity, or request-path
Rust middleware.

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

The implementation takes the useful Laravel idea of named, reviewable traffic
limits but deliberately does not copy Laravel's cache-backed request middleware.
Minco's default production path should spend no Lambda/database work on traffic
that the managed ingress can reject first.

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
- the implementation adds no AWS resource, schedule, fixed compute, runtime
  dependency, Redis/cache requirement, or application request-path allocation;
- documentation states that API Gateway throttling is best-effort, can still
  incur API Gateway request charges, remains subject to account-level AWS
  throttling limits, and is not a hard spend cap or per-user authorization
  control; and
- focused unit tests cover default/route rendering, provider field bounds, and
  fail-closed validation.

## Non-goals

- per-user, per-IP, per-tenant, or per-credential distributed rate limiting;
- API keys, usage plans, billing quotas, WAF rules, or bot management;
- automatic traffic limits inferred from Lambda concurrency or database size;
- pretending provider-generated `429` bodies use Minco Problem Details;
- introducing a breaking field into the frozen public `DeploymentPlan` schema;
- enabling detailed API Gateway metrics by default; or
- running, dispatching, editing, or adding GitHub Actions.

## Compatibility decision

`DeploymentPlan` is a post-1.0 public serialized contract. Adding a new required
field to it or `DeploymentConfig` would break downstream Rust struct literals
and serialized consumers. This task therefore adds an explicit sidecar
`HttpTrafficPolicy` and traffic-aware SAM rendering entry points while leaving
all existing rendering APIs unchanged. A future Plan schema revision can absorb
this policy only behind its own compatibility task.

## Evidence

Research used the current AWS API Gateway HTTP API throttling documentation, AWS
SAM `AWS::Serverless::HttpApi` reference, and API Gateway v2/CloudFormation stage
route-setting schemas, including default/per-route settings and provider numeric
types. Open-source SAM examples confirm the same declarative route-settings
shape. The deep review additionally checked the exact Minco SAM renderer markers,
`HttpMethod` ownership/method rendering, canonical OpenAPI operation-ID
uniqueness, placeholder/fake-code patterns, and the actual M14 task dependency.
No AWS account operation, provider mutation, deployment, release, registry
action, or GitHub workflow is authorized by this task.

The execution environment used to prepare this draft does not contain `cargo`,
`rustc`, `rustfmt`, `clippy-driver`, `sam`, or `gh`, and direct GitHub network
access from the shell is unavailable. The GitHub connector is therefore used for
source mutation and PR creation. Compilation, rustfmt and Clippy must remain
`NOT RUN` until the exact branch is qualified in a Rust-capable local
environment; they must not be represented as passes.
