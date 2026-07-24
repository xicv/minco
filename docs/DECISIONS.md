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
