---
title: Project Structure
description: Know where contracts, business rules, adapters, delivery, composition, infrastructure, and evidence belong.
---

# Project Structure

A generated application makes dependency direction visible. Names vary by
application, but ownership does not.

```text
app/
├── openapi/                 reviewed external contract
├── crates/
│   ├── domain/              invariants and state transitions
│   ├── application/         use cases and owned ports
│   ├── adapters/            memory, SQLx, or provider implementations
│   └── api/                 Axum extraction, response mapping, contract tests
├── services/app/            composition root and runtime entry points
├── migrations/              backend-specific, attributable schema changes
├── seeds/                   classified and independently verified data sets
├── config/                  typed environment layers and secret references
├── infra/                   generated or reviewed deployment inputs
├── tasks/                   bounded work ownership and verification commands
└── minco.toml               graph, runtime, database, and deployment intent
```

## Dependency Rules

| Layer | May know | Must not know |
|---|---|---|
| Domain | domain values and rules | Axum, SQLx, Lambda, AWS SDKs, Minco HTTP or Plan crates |
| Application | domain and use-case-shaped ports | Axum, SQLx, Lambda, AWS SDKs |
| Adapter | application-owned ports and selected engine | unrelated delivery or composition policy |
| HTTP | request/response mapping and one use case | SQL and transaction implementation |
| Composition | every selected concrete implementation | business rules hidden in wiring |

## One Operation End to End

For `updateOrder`, the external contract requires a strong `If-Match` value.
The HTTP layer parses it, the application use case performs authorization and
validation, and the adapter applies the revision predicate atomically. A stale
revision becomes a stable `412` Problem response.

```text
PATCH /orders/{id}
  → parse If-Match
  → UpdateOrder use case
  → OrderWriter::update_if_revision
  → ResourceDocument<OrderResponse> + new ETag
```

Each boundary has its own test. That makes a failure attributable instead of
depending on one large end-to-end test to diagnose every layer.

## Generated Versus Owned Source

Generated contract bindings are committed and reproducible. Generators create
structure and failing specifications, not fake business behavior. Review the
plan before writing:

```bash
cargo minco make operation updateOrder --dry-run --json
cargo minco make resource order --dry-run --json
```

Application code owns authorization, fields, invariants, transaction scope,
audit, retention, and delete policy. Minco owns only the cross-application
conventions that can be checked deterministically.

## Inspect Instead of Guessing

```bash
cargo minco inspect --json
cargo minco explain updateOrder --json
cargo minco config schema --json
cargo minco deploy plan --stdout --json
```

These commands expose bounded metadata and provenance. They must not serialize
credentials, secret-reference names, service values, or customer data.
