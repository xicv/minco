---
id: M6-T01
title: Design and implement the explicit DynamoDB Orders access model
milestone: M6
status: ready
priority: medium
area: persistence/dynamodb
depends_on: [M4-T01, M5-T01, M9-T08, M12-T06]
operations: [placeOrder, listOrders, getOrder, updateOrder, deleteOrder]
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - crates/minco-plan/src/lib.rs
  - crates/minco-plan/src/model.rs
  - crates/minco-plan/src/sam.rs
  - crates/minco-plan/tests/**
  - extensions/minco-aws-dynamodb/**
  - examples/orders/adapters/Cargo.toml
  - examples/orders/adapters/src/lib.rs
  - examples/orders/adapters/src/dynamodb.rs
  - examples/orders/adapters/tests/dynamodb.rs
  - examples/orders/service/Cargo.toml
  - examples/orders/service/src/lib.rs
  - examples/orders/service/src/bin/lambda.rs
  - examples/orders/config/minco.dynamodb.toml
  - infra/local/compose.yaml
  - scripts/dev/rustack-dynamodb-smoke.sh
  - docs/DECISIONS.md
  - docs/adrs/0032-access-pattern-dynamodb.md
  - docs/deployment/dynamodb-orders.md
  - docs/reference/generated/packages.md
  - docs/reference/supported-matrix.md
  - roadmap/roadmap.yaml
  - roadmap/tasks.mmd
  - tasks/M6/M6-T01-dynamodb-adapter.md
  - verification/deep-review.json
  - verification/publish-validation.json
  - verification/repository-truth.toml
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p minco-aws-dynamodb --all-features --locked
  - cargo test -p orders-adapters --features dynamodb --test dynamodb --locked
  - cargo test -p orders-service --features dynamodb --locked
  - cargo test -p minco-plan --locked
  - scripts/dev/rustack-dynamodb-smoke.sh
  - cargo minco deploy plan --config examples/orders/config/minco.dynamodb.toml --stdout
  - cargo minco cost --config examples/orders/config/minco.dynamodb.toml
  - ./scripts/quality.sh
---

## Goal

Implement DynamoDB as a distinct, selectable Orders access model with an
explicit table contract, conditional writes, bounded index queries and exact
Plan/SAM/IAM behavior. Do not pretend it is a SQLx PostgreSQL substitute.

This is a post-1.0 descendant of the exact M12-T06 source candidate. It must not
rewrite or weaken that candidate's evidence; it records its own exact source
and qualification evidence.

## Acceptance

- The application-owned adapter implements all five current Orders ports. A
  partial `placeOrder`/`getOrder` store is not selectable and is not accepted.
- `placeOrder` atomically creates the canonical order and immutable
  idempotency response. Concurrent same-fingerprint requests commit one order
  and replay it; a different fingerprint returns the existing stable conflict.
- Direct reads are explicitly strongly consistent. List reads declare their
  index consistency, use bounded key queries rather than `Scan`, preserve every
  allowlisted sort, filter and opaque cursor contract, and exclude soft-deleted
  orders.
- Update and soft delete use item-level revision conditions so stale writes
  return `PreconditionFailed` and missing/deleted items return `NotFound`.
- Malformed stored items fail closed. Throttling, transport, authorization and
  provider failures map to redacted `StoreError` variants without item data,
  credentials, endpoints, table names or provider request bodies in public
  responses or retained evidence.
- The official AWS extension supplies a real descriptor, typed configuration,
  validated endpoint behavior, resource/cost intent and tests. It supplies no
  generic CRUD repository and owns no Orders business semantics.
- Plan IR carries an explicit access-pattern table contract. Generic DynamoDB
  SAM still fails closed when that contract is absent. When present, SAM emits
  the exact on-demand table, indexes, runtime table reference and least-
  privilege item/query/describe IAM without `dynamodb:*` or `Resource: "*"`.
- The same standard AWS SDK client uses a Rustack loopback endpoint locally and
  regional AWS endpoints in production. Rustack conformance creates unique
  local resources, exercises all five ports, and removes every resource on
  exit.
- Cost output keeps reads, writes, transactions, indexes, retained storage,
  backups and missing regional rates visible. No zero-traffic request charge
  is turned into a complete zero-dollar claim.
- No real AWS mutation is authorized by this task. A later bounded provider
  smoke needs a separate exact account/Region/resource/spend/cleanup approval.

## Architecture boundary

ADR-0032 owns the split: the Orders crate owns the access pattern and port
implementation; `minco-aws-dynamodb` owns provider configuration and typed AWS
resource support; Plan owns only the explicit serialized deployment contract
and deterministic renderer. Domain and application crates gain no AWS or Plan
dependency.

## Red-first evidence

Before implementation, `cargo minco task verify M6-T01 --json` failed at the
first check with:

```text
error: package ID specification `minco-aws-dynamodb` did not match any packages
```

The task was created on 2026-07-24 for only `placeOrder` and `getOrder`. The
five-operation resource convention, including list cursors, revision-checked
update and soft delete, landed on 2026-07-31. The current Orders composition
root requires one selected store to implement all five ports, so the old
two-port/two-path scope could never provide a selectable database profile. This
approved re-scope repairs that stale task contract before source implementation.
