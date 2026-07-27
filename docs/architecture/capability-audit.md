# Minco capability audit against GarmentIQ and CGSP

Status: current architecture review for the Feedback plugin change.

This audit tests Minco against capabilities that were implemented in GarmentIQ
and CGSP. It distinguishes four levels:

- **core** — framework composition, contracts, diagnostics, lifecycle, and plan model;
- **official plugin contract** — provider-neutral application-facing API plus a
  deterministic memory/reference implementation;
- **official provider adapter** — concrete SQLx or AWS implementation shipped by Minco;
- **application policy** — business-specific behavior that must stay outside the framework.

## Result

The Minco core is strong enough for both applications after this change. The
plugin kernel now supports typed single-provider services, ordered multi-provider
contributions, strict configuration schemas with defaults, dependency resolution,
core-version checks, graph validation before construction, and a deterministic
second finalization pass for aggregate registries. It remains statically linked
and contains no runtime shared-library loading or global service locator.

The official plugin surface covers the reusable application capabilities used
by both products. SQLx and AWS production adapters are separate crates and
remain explicit composition-root choices; memory adapters are still references,
not production durability.

## Coverage matrix

| Prior capability | Minco coverage | Level | Notes |
|---|---|---|---|
| OpenAPI-first HTTP APIs | `minco-contract`, `minco-http` | core | One operation inventory feeds routing and deployment metadata. |
| Axum/Tower middleware | `minco-http` | core | Exact CORS, request IDs, timeouts, limits, sensitive headers, Problem Details. |
| Modular monolith and use-case ports | workspace layout and dependency checks | core/application policy | Business rules remain ordinary Rust. |
| PostgreSQL | `minco-sqlx-postgres` | official provider adapter | Bounded pools and explicit migrations; domain ports stay application-owned. |
| SQLite | `minco-sqlx-sqlite` | official provider adapter | Local/single-process profile with explicit durability constraints. |
| Database deployment choices | `minco-plan` | core | Neon, self-hosted PostgreSQL, RDS, Aurora, DynamoDB, and SQLite cost profiles. |
| Idempotent commands | `minco-plugin-idempotency`, SQLx adapters | official plugin contract + provider adapters | Memory plus persistent PostgreSQL and SQLite lease/replay stores with concurrent-owner tests. |
| Liveness/readiness | `minco-plugin-health` | official plugin contract | Other plugins contribute async checks during deterministic finalization. |
| Structured logging | `minco-plugin-observability` | official plugin contract | CloudWatch-compatible JSON through `tracing`. |
| Browser/API identity | `minco-plugin-identity`, `minco-aws-lambda` | plugin contract + provider adapter | Verified claims map to permissions; API Gateway JWT context is supported. |
| Server-side sessions and CSRF | `minco-plugin-sessions`, SQLx adapters | official plugin contract + provider adapters | Opaque hashed tokens, expiry, revocation, HMAC CSRF, and persistent PostgreSQL/SQLite stores. |
| S3-style object storage | `minco-plugin-object-storage`, `minco-aws-adapters` | official plugin contract + provider adapter | Server-side storage, signed POST size enforcement, signed GET, exact-prefix IAM, Rustack proof, and bounded real-S3 proof. |
| SQS/domain events | `minco-plugin-events`, `minco-aws-adapters`, `minco-sqlx-postgres` | official plugin contract + provider adapters | SQS publication and transaction-integrated leased PostgreSQL outbox; no hidden poller. |
| Email/webhook/in-app notification | `minco-plugin-notifications`, `minco-aws-adapters` | official plugin contract + provider adapters | SES and DNS-pinned signed-webhook delivery exist; SES live send is externally blocked by the absence of a verified account identity. |
| Durable business audit | `minco-plugin-audit`, SQLx adapters | official plugin contract + provider adapters | Append-only memory, PostgreSQL, and SQLite sinks, separate from operational logs. |
| Feedback and client review | `minco-plugin-feedback` | official vertical slice (stable) | Compiler, HTTP, PostgreSQL, SQLite, memory, CLI, Chromium, and Firefox gates pass. Project isolation, authenticated transcription, public error redaction, HTTPS developer transport, and backend-specific SQLx feature graphs are regression-tested. |
| Native Lambda/API Gateway deployment | `minco-aws-lambda`, `minco-plan` | official provider adapter/core | One ARM64 ZIP and HTTP API default. |
| Build-once release promotion | `minco-release` | core | Contract, migration, plan, source, and artifact hashes. |
| Rustack-shaped local AWS | endpoint override/local scripts | extension policy | Pinned local S3/SQS/SSM/STS and compiled adapter proof pass; real AWS remains authoritative. |
| Static web delivery | `minco-plugin-static-site`, `minco-aws-adapters` | official plugin contract + provider adapter | Safe build traversal, S3 publication, private CloudFront OAC rendering, custom-domain inputs, invalidation, and exact-prefix IAM. |
| Cognito user administration/invitations | `minco-plugin-identity`, `minco-aws-adapters` | official plugin contract + provider adapter/application policy | Bounded real-AWS create/get/disable/delete passes; invitation message and role policy remain product-aware. |
| Mapbox, ERP, courier, reports | application adapters | application policy | External integrations and business reports are not framework concepts. |

