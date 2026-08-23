# Facade feature reference

<!-- @generated; do not edit by hand -->
<!-- generated-reference-schema: 1 -->

Generator: `scripts/docs/generate_reference.py` schema `1`.

Authorities:

- `crates/minco/Cargo.toml [features]`
- `crates/minco/Cargo.toml [dependencies]`

Regenerate with `scripts/docs/generate-reference.sh`; use `--check` to verify byte-for-byte freshness.

Features are compile-time composition only. They do not discover plugins, select providers at runtime, or create AWS resources by themselves.

| Feature | Kind | Enables |
|---|---|---|
| `aws-adapters` | AWS adapter/runtime | `dep:minco-aws-adapters`, `minco-aws-adapters/full` |
| `aws-dynamodb` | AWS adapter/runtime | `dep:minco-aws-dynamodb` |
| `aws-lambda` | AWS adapter/runtime | `dep:minco-aws-lambda`, `http` |
| `aws-worker` | AWS adapter/runtime | `dep:minco-aws-worker` |
| `config` | framework plane | `dep:minco-config` |
| `contract` | framework plane | `dep:minco-contract` |
| `db` | framework plane | `dep:minco-db` |
| `default` | bundle | `contract`, `http`, `default-plugins` |
| `default-plugins` | bundle | `plugin-health`, `plugin-observability`, `plugin-idempotency` |
| `full` | bundle | `config`, `db`, `contract`, `http`, `interaction`, `plan`, `release`, `test`, `official-plugins`, `sqlx-postgres`, `sqlx-sqlite`, `aws-adapters`, `aws-dynamodb`, `aws-lambda`, `aws-worker` |
| `http` | framework plane | `dep:minco-http` |
| `interaction` | framework plane | `dep:minco-interaction` |
| `official-plugins` | bundle | `default-plugins`, `plugin-payments-waffo`, `plugin-sessions`, `plugin-identity`, `plugin-object-storage`, `plugin-events`, `plugin-jobs`, `plugin-notifications`, `plugin-audit`, `plugin-feedback`, `plugin-ticketing`, `plugin-static-site`, `plugin-realtime` |
| `plan` | framework plane | `dep:minco-plan`, `contract` |
| `plugin-audit` | plugin | `dep:minco-plugin-audit` |
| `plugin-events` | plugin | `dep:minco-plugin-events` |
| `plugin-feedback` | plugin | `dep:minco-plugin-feedback`, `plugin-health`, `plugin-identity`, `plugin-object-storage`, `plugin-events`, `plugin-notifications`, `plugin-audit`, `http`, `minco-plugin-feedback/http` |
| `plugin-health` | plugin | `dep:minco-plugin-health` |
| `plugin-idempotency` | plugin | `dep:minco-plugin-idempotency` |
| `plugin-identity` | plugin | `dep:minco-plugin-identity`, `http` |
| `plugin-jobs` | plugin | `dep:minco-plugin-jobs` |
| `plugin-notifications` | plugin | `dep:minco-plugin-notifications` |
| `plugin-object-storage` | plugin | `dep:minco-plugin-object-storage` |
| `plugin-observability` | plugin | `dep:minco-plugin-observability` |
| `plugin-payments-waffo` | plugin | `dep:minco-plugin-payments-waffo`, `plugin-idempotency` |
| `plugin-realtime` | plugin | `dep:minco-plugin-realtime` |
| `plugin-sessions` | plugin | `dep:minco-plugin-sessions` |
| `plugin-static-site` | plugin | `dep:minco-plugin-static-site` |
| `plugin-ticketing` | plugin | `dep:minco-plugin-ticketing`, `interaction`, `plugin-health`, `plugin-identity`, `plugin-object-storage`, `plugin-events`, `plugin-notifications`, `plugin-audit`, `http`, `minco-plugin-ticketing/http` |
| `release` | framework plane | `dep:minco-release` |
| `sqlx-postgres` | database adapter | `db`, `dep:minco-sqlx-postgres` |
| `sqlx-sqlite` | database adapter | `db`, `dep:minco-sqlx-sqlite` |
| `test` | framework plane | `dep:minco-test`, `http` |
