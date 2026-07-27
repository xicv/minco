# Minco capability audit against GarmentIQ and CGSP

Status: Minco 0.3.0 cross-project adoption review, updated after the first real
CGSP contract, composition, HTTP-parity, and local Feedback slices.

This audit distinguishes four levels:

- **core** — framework composition, contracts, diagnostics, lifecycle, and Plan
  IR;
- **official plugin contract** — provider-neutral application-facing API plus a
  deterministic memory/reference implementation;
- **official provider adapter/runtime** — concrete SQLx, AWS, Lambda HTTP, or
  Lambda worker implementation shipped by Minco;
- **application policy** — product-specific behavior that must stay outside the
  framework.

## Verdict

**Ready with explicit gaps.**

Minco 0.3.0 has a strong provider-neutral core and a credible official plugin
and adapter set for the reusable platform capabilities exercised by GarmentIQ
and CGSP. It is now being consumed by CGSP for contract validation, an
unselected composition shell, HTTP parity evidence, and a feature-flagged local
Feedback vertical slice. Those integrations preserve CGSP business code,
PostgreSQL/RLS authority, frontend, worker, Pulumi deployment, and release
controls.

The framework should not absorb routing, inventory, garment, fulfilment,
finance, reporting, customer ownership, role policy, or rollback admission
rules that belong to a product. The remaining framework gaps are bounded:

1. Minco 0.3.0 activates both SQLx database backends for a Feedback
   PostgreSQL-only consumer because the workspace dependency owns both backend
   features. M6-T08 isolates backend features; its source correction remains
   unverified until the exact Rust quality and source-manifest gates run.
2. The deployment Plan IR and SAM renderer still model the initial single HTTP
   API function. M6-T09 owns a versioned, trigger-aware design for API + SQS
   worker + event-source mapping + explicit recovery schedule. Existing product
   IaC remains authoritative meanwhile.
3. GarmentIQ's rollback runtime-compatibility, preservation admission, and
   rehearsal evidence remain application release policy. `minco-release`
   supplements but does not replace those controls.
4. Live SES delivery and a complete CloudFront distribution/invalidation
   rehearsal remain cost-bearing provider evidence, not missing
   provider-neutral APIs.

## Core and plugin-system review

The core now provides:

- static, explicit, semver-addressable plugin crates;
- strict lower-kebab IDs and stable configuration namespaces;
- core-version compatibility and versioned required/provided capabilities;
- dependency auto-enablement with fail-closed explicit disabling;
- graph validation before service construction;
- typed singleton bindings and deterministic ordered multi-bindings;
- non-spoofable application/plugin ownership provenance;
- duplicate-service diagnostics naming Rust type, first owner, and attempted
  owner;
- bounded metadata-only registration inspection without values or secrets;
- deterministic install and finalize phases;
- machine-readable operations, migrations, resources, health, wake sources,
  cost, stability, data classes, documentation, and configuration fields;
- exact HTTP operation ownership and plugin-contributed request-header policy;
- compile-time Cargo features plus explicit runtime selection;
- the same public APIs for official and third-party plugins.

Deliberate boundaries remain:

- no runtime dynamic-library ABI;
- no string-key service locator or global facade;
- no automatic filesystem or package scanning;
- no remote calls, migrations, or hidden background work during composition;
- no generic ORM repository;
- no provider-specific dependency in `minco-core`;
- no claim that independently configured stores share one transaction.

## Coverage matrix