## Gaps that remain explicit

The remaining boundaries are operational or adoption gates, not missing adapter
implementations:

1. SES delivery has compile/unit coverage but no live send because the approved
   account currently has zero verified email and domain identities. Minco does
   not create an unverifiable identity or weaken SES policy to manufacture a
   pass.
2. CloudFormation validates the private S3/CloudFront OAC template, and S3
   publication passes in real AWS. Creating a live distribution and exercising
   invalidation remains a separately approved, slower/cost-bearing release
   rehearsal.
3. Applications must still select exact providers, supply retention/privacy
   policy, configure verified domains and certificates, and own any business
   invitation or role workflow.
4. The external repository-wide Deep Security Scan repeatedly failed to produce
   canonical artifacts for the authorized defensive review. M6-T05 records a
   one-release owner waiver, manual validation and remediation of the partial
   Feedback candidates, and independent exact-head controls. This is not a
   successful scan or a reusable exception; a later release must retry an
   available scanner or make a new explicit risk decision.

## Plugin-system review

### Strengths

- static, explicit, semver-addressable plugin crates;
- strict lower-kebab IDs and one stable configuration namespace per plugin;
- required/provided capability versions;
- plugin dependency auto-enablement with explicit-disable failure;
- graph validation before any service construction;
- typed single bindings and ordered typed multi-bindings;
- separate install/finalize phases without remote side effects;
- machine-readable operations, migrations, resources, health, cost, data classes,
  stability, documentation, and configuration fields;
- compile-time Cargo features plus runtime selection;
- third-party plugins use the same APIs as official plugins.

### Boundaries retained deliberately

- no runtime dynamic-library ABI;
- no string-key service locator;
- no automatic filesystem/package scanning;
- no remote calls, migrations, or background tasks during plugin composition;
- no generic ORM repository;
- no provider-specific dependencies in `minco-core`;
- no claim that independent data stores share a transaction.

## Conclusion

Minco now has a stable architectural center and concrete SQLx/AWS adapter
boundary. PostgreSQL-only, SQLite-only, memory, and no-default dependency graphs
are checked explicitly while deliberate all-feature builds retain both SQLx
backends. Feedback's compiler, database, browser, security-regression, and
exact-head local gates pass; Lambda/SAM, exact-resource IAM, Rustack, bounded
real AWS, and verified cleanup also pass within the recorded scope. Feedback is
stable under the release-scoped M6-T05 risk decision. The unrun SES delivery and
live CloudFront distribution remain explicit deployment rehearsals rather than
silent passes or blockers for the provider-neutral Feedback contract.
