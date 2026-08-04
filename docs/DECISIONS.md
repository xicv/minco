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
| [ADR-0013](adrs/0013-quality-and-update.md) | Keep local quality gates authoritative and make updates explicit, reviewable and non-self-replacing. | Accepted |
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
