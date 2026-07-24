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

The official plugin surface now covers the reusable application capabilities
used by both products. Provider-specific production adapters are intentionally
tracked separately and must not be described as implemented when only a port or
memory adapter exists.

## Coverage matrix

| Prior capability | Minco coverage | Level | Notes |
|---|---|---|---|
| OpenAPI-first HTTP APIs | `minco-contract`, `minco-http` | core | One operation inventory feeds routing and deployment metadata. |
| Axum/Tower middleware | `minco-http` | core | Exact CORS, request IDs, timeouts, limits, sensitive headers, Problem Details. |
| Modular monolith and use-case ports | workspace layout and dependency checks | core/application policy | Business rules remain ordinary Rust. |
| PostgreSQL | `minco-sqlx-postgres` | official provider adapter | Bounded pools and explicit migrations; domain ports stay application-owned. |
| SQLite | `minco-sqlx-sqlite` | official provider adapter | Local/single-process profile with explicit durability constraints. |
| Database deployment choices | `minco-plan` | core | Neon, self-hosted PostgreSQL, RDS, Aurora, DynamoDB, and SQLite cost profiles. |
| Idempotent commands | `minco-plugin-idempotency` | official plugin contract | Memory reference store; products may keep transaction-integrated command stores. |
| Liveness/readiness | `minco-plugin-health` | official plugin contract | Other plugins contribute async checks during deterministic finalization. |
| Structured logging | `minco-plugin-observability` | official plugin contract | CloudWatch-compatible JSON through `tracing`. |
| Browser/API identity | `minco-plugin-identity`, `minco-aws-lambda` | plugin contract + provider adapter | Verified claims map to permissions; API Gateway JWT context is supported. |
| Server-side sessions and CSRF | `minco-plugin-sessions` | official plugin contract | Opaque hashed tokens, expiry, revocation, and HMAC CSRF primitives. |
| S3-style object storage | `minco-plugin-object-storage` | official plugin contract | Server-side object and direct-access signing ports; S3 implementation remains an AWS-adapter task. |
| SQS/domain events | `minco-plugin-events` | official plugin contract | Explicit publisher and leased outbox ports; no hidden one-minute poller. |
| Email/webhook/in-app notification | `minco-plugin-notifications` | official plugin contract | SES/webhook implementations remain provider-adapter tasks. |
| Durable business audit | `minco-plugin-audit` | official plugin contract | Append-only contract, separate from operational logs. |
| Feedback and client review | `minco-plugin-feedback` | official vertical slice, compiler verification pending | Widget, screenshots, voice, threads, PostgreSQL/SQLite/memory stores, developer API/CLI, and deterministic AI context. |
| Native Lambda/API Gateway deployment | `minco-aws-lambda`, `minco-plan` | official provider adapter/core | One ARM64 ZIP and HTTP API default. |
| Build-once release promotion | `minco-release` | core | Contract, migration, plan, source, and artifact hashes. |
| Rustack-shaped local AWS | endpoint override/local scripts | extension policy | Emulator is replaceable; real AWS remains authoritative. |
| Static web delivery | `minco-plugin-static-site` | official plugin contract | Build-directory, SPA fallback, cache, custom-domain, and deployment intents; S3/CloudFront rendering remains an AWS-adapter task. |
| Cognito user administration/invitations | identity/application ports | planned provider adapter/application policy | Authentication is reusable; invitation workflow and role policy remain product-aware. |
| Mapbox, ERP, courier, reports | application adapters | application policy | External integrations and business reports are not framework concepts. |

## Gaps that remain explicit

The following are not blockers in the core/plugin design, but concrete official
provider adapters are still required before Minco can claim turnkey production
coverage for every AWS deployment used by the two applications:

1. S3 `ObjectStore` and `ObjectAccessSigner` adapters.
2. SQS `EventPublisher` plus a transaction-integrated PostgreSQL outbox adapter.
3. SES notification delivery and a signed webhook notification adapter.
4. Persistent PostgreSQL/SQLite session, idempotency, and audit adapters where a
   product does not already own the transaction boundary.
5. Cognito administrative user/invitation adapter.
6. S3/CloudFront renderer for the static-site plugin, including OAC, custom-domain, and invalidation policy.
7. Real-AWS conformance tests and IAM derivation for each selected adapter.

These are tracked work, not silently filled by memory implementations. The public
ports make them independently addable without changing Minco core or Feedback.

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

Minco now has a stable architectural center for the previous applications and a
credible ecosystem boundary. The remaining risk is implementation maturity, not
missing core extensibility: compiler, database, browser, Lambda, SAM, and real-AWS
conformance gates must still run before the new beta plugins are released.
