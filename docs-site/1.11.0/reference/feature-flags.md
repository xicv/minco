---
title: Cargo Feature Flags
description: Choose Minco facade features and bundles without compiling unused providers.
---

# Cargo Feature Flags

Cargo features choose code at compile time. Runtime plugin selection chooses
among components already compiled into the binary. Neither action creates AWS
resources.

## Default Surface

```toml
[dependencies]
minco = "1.10.0"
```

The default enables `contract`, `http`, and `default-plugins`. The default
plugin bundle contains health, observability, and idempotency.

## Framework Planes

| Feature | Enables |
|---|---|
| `config` | strict typed configuration graph |
| `db` | provider-neutral database lifecycle contracts |
| `contract` | OpenAPI validation, indexing, and compatibility |
| `http` | resource shapes and Axum/Tower conventions |
| `plan` | Plan IR and contract-aware deployment planning |
| `release` | exact artifact and release manifests |
| `test` | public test and plugin conformance helpers |

## Plugins

| Feature | Component | Default |
|---|---|:---:|
| `plugin-health` | liveness, readiness, dependency health | yes |
| `plugin-observability` | structured tracing and CloudWatch-compatible logs | yes |
| `plugin-idempotency` | keys, fingerprints, and a storage port | yes |
| `plugin-sessions` | session issuance, lookup, expiry, revocation | no |
| `plugin-identity` | verified claims, identities, scopes, permissions | no |
| `plugin-object-storage` | uploads, exports, attachments | no |
| `plugin-events` | domain events and transactional-outbox ports | no |
| `plugin-notifications` | email, webhook, in-app, developer channels | no |
| `plugin-audit` | append-only audit history | no |
| `plugin-feedback` | client review loop and AI handoff | no |
| `plugin-ticketing` | portal-first project support, atomic handoffs, and ticket conversation | no |
| `plugin-static-site` | private assets and CDN deployment intent | no |
| `plugin-realtime` | ephemeral backend publication and subscriber-only browser invalidation | no |
| `plugin-payments-waffo` | Waffo hosted checkout, read-only queries, and verified webhooks | no |

`plugin-feedback` also enables the plugin capabilities it composes: health,
identity, object storage, events, notifications, audit, and HTTP.

## Adapters and Runtimes

| Feature | Component |
|---|---|
| `sqlx-postgres` | PostgreSQL pool and explicit migration adapter |
| `sqlx-sqlite` | SQLite adapter with explicit durability constraints |
| `aws-adapters` | opt-in S3, SQS, SSM, and related provider adapters |
| `aws-dynamodb` | validated DynamoDB client, table intent, and readiness boundary |
| `aws-lambda` | native Lambda HTTP runtime and SSM configuration loading |
| `aws-worker` | SQS Lambda partial-batch worker runtime |

## Bundles

`official-plugins` enables all official plugins. `full` additionally enables
every framework plane, all three database adapters, and all AWS runtimes and
adapters. Use
`full` for framework qualification; application binaries should normally pick
the smaller explicit set they operate.

```toml
[dependencies]
minco = { version = "1.10.0", features = [
  "config",
  "plan",
  "release",
  "test",
  "sqlx-postgres",
  "aws-lambda",
  "plugin-identity",
  "plugin-sessions",
] }
```

The generated authority is
[`docs/reference/generated/features.md`](https://github.com/xicv/minco/blob/main/docs/reference/generated/features.md).
It is derived from `crates/minco/Cargo.toml` and checked byte-for-byte in the
repository quality gate.
