# Package reference

<!-- @generated; do not edit by hand -->
<!-- generated-reference-schema: 1 -->

Generator: `scripts/docs/generate_reference.py` schema `1`.

Authorities:

- `Cargo.toml [workspace.package]`
- `Cargo.toml [workspace.metadata.minco.release]`
- `each publishable package Cargo.toml`

Regenerate with `scripts/docs/generate-reference.sh`; use `--check` to verify byte-for-byte freshness.

Workspace version: `1.10.0`. MSRV: `1.97.1`. Publishable packages: `36`.

Publication is dependency ordered. A docs.rs link is present for every public package; archive-smoke packages are the subset exercised independently before publication.

| Order | Package | Purpose | Archive smoke | Rust API |
|---:|---|---|:---:|---|
| 1 | `minco-core` | Provider-neutral application graph, static plugin composition, capabilities, and typed services for Minco | no | [docs.rs](https://docs.rs/minco-core/1.10.0/minco_core/) |
| 2 | `minco-config` | Provider-neutral typed configuration, environment, and secret-reference graph for Minco | yes | [docs.rs](https://docs.rs/minco-config/1.10.0/minco_config/) |
| 3 | `minco-db` | Provider-neutral migration and safe seed lifecycle models for Minco | yes | [docs.rs](https://docs.rs/minco-db/1.10.0/minco_db/) |
| 4 | `minco-dev` | Deterministic local process plans and coordinated development supervision for Minco | yes | [docs.rs](https://docs.rs/minco-dev/1.10.0/minco_dev/) |
| 5 | `minco-contract` | OpenAPI-first contract validation, operation inventory, and deterministic Rust binding generation for Minco | yes | [docs.rs](https://docs.rs/minco-contract/1.10.0/minco_contract/) |
| 6 | `minco-http` | Axum and Tower HTTP conventions, principals, request IDs, limits, and RFC 9457 errors for Minco | no | [docs.rs](https://docs.rs/minco-http/1.10.0/minco_http/) |
| 7 | `minco-release` | Immutable release manifests and artifact digest verification for Minco | no | [docs.rs](https://docs.rs/minco-release/1.10.0/minco_release/) |
| 8 | `minco-plugin-static-site` | Provider-neutral static-site deployment intent for Minco applications | no | [docs.rs](https://docs.rs/minco-plugin-static-site/1.10.0/minco_plugin_static_site/) |
| 9 | `minco-deploy-aws` | Fail-closed AWS deployment guards and CloudFormation change-set review for Minco | yes | [docs.rs](https://docs.rs/minco-deploy-aws/1.10.0/minco_deploy_aws/) |
| 10 | `minco-test` | In-process HTTP clients, deterministic fixtures, and command evidence for Minco applications | no | [docs.rs](https://docs.rs/minco-test/1.10.0/minco_test/) |
| 11 | `minco-plan` | Deployment plans, database cost profiles, structural policy checks, and AWS SAM rendering for Minco | no | [docs.rs](https://docs.rs/minco-plan/1.10.0/minco_plan/) |
| 12 | `minco-project-view` | Bounded, schema-versioned read models for Minco project architecture and evidence | yes | [docs.rs](https://docs.rs/minco-project-view/1.10.0/minco_project_view/) |
| 13 | `minco-mcp` | Local read-only Model Context Protocol server for bounded Minco project views | yes | [docs.rs](https://docs.rs/minco-mcp/1.10.0/minco_mcp/) |
| 14 | `minco-workbench` | Optional local dashboard and deterministic exports for bounded Minco project views | yes | [docs.rs](https://docs.rs/minco-workbench/1.10.0/minco_workbench/) |
| 15 | `minco-plugin-health` | Health, readiness, and dependency-check plugin for Minco applications | no | [docs.rs](https://docs.rs/minco-plugin-health/1.10.0/minco_plugin_health/) |
| 16 | `minco-plugin-observability` | Structured tracing and CloudWatch-compatible JSON logging plugin for Minco applications | no | [docs.rs](https://docs.rs/minco-plugin-observability/1.10.0/minco_plugin_observability/) |
| 17 | `minco-plugin-idempotency` | Idempotency keys, request fingerprints, and storage ports for Minco applications | no | [docs.rs](https://docs.rs/minco-plugin-idempotency/1.10.0/minco_plugin_idempotency/) |
| 18 | `minco-plugin-sessions` | Provider-neutral, revocable browser and API session primitives for Minco | no | [docs.rs](https://docs.rs/minco-plugin-sessions/1.10.0/minco_plugin_sessions/) |
| 19 | `minco-plugin-identity` | Verified-claims identity mapping and permission authorization for Minco | no | [docs.rs](https://docs.rs/minco-plugin-identity/1.10.0/minco_plugin_identity/) |
| 20 | `minco-plugin-object-storage` | Provider-neutral object storage port and reference memory implementation for Minco | no | [docs.rs](https://docs.rs/minco-plugin-object-storage/1.10.0/minco_plugin_object_storage/) |
| 21 | `minco-plugin-events` | Domain event publisher and transactional outbox ports for Minco | no | [docs.rs](https://docs.rs/minco-plugin-events/1.10.0/minco_plugin_events/) |
| 22 | `minco-plugin-notifications` | Provider-neutral notification and rich outbound mail ports with deterministic local test adapters for Minco | no | [docs.rs](https://docs.rs/minco-plugin-notifications/1.10.0/minco_plugin_notifications/) |
| 23 | `minco-plugin-audit` | Append-only audit event port and reference memory sink for Minco | no | [docs.rs](https://docs.rs/minco-plugin-audit/1.10.0/minco_plugin_audit/) |
| 24 | `minco-interaction` | Provider-neutral support entry, attachments, transcription, workflow, and activity helpers for Minco applications | yes | [docs.rs](https://docs.rs/minco-interaction/1.10.0/minco_interaction/) |
| 25 | `minco-sqlx-postgres` | Bounded SQLx PostgreSQL pools, migrations, and safe seed execution for Minco | no | [docs.rs](https://docs.rs/minco-sqlx-postgres/1.10.0/minco_sqlx_postgres/) |
| 26 | `minco-sqlx-sqlite` | SQLx SQLite pools, migrations, and safe seed execution for Minco applications | no | [docs.rs](https://docs.rs/minco-sqlx-sqlite/1.10.0/minco_sqlx_sqlite/) |
| 27 | `minco-plugin-feedback` | AI-ready client feedback loops with screenshots, voice, discussion, persistence, and an embeddable widget for Minco | no | [docs.rs](https://docs.rs/minco-plugin-feedback/1.10.0/minco_plugin_feedback/) |
| 28 | `minco-plugin-ticketing` | Project-scoped support ticketing, atomic handoffs, conversation, SQLite persistence, and support-entry HTTP for Minco | yes | [docs.rs](https://docs.rs/minco-plugin-ticketing/1.10.0/minco_plugin_ticketing/) |
| 29 | `minco-plugin-realtime` | Provider-neutral subscriber-only realtime publication for Minco applications | yes | [docs.rs](https://docs.rs/minco-plugin-realtime/1.10.0/minco_plugin_realtime/) |
| 30 | `minco-plugin-payments-waffo` | Signed Waffo Pancake checkout, payment API, and webhook integration for Minco | yes | [docs.rs](https://docs.rs/minco-plugin-payments-waffo/1.10.0/minco_plugin_payments_waffo/) |
| 31 | `minco-aws-adapters` | Production AWS and signed-webhook adapters for Minco official plugin ports | no | [docs.rs](https://docs.rs/minco-aws-adapters/1.10.0/minco_aws_adapters/) |
| 32 | `minco-aws-dynamodb` | Validated AWS DynamoDB provider primitives for explicit Minco access models | yes | [docs.rs](https://docs.rs/minco-aws-dynamodb/1.10.0/minco_aws_dynamodb/) |
| 33 | `minco-aws-lambda` | Native AWS Lambda HTTP runtime, API Gateway principal mapping, and SSM loading for Minco | no | [docs.rs](https://docs.rs/minco-aws-lambda/1.10.0/minco_aws_lambda/) |
| 34 | `minco-aws-worker` | Explicit AWS Lambda SQS partial-batch worker runtime for Minco | no | [docs.rs](https://docs.rs/minco-aws-worker/1.10.0/minco_aws_worker/) |
| 35 | `minco` | Contract-first, AI-native, AWS-native Rust web framework with static plugins and deployment planning | no | [docs.rs](https://docs.rs/minco/1.10.0/minco/) |
| 36 | `cargo-minco` | Cargo subcommand for Minco local development, contracts, plugins, plans, releases, and JJ workflows | no | [docs.rs](https://docs.rs/cargo-minco/1.10.0/cargo_minco/) |
