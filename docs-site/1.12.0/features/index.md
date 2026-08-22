---
title: Feature Catalog
description: Browse Minco's shipped web-development capabilities by framework plane.
---

# Feature Catalog

Minco is intentionally narrow: Rust applications deployed with native AWS
building blocks, explicit cost and wake behavior, and no always-on Minco
service. Within that boundary, it covers the essential web-application path
from contract to verified release.

## Contract and HTTP

| Capability | What is standardized | Start here |
|---|---|---|
| OpenAPI 3.1 contracts | operation identity, request/response schemas, security, examples, deterministic bindings | [Resource API](../guides/resource-api) |
| Contract-enforced requests | opt-in direct generated assertions, typed extraction, coarse permission/scope policy, bounded Problems | [Request validation](../guides/contract-request-validation) |
| Resource conventions | data envelopes, bounded cursors, Problem Details, idempotent create, strong ETags, conditional mutation | [Resource reference](../reference/resource-api) |
| Axum and Tower policy | exact CORS/header policy, request IDs, body limits, plugin HTTP contributions | [Framework tour](../getting-started/framework-tour) |
| Contract-aware generation | modules, operations, resources, adapters, tests, workers, migrations, seeders, plugins | [CLI reference](../reference/cli) |

Minco keeps Axum visible. HTTP handlers extract and map, call one application
use case, then map the result; they do not contain SQL or business policy.
Generated request policy is a coarse delivery gate; application use cases still
own tenancy, resource ownership, stored-state authorization and invariants.

## Data and Application Lifecycle

| Capability | What is standardized | Start here |
|---|---|---|
| Typed configuration | fixed precedence, environment classes, strict schema, opaque secret references, redacted provenance | [Configuration](../guides/configuration) |
| SQLx adapters | explicit PostgreSQL and SQLite ports/adapters without an ORM | [Database lifecycle](../guides/database-lifecycle) |
| Migrations | attributable sets, risk metadata, digests, plan/apply/verify receipts | [Migrations and seeders](../guides/database-lifecycle) |
| Seeders | data classification, environment gates, preservation rules, dry run, verification | [Migrations and seeders](../guides/database-lifecycle) |
| Local supervisor | one graph-derived service/lifecycle/process plan with readiness and clean shutdown | [Local development](../guides/local-development) |
| ProjectView | bounded repository-native graph, raw statuses, diagnostics, and six independent evidence lanes | [ProjectView, MCP, and workbench](../guides/project-view) |

Domain crates stay framework-free. Application ports are shaped around use
cases, while PostgreSQL, SQLite, DynamoDB access patterns, AWS clients, and
other infrastructure live in adapters.

## Application Services

Minco ships static, typed plugins for health, observability, idempotency,
identity, sessions, object storage, events, notifications, audit, Feedback,
Waffo hosted payments and static sites, plus subscriber-only realtime
invalidation. The catalog also
includes PostgreSQL/SQLite/DynamoDB adapters and native Lambda HTTP/SQS
runtimes.

- [Identity and sessions](../guides/identity-and-sessions)
- [Files and static sites](../guides/files-and-static-sites)
- [Events, rich mail, and notifications](../guides/events-and-notifications)
- [Realtime invalidation](../guides/realtime)
- [Client feedback loop](../guides/feedback)
- [Ticketing support entry](../guides/ticketing)
- [Waffo hosted payments](../guides/payments-waffo)
- [All 19 built-in components](../plugins/)

Plugins are Cargo dependencies plus explicit typed constructors. Metadata can
describe them before linking, but never enables or executes them.

## Background Work

The `aws-worker` runtime handles SQS partial-batch responses, FIFO fail-forward
behavior, bounded concurrency, and redacted failures. Queues, mappings, retry
policy, dead-letter queues, schedules, and IAM remain explicit Plan IR inputs.

See [Queues and workers](../guides/background-work).

## AWS Deployment and Cost

| Capability | Boundary |
|---|---|
| Plan IR | provider-neutral topology, triggers, IAM, connection pressure, wake sources, and cost classes |
| Native Lambda | ARM64 ZIP runtime behind API Gateway HTTP API; no fixed application compute |
| Guarded deployment | exact account/Region/environment, immutable change-set review, drift and digest checks |
| Exact promotion | build once, verify manifests/digests, promote without rebuilding |
| Static-site delivery | private S3 origin, CloudFront, SPA fallback, optional domain, exact asset receipts |
| DynamoDB | access-pattern-specific conditional writes, bounded indexed queries, exact IAM, and explicit eventual consistency |
| Recovery | compatibility-checked rollback, explicit preview cleanup, optional guarded canary |

Start with [Plan an AWS deployment](../guides/deployment) and
[Zero idle, precisely](../explanation/zero-idle).

## Quality and AI-First Inspection

Tests are nearest-boundary: pure domain tests, fake-port application tests,
real-engine adapter tests, in-process HTTP tests, plugin graph/conformance
tests, Plan/SAM structure tests, and exact-artifact release checks.

`cargo minco inspect --json`, `explain`, task commands, ProjectView, local
read-only MCP, the accessible workbench, deterministic plans, stable
diagnostics, and evidence receipts make the application legible to both people
and coding agents without giving an agent hidden production authority.

See [ProjectView, MCP, and workbench](../guides/project-view) and
[Testing and evidence](../reference/testing).

## Deliberate Non-features

Minco does not provide Active Record, a generic CRUD repository, a global
service locator, runtime plugin scanning, dynamic libraries, implicit
production migrations, hidden schedules, a NAT Gateway in the minimal profile,
or a hosted Minco control plane. These omissions preserve dependency direction,
static composition, ownership, and low idle cost.
