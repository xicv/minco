---
id: M9-T08
title: Standardize resource API conventions and scaffolding
milestone: M9
status: complete
priority: high
area: contract/developer-experience
depends_on: [M9-T07]
operations: [placeOrder, listOrders, getOrder, updateOrder, deleteOrder]
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - crates/minco-contract/**
  - crates/minco-http/**
  - crates/minco-cli/**
  - crates/minco/**
  - crates/minco-plan/**
  - examples/orders/**
  - infra/aws/generated/**
  - docs/DECISIONS.md
  - docs/adrs/0026-resource-api-conventions.md
  - docs/architecture/contract-first.md
  - docs/development/generators.md
  - docs/development/testing.md
  - docs/reference/cli.md
  - roadmap/**
  - scripts/test/e2e.sh
  - scripts/test/generated_apps.sh
  - scripts/test/scaffold_templates.py
  - tasks/M9/M9-T08-resource-api-conventions.md
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo minco contract check
  - cargo minco contract sync --check
  - cargo test -p minco-contract -p minco-http -p cargo-minco --all-features --locked
  - cargo test -p orders-domain -p orders-application -p orders-adapters -p orders-api --all-features --locked
  - cargo clippy -p minco-contract -p minco-http -p cargo-minco -p orders-domain -p orders-application -p orders-adapters -p orders-api --all-targets --all-features --locked -- -D warnings
  - scripts/test/generated_apps.sh
---

## Goal

Define a small OpenAPI-first resource convention that makes ordinary create,
list, read, update and delete APIs predictable for clients and generators
without adding an ORM, Active Record model, generic repository, hidden SQL or
business behavior.

## Acceptance

- each resource operation declares validated `x-minco-resource` identity and
  action metadata;
- create, read and update return a stable JSON `data` envelope, list returns
  `data` plus cursor-page metadata, delete returns `204`, and errors remain RFC
  9457 `application/problem+json`;
- collection queries use bounded cursor pagination, a deterministic stable
  sort, and operation-declared allowlisted filters/sort fields;
- read/create/update responses expose a strong `ETag`; update and delete
  require `If-Match`, return `428` when absent and `412` when stale, and fail
  before persistence on invalid or unauthorized requests;
- idempotent create retries replay the immutable original response and remain
  valid after the resource is updated or deleted;
- `cargo minco make resource <name>` selects only an already-reviewed complete
  OpenAPI resource family and plans failing application/HTTP specifications
  without inventing domain rules or overwriting files;
- the Orders reference implements the complete resource family through
  use-case-shaped ports, memory, PostgreSQL and SQLite adapters, Axum
  `oneshot` tests, migrations and explainable operation traces;
- generated OpenAPI/SAM groups every method under one path item so a resource
  family remains valid when collection and member paths serve multiple actions;
- real-service E2E proves the standardized document and collection envelopes,
  immutable create replay, conditional update, stale-write rejection and `204`
  delete through the SQLite composition;
- contract diff and generated bindings make the breaking response-shape and
  new-operation boundary explicit.

## Non-goals

- a generic CRUD repository, ORM, Active Record, database query language or
  automatic persistence implementation;
- arbitrary client-supplied SQL fields, operators or sort expressions;
- offset pagination as the default;
- deciding application-specific authorization, validation, soft-delete,
  audit, retention or transaction rules in framework core;
- publishing a crate, deploying an application, mutating AWS, or preparing a
  release before this task is independently reviewed and merged.

## Evidence

Implemented and locally qualified on 2026-07-31 in the isolated
`minco-task-m9-t08` JJ workspace against merged-main ancestor
`ca071aa8a4b30c9538033b781772bec486f3a97b`.

- `./scripts/quality.sh` passed the complete repository gate: formatting,
  repository/static/deep validation, contract checks, generated PostgreSQL and
  SQLite application checks, workspace compiler/tests/clippy, browser tests
  (40 passed), documentation, dependency policy, RustSec (0 vulnerabilities),
  secret scanning (0 leaks), and source-manifest verification.
- Focused contract suites passed 23 policy and 23 compatibility tests. The
  synchronized Orders contract SHA-256 is
  `f0e0d1aee9858e54f814270b6f78a5c63ea6993311210f6a6f6ee7323838843f`.
- Orders domain, application, Axum, memory and SQLite suites passed. The seven
  SQLite adapter tests include legacy migration backfill and immutable
  idempotent-create replay after later update/deletion.
- The SAM renderer regression proves multiple methods share one OpenAPI path
  item. `scripts/aws/plan.sh` regenerated the deployment snapshots and
  `scripts/aws/validate.sh` passed with SAM CLI 1.164.0.
- Four real-PostgreSQL adapter tests compiled but remained ignored because
  `MINCO_ORDERS_TEST_POSTGRES_URL` was not supplied. No PostgreSQL runtime pass
  is claimed.
- SQLite migration `0002_resource_concurrency.sql` deliberately rebuilds the
  idempotency table to make the response snapshot non-null. Deep review reports
  `REVIEW-SQL-001` for its `DROP TABLE`; the migration is classified
  `data_rewrite`, requires explicit `--allow-destructive`, runs transactionally,
  and has a backfill/replay regression test.
- Exact-source ARM64 qualification artifacts were built locally:
  `orders-lambda/bootstrap.zip` is 5,102,419 bytes with SHA-256
  `8a14f40778fea9f235a99a8ed3004eb2faffc505e2f7275be39f13bb409b9ffc`;
  `sqs_worker/bootstrap.zip` is 574,199 bytes with SHA-256
  `c1508117d7329029aaedc85691b416f3321d1fa11831c5c162f9647465bd3a44`.
- Hosted CI run `30607066699` independently passed the authoritative quality,
  publish dry-run, Plan/SAM/native ARM64 and Rustack conformance steps. Its E2E
  step correctly exposed the stale pre-convention response assertions in
  `scripts/test/e2e.sh`; the test now exercises the standardized full resource
  lifecycle. AWS deployment, crate publication and release preparation remain
  intentionally outside this task and are not claimed here.