| Prior capability | Minco coverage | Level | Notes |
|---|---|---|---|
| OpenAPI-first HTTP APIs | `minco-contract`, `minco-http` | core | Canonical OpenAPI inventory, strict idempotency/auth/Problem Details policy, deterministic bindings, and exact HTTP ownership. |
| Axum/Tower middleware | `minco-http` | core | Exact origins and headers, request IDs, timeouts, limits, redaction, compression, principal mapping, and RFC 9457 responses. |
| Modular monolith/use-case ports | workspace conventions and architecture checks | core/application policy | Business rules remain ordinary Rust behind application-owned ports. |
| PostgreSQL | `minco-sqlx-postgres` | provider adapter | Bounded pools, explicit migration history, sessions, idempotency, audit, and transaction-integrated outbox. |
| SQLite | `minco-sqlx-sqlite` | provider adapter | Persistent local/single-process profile with explicit concurrency and durability limits. |
| Database deployment choices | `minco-plan` | core | Neon, self-hosted PostgreSQL, RDS, Aurora, DynamoDB, and SQLite cost profiles. DynamoDB is not presented as a relational drop-in. |
| Idempotent commands | `minco-plugin-idempotency`, SQLx adapters | plugin + adapters | Fingerprints, leases, replay/conflict semantics, persistent PostgreSQL/SQLite implementations. |
| Liveness/readiness | `minco-plugin-health` | plugin | Other plugins contribute bounded async checks. |
| Structured logging | `minco-plugin-observability` | plugin | CloudWatch-compatible JSON through `tracing`. |
| Browser/API identity | `minco-plugin-identity`, `minco-aws-lambda` | plugin + runtime | Verified API Gateway claims map to a generic principal; product authorization and RLS remain authoritative. |
| Server sessions and CSRF | `minco-plugin-sessions`, SQLx adapters | plugin + adapters | Opaque hashed tokens, expiry, revocation, HMAC CSRF, and persistent stores. |
| Object storage | `minco-plugin-object-storage`, `minco-aws-adapters` | plugin + adapter | S3 storage, signed GET, size-enforced signed POST, exact-prefix IAM, Rustack and bounded real-S3 evidence. |
| Events/outbox/SQS | `minco-plugin-events`, `minco-sqlx-postgres`, `minco-aws-adapters` | plugin + adapters | Caller-transaction enqueue, leased claims, request/operator dispatch, and SQS publication; no hidden poller. |
| SQS Lambda invocation | `minco-aws-worker` | runtime | Partial-batch responses, bounded payload/concurrency, deterministic failures, FIFO fail-forward, no queue or schedule creation. |
| Notifications | `minco-plugin-notifications`, `minco-aws-adapters` | plugin + adapters | SES and DNS-pinned signed webhooks plus provider-neutral in-app/developer channels. |
| Durable business audit | `minco-plugin-audit`, SQLx adapters | plugin + adapters | Append-only memory/PostgreSQL/SQLite sinks separate from operational logs. |
| Client Feedback | `minco-plugin-feedback` | stable official vertical slice | Widget, screenshots/files, optional voice/transcription, threads, workflow states, PostgreSQL/SQLite/memory/custom stores, protected developer APIs, audit/events/notifications, and AI context. |
| Native HTTP Lambda | `minco-aws-lambda` | runtime | ARM64 Lambda HTTP/API Gateway integration and SSM SecureString loading. |
| Static web delivery | `minco-plugin-static-site`, `minco-aws-adapters` | plugin + adapter | Safe traversal, private S3/CloudFront OAC, custom-domain inputs, invalidation, exact IAM. |
| Cognito administration | `minco-plugin-identity`, `minco-aws-adapters` | plugin + adapter/application policy | Generic create/get/disable/delete; invitation ownership, resend, groups, and roles remain product policy. |
| Build-once promotion | `minco-release` | core | Contract, migrations, source, plan, template, lockfile, and artifact digests. |
| Rustack-shaped local AWS | graph-derived local topology and endpoint overrides | extension policy | S3/SQS/SSM/STS conformance; real AWS remains authoritative. |
| Cost/performance awareness | `minco-plan`, `cargo minco cost`, `cargo minco perf` | core | Idle-cost classes, schedules, NAT/provisioned concurrency, connection budgets, body/timeout/memory/artifact limits. |
| Mapbox, ERP, courier, payments, reports | application adapters | application policy | External integrations and product reporting are not framework plugins. |

## Evidence from real adoption

CGSP has already proved that Minco 0.3.0 can be introduced incrementally without
a rewrite:

- the existing 31-operation OpenAPI contract is validated and reconciled while
  the legacy router remains selected;
- an unselected `cgsp-platform` shell composes Health and Observability and
  exposes registration provenance without network/database side effects;
- an HTTP parity harness verifies status, Problem Details, request IDs,
  authentication, and all six idempotent command boundaries;
- a default-off, loopback-only local Feedback pilot uses PostgreSQL persistence
  while CGSP identity, authorization, migrations, deployment, and business
  logic remain authoritative.

This is meaningful framework evidence, but it is not yet the complete M7
stabilization criterion: GarmentIQ still needs one bounded Minco slice, and
CGSP's deployment Plan parity is intentionally incomplete.

## Provider and operational boundaries

- Applications must select exact providers, retention/privacy policy, domains,
  certificates, retry/DLQ policy, and business invitation/role workflows.
- Memory implementations are reference/test adapters, not production
  durability.
- SES live-send evidence requires an approved verified identity.
- CloudFront creation/invalidation is a separately approved cost-bearing
  rehearsal.
- Local compiler, Rustack, package, and SAM evidence is not live production
  evidence.
- A release-scoped security-scan waiver is not reusable; future releases must
  obtain fresh scanner evidence or record a new explicit risk decision.

## Conclusion

Minco's architectural center is strong enough for continued CGSP adoption and a
future GarmentIQ pilot. The immediate correctness issue is SQLx backend feature
isolation; the larger deployment-topology limitation belongs in a separately
versioned Plan IR task. Neither finding justifies weakening the core boundaries
or moving product business policy into official plugins.
