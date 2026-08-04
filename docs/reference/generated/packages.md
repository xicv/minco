# Package reference

<!-- @generated; do not edit by hand -->
<!-- generated-reference-schema: 1 -->

Generator: `scripts/docs/generate_reference.py` schema `1`.

Authorities:

- `Cargo.toml [workspace.package]`
- `Cargo.toml [workspace.metadata.minco.release]`
- `each publishable package Cargo.toml`

Regenerate with `scripts/docs/generate-reference.sh`; use `--check` to verify byte-for-byte freshness.

Workspace version: `0.7.0`. MSRV: `1.97.1`. Publishable packages: `29`.

Publication is dependency ordered. A docs.rs link is present for every public package; archive-smoke packages are the subset exercised independently before publication.

| Order | Package | Purpose | Archive smoke | Rust API |
|---:|---|---|:---:|---|
| 1 | `minco-core` | Provider-neutral application graph, static plugin composition, capabilities, and typed services for Minco | no | [docs.rs](https://docs.rs/minco-core/0.7.0/minco_core/) |
| 2 | `minco-config` | Provider-neutral typed configuration, environment, and secret-reference graph for Minco | yes | [docs.rs](https://docs.rs/minco-config/0.7.0/minco_config/) |
| 3 | `minco-db` | Provider-neutral migration and safe seed lifecycle models for Minco | yes | [docs.rs](https://docs.rs/minco-db/0.7.0/minco_db/) |
| 4 | `minco-dev` | Deterministic local process plans and coordinated development supervision for Minco | yes | [docs.rs](https://docs.rs/minco-dev/0.7.0/minco_dev/) |
| 5 | `minco-contract` | OpenAPI-first contract validation, operation inventory, and deterministic Rust binding generation for Minco | yes | [docs.rs](https://docs.rs/minco-contract/0.7.0/minco_contract/) |
| 6 | `minco-http` | Axum and Tower HTTP conventions, principals, request IDs, limits, and RFC 9457 errors for Minco | no | [docs.rs](https://docs.rs/minco-http/0.7.0/minco_http/) |
| 7 | `minco-release` | Immutable release manifests and artifact digest verification for Minco | no | [docs.rs](https://docs.rs/minco-release/0.7.0/minco_release/) |
| 8 | `minco-plugin-static-site` | Provider-neutral static-site deployment intent for Minco applications | no | [docs.rs](https://docs.rs/minco-plugin-static-site/0.7.0/minco_plugin_static_site/) |
| 9 | `minco-deploy-aws` | Fail-closed AWS deployment guards and CloudFormation change-set review for Minco | yes | [docs.rs](https://docs.rs/minco-deploy-aws/0.7.0/minco_deploy_aws/) |
| 10 | `minco-test` | In-process HTTP clients, deterministic fixtures, and command evidence for Minco applications | no | [docs.rs](https://docs.rs/minco-test/0.7.0/minco_test/) |
| 11 | `minco-plan` | Deployment plans, database cost profiles, structural policy checks, and AWS SAM rendering for Minco | no | [docs.rs](https://docs.rs/minco-plan/0.7.0/minco_plan/) |
| 12 | `minco-plugin-health` | Health, readiness, and dependency-check plugin for Minco applications | no | [docs.rs](https://docs.rs/minco-plugin-health/0.7.0/minco_plugin_health/) |
| 13 | `minco-plugin-observability` | Structured tracing and CloudWatch-compatible JSON logging plugin for Minco applications | no | [docs.rs](https://docs.rs/minco-plugin-observability/0.7.0/minco_plugin_observability/) |
| 14 | `minco-plugin-idempotency` | Idempotency keys, request fingerprints, and storage ports for Minco applications | no | [docs.rs](https://docs.rs/minco-plugin-idempotency/0.7.0/minco_plugin_idempotency/) |
| 15 | `minco-plugin-sessions` | Provider-neutral, revocable browser and API session primitives for Minco | no | [docs.rs](https://docs.rs/minco-plugin-sessions/0.7.0/minco_plugin_sessions/) |
| 16 | `minco-plugin-identity` | Verified-claims identity mapping and permission authorization for Minco | no | [docs.rs](https://docs.rs/minco-plugin-identity/0.7.0/minco_plugin_identity/) |
| 17 | `minco-plugin-object-storage` | Provider-neutral object storage port and reference memory implementation for Minco | no | [docs.rs](https://docs.rs/minco-plugin-object-storage/0.7.0/minco_plugin_object_storage/) |
| 18 | `minco-plugin-events` | Domain event publisher and transactional outbox ports for Minco | no | [docs.rs](https://docs.rs/minco-plugin-events/0.7.0/minco_plugin_events/) |
| 19 | `minco-plugin-notifications` | Provider-neutral notification delivery port and reference memory sink for Minco | no | [docs.rs](https://docs.rs/minco-plugin-notifications/0.7.0/minco_plugin_notifications/) |
| 20 | `minco-plugin-audit` | Append-only audit event port and reference memory sink for Minco | no | [docs.rs](https://docs.rs/minco-plugin-audit/0.7.0/minco_plugin_audit/) |
| 21 | `minco-sqlx-postgres` | Bounded SQLx PostgreSQL pools, migrations, and safe seed execution for Minco | no | [docs.rs](https://docs.rs/minco-sqlx-postgres/0.7.0/minco_sqlx_postgres/) |
| 22 | `minco-sqlx-sqlite` | SQLx SQLite pools, migrations, and safe seed execution for Minco applications | no | [docs.rs](https://docs.rs/minco-sqlx-sqlite/0.7.0/minco_sqlx_sqlite/) |
| 23 | `minco-plugin-feedback` | AI-ready client feedback loops with screenshots, voice, discussion, persistence, and an embeddable widget for Minco | no | [docs.rs](https://docs.rs/minco-plugin-feedback/0.7.0/minco_plugin_feedback/) |
| 24 | `minco-plugin-realtime` | Provider-neutral subscriber-only realtime publication for Minco applications | yes | [docs.rs](https://docs.rs/minco-plugin-realtime/0.7.0/minco_plugin_realtime/) |
| 25 | `minco-aws-adapters` | Production AWS and signed-webhook adapters for Minco official plugin ports | no | [docs.rs](https://docs.rs/minco-aws-adapters/0.7.0/minco_aws_adapters/) |
| 26 | `minco-aws-lambda` | Native AWS Lambda HTTP runtime, API Gateway principal mapping, and SSM loading for Minco | no | [docs.rs](https://docs.rs/minco-aws-lambda/0.7.0/minco_aws_lambda/) |
| 27 | `minco-aws-worker` | Explicit AWS Lambda SQS partial-batch worker runtime for Minco | no | [docs.rs](https://docs.rs/minco-aws-worker/0.7.0/minco_aws_worker/) |
| 28 | `minco` | Contract-first, AI-native, AWS-native Rust web framework with static plugins and deployment planning | no | [docs.rs](https://docs.rs/minco/0.7.0/minco/) |
| 29 | `cargo-minco` | Cargo subcommand for Minco local development, contracts, plugins, plans, releases, and JJ workflows | no | [docs.rs](https://docs.rs/cargo-minco/0.7.0/cargo_minco/) |
