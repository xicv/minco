# Decision Register

This register summarizes the settled framework decisions. Detailed rationale and consequences live under `docs/adrs/`.

| ID | Decision | Status |
|---|---|---|
| [ADR-0001](adrs/0001-openapi-contract.md) | OpenAPI 3.1 is the canonical external HTTP contract. | Accepted |
| [ADR-0002](adrs/0002-modular-monolith.md) | Use a modular monolith and dependency direction `delivery -> application -> domain`. | Accepted |
| [ADR-0003](adrs/0003-axum-tower.md) | Use Axum and Tower directly; Minco adds conventions rather than a replacement HTTP runtime. | Accepted |
| [ADR-0004](adrs/0004-sqlx-no-orm.md) | Use SQLx with explicit PostgreSQL and SQLite adapters; no ORM. | Accepted |
| [ADR-0005](adrs/0005-static-plugins.md) | Compose statically through typed plugin constructors and descriptors; no runtime DI container or dynamic ABI. | Accepted |
| [ADR-0006](adrs/0006-aws-runtime.md) | Default to native ARM64 Lambda ZIP + API Gateway HTTP API. | Accepted |
| [ADR-0007](adrs/0007-plan-ir.md) | Model deployment through provider-neutral Plan IR and structural cost/performance policy. | Accepted |
| [ADR-0008](adrs/0008-rustack-local.md) | Use Rustack through standard AWS endpoint overrides for fast local AWS seams. | Accepted |
| [ADR-0009](adrs/0009-release-manifest.md) | Use immutable build-once release manifests and explicit migrations. | Accepted |
| [ADR-0010](adrs/0010-ai-native.md) | Make AI support depend on transparent structure and JSON introspection. | Accepted |
| [ADR-0011](adrs/0011-jj-first.md) | Use Jujutsu as the default VCS interface with colocated Git for GitHub compatibility. | Accepted |
| [ADR-0012](adrs/0012-database-portfolio.md) | Treat Neon, self-hosted PostgreSQL, RDS, Aurora, DynamoDB and SQLite as explicit correctness/cost profiles. | Accepted |
| [ADR-0013](adrs/0013-quality-and-update.md) | Keep local quality gates authoritative and make updates explicit, reviewable and non-self-replacing. | Superseded by ADR-0038 |
| [ADR-0014](adrs/0014-plugin-lifecycle-and-feedback.md) | Use typed multi-contributions and deterministic plugin finalization; ship Feedback as an explicit AI-ready review loop. | Accepted |
| [ADR-0015](adrs/0015-exact-http-policy-and-explicit-workers.md) | Merge exact application/plugin HTTP policy and keep SQS workers opt-in, bounded and unscheduled. | Accepted |
| [ADR-0016](adrs/0016-explicit-openapi-policy-exceptions.md) | Keep OpenAPI exceptions explicit and validate effective idempotency and security semantics. | Accepted |
| [ADR-0017](adrs/0017-bounded-plugin-registration-provenance.md) | Retain metadata-only application/plugin ownership for typed registrations without exposing values or permitting owner spoofing. | Accepted |
| [ADR-0018](adrs/0018-framework-golden-path.md) | Define Minco by a five-plane contract-to-cloud golden path and measurable 1.0 completion boundary. | Accepted |
| [ADR-0019](adrs/0019-trigger-aware-multi-runtime-plan.md) | Make worker, queue, mapping, schedule, IAM, local-service and cost topology explicit in Plan IR schema 2. | Accepted |
| [ADR-0020](adrs/0020-typed-configuration-graph.md) | Compose strict typed environments and opaque secret references with fixed precedence, redacted provenance and deterministic digests. | Accepted |
| [ADR-0021](adrs/0021-database-migration-lifecycle.md) | Plan, inspect, apply and verify attributable SQLx migration sets through digest-bound commands and durable receipts. | Accepted |
| [ADR-0022](adrs/0022-classified-safe-seeders.md) | Plan and apply classified, preservation-aware seed sets with environment gates, read-only verification and durable receipts. | Accepted |
| [ADR-0023](adrs/0023-graph-driven-development-supervisor.md) | Derive one inspectable local DevPlan and supervise declared services and process groups through `cargo minco dev`. | Accepted |
| [ADR-0024](adrs/0024-guarded-cloudformation-controller.md) | Separate immutable CloudFormation review from exact apply behind current environment, drift, migration and digest approvals. | Accepted |
| [ADR-0025](adrs/0025-zero-provisioned-compute-review-loop.md) | Define zero provisioned application compute, explicit residual cost, and a repository-native Verified Review Loop. | Accepted |
| [ADR-0026](adrs/0026-resource-api-conventions.md) | Standardize opt-in OpenAPI resource shapes, bounded cursors and strong conditional writes without a generic repository or ORM. | Accepted |
| [ADR-0027](adrs/0027-static-plugin-distribution-manifest.md) | Publish a strict archive-visible plugin distribution record while preserving static Cargo composition and runtime descriptors. | Accepted |
| [ADR-0028](adrs/0028-exact-static-site-deployment.md) | Bind static assets to releases and guard private S3, CloudFront, domain publication and hosted byte verification with immutable receipts. | Accepted |
| [ADR-0029](adrs/0029-compatibility-checked-rollback-and-canary.md) | Assess rollback across exact release boundaries and make API canaries opt-in, alarm-guarded, receipt-bound and worker-explicit. | Accepted |
| [ADR-0030](adrs/0030-repository-native-project-view.md) | Project one bounded repository-native read model into local MCP, diagrams, progress views and accessible narration without creating a second state machine. | Accepted |
| [ADR-0031](adrs/0031-subscriber-only-realtime.md) | Use subscriber-only AppSync Events with IAM publication, OIDC subscriptions and HTTP resynchronization for the minimal AWS realtime profile. | Accepted |
| [ADR-0032](adrs/0032-access-pattern-dynamodb.md) | Keep DynamoDB access patterns application-owned and render only explicit table and IAM contracts. | Accepted |
| [ADR-0033](adrs/0033-agent-native-development.md) | Project version-matched portable Agent Skills and bounded CLI context into Codex and Claude without granting implicit mutation authority. | Accepted |
| [ADR-0034](adrs/0034-outbound-mail-delivery.md) | Keep rich outbound mail provider-neutral, ambiguity-safe, privacy-bounded, and direct-SES by default. | Accepted |
| [ADR-0035](adrs/0035-verified-direct-object-uploads.md) | Keep file bytes on direct private object-storage paths and verify issued uploads through typed policy, generated keys, and provider metadata. | Accepted |
| [ADR-0036](adrs/0036-owned-local-service-runtimes.md) | Keep the application native while supervising explicitly owned Docker Compose or Apple Container dependencies through one typed local-service contract. | Accepted |
| [ADR-0037](adrs/0037-release-bound-delivery-evidence.md) | Bind feedback, operational evidence and client handover to exact release/deployment identities behind digest-approved, create-only workflows. | Accepted |
| [ADR-0038](adrs/0038-local-first-actions-boundary.md) | Keep substantive qualification local and reserve GitHub Actions for platform-required compatibility, Pages and crates.io OIDC work. | Accepted |
| [ADR-0039](adrs/0039-waffo-payment-boundary.md) | Keep Waffo checkout and webhook mechanics provider-specific while applications own payment state and live evidence. | Accepted |
| [ADR-0040](adrs/0040-measured-framework-assurance.md) | Bind pinned measured quality and release identity projections to exact source without broadening runtime authority. | Accepted |
| [ADR-0041](adrs/0041-topology-cost-regression-baseline.md) | Guard reviewed golden-topology cost projections without inventing provider prices. | Accepted |
| [ADR-0042](adrs/0042-typed-side-effect-fakes.md) | Keep application-test fakes port-specific, failure-scriptable and privacy-bounded. | Accepted |
| [ADR-0043](adrs/0043-durable-audit-ledger.md) | Record semantic actions through a separate durable audit ledger with provider-specific atomicity and lifecycle policy. | Accepted |
| [ADR-0044](adrs/0044-apple-container-default.md) | Prefer a ready qualified Apple Container for fresh local services while retaining Docker fallback and exact-resource recovery. | Accepted |
| [ADR-0045](adrs/0045-resumable-direct-object-transfers.md) | Keep large HTTP and mobile object transfers direct, resumable, immutable, validation-gated and structurally cost-aware. | Accepted |
| [ADR-0046](adrs/0046-multi-surface-ticketing-entry.md) | Keep Ticketing portal-first and expose one privacy-bounded handoff contract to widgets, extensions, native clients and BFF integrations. | Accepted |
| [ADR-0047](adrs/0047-contract-derived-request-boundary.md) | Generate bounded request validation and coarse authorization from opted-in OpenAPI while preserving application-owned business policy. | Accepted |
| [ADR-0048](adrs/0048-durable-typed-work.md) | Add durable typed work with at-least-once delivery, lease-based execution, explicit schedules and guarded operator recovery. | Accepted |
| [ADR-0049](adrs/0049-integration-ready-ticketing-agent-console.md) | Give Ticketing agents a first-party console seam with compact newest-first summaries, enforced agent capabilities and one atomic management operation, without Jobs or hidden compute. | Accepted |
| [ADR-0050](adrs/0050-requester-safe-public-projections.md) | Give Ticketing requesters a closed public projection (enum authors, public status labels) and own-ticket requester operations with no internal vocabulary or subjects crossing the boundary. | Accepted |
| [ADR-0051](adrs/0051-requester-portal-sessions-and-idempotency.md) | Reuse the sessions and idempotency plugins for durable requester portal sessions, CSRF-bound cookie identity and shared Idempotency-Key replay, all optional. | Accepted |
| [ADR-0052](adrs/0052-append-oriented-ticket-persistence.md) | Make ticket conversation persistence append-oriented: columnar authoritative rows, one-row message appends, independently paginated public messages. | Accepted |
| [ADR-0053](adrs/0053-truthful-bootstrap-and-explicit-store-selection.md) | Remove the ticketing memory Default and make bootstrap capability claims per-feature truthful (portal sessions only when registered, unimplemented captures false). | Accepted |
| [ADR-0054](adrs/0054-optional-ticketing-jobs-bridge.md) | Bridge Ticketing to the released Jobs plugin behind an optional feature with a real notification handler and same-transaction enqueue; no second queue system, no hidden topology. | Accepted |
| [ADR-0055](adrs/0055-verified-inbound-email-command.md) | Process inbound email as a verified durable command: digest-checked raw object, mail-parser MIME, fail-closed classification, ingestion only through the authorized use case. | Accepted |
| [ADR-0056](adrs/0056-activity-intents-as-domain-events.md) | Dispatch transactionally-committed ticketing activity intents as domain events through an explicit bounded pass; at-least-once, never scheduled implicitly. | Accepted |
| [ADR-0057](adrs/0057-ticketing-generated-request-boundary.md) | Ticketing adopts the generated request boundary: contract-derived DTOs with schema bounds at extraction, deterministic plugin-owned sync, authorization stays in the service. | Accepted |
| [ADR-0058](adrs/0058-inbound-thread-routing.md) | Route inbound email to tickets strictly by In-Reply-To/References against ingested external identities and submit the durable ingest job; unresolved threading fails closed. | Accepted |
| [ADR-0059](adrs/0059-inbound-wake-use-case.md) | Wake inbound email through the object-storage port: extract routing facts from authoritative raw MIME and submit through the routing use case; the durable job stays the verification authority. | Accepted |
| [ADR-0060](adrs/0060-s3-wake-event-translation.md) | Translate bounded S3 ObjectCreated records into ticketing inbound wakes inside the SQS worker; eventTime anchors the fingerprint and redelivery stays the retry authority. | Accepted |
| [ADR-0061](adrs/0061-real-s3-wake-envelope.md) | The ticketing wake parses the real S3 Records envelope through the pinned aws_lambda_events types; exactly one aws:s3 ObjectCreated record, urlDecodedKey-aware, everything else fails closed. | Accepted |
| [ADR-0062](adrs/0062-contract-reference-integrity.md) | Contract validation resolves every local `$ref` with JSON Pointer semantics; unresolved targets are errors, and the ticketing contract's dangling agent schemas are restored to match the implementation. | Accepted |
| [ADR-0063](adrs/0063-outbound-delivery-evidence.md) | Outbound ticketing email submits through the observable mail path: acceptance is recorded as append-only evidence (never delivery), retries reconcile against evidence first, and bounce/complaint/delay feedback enters through an authorized use case. | Accepted |
| [ADR-0064](adrs/0064-rustack-inbound-mail-seam.md) | The inbound mail chain is proven live against local Rustack S3/SQS (at-least-once in, dedupe out); S3 reads tolerate foreign-written objects, and SES availability is probed and recorded — never assumed. | Accepted |
| [ADR-0065](adrs/0065-inbound-mail-plan-binding.md) | The inbound mail chain renders as an explicit plan binding: synthesized wake queue and trigger, SES receipt rule with scanning disabled, SES-only writes, worker read-only raw access, lifecycle retention, and cost stated as assumptions. | Accepted |
| [ADR-0066](adrs/0066-ticket-types-and-typed-forms.md) | Tickets carry a bounded taxonomy (question/incident/problem/task, question the default) and typed form answers: unique bounded field ids, exactly one value slot per kind, RFC 3339 date-times, f64-safe integers — no form-definition DSL. | Accepted |
