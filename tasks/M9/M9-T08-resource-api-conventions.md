---
id: M9-T08
title: Standardize resource API conventions and scaffolding
milestone: M9
status: ready
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
  - examples/orders/**
  - docs/DECISIONS.md
  - docs/adrs/0026-resource-api-conventions.md
  - docs/architecture/contract-first.md
  - docs/development/generators.md
  - docs/development/testing.md
  - docs/reference/cli.md
  - roadmap/**
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
- `cargo minco make resource <name>` selects only an already-reviewed complete
  OpenAPI resource family and plans failing application/HTTP specifications
  without inventing domain rules or overwriting files;
- the Orders reference implements the complete resource family through
  use-case-shaped ports, memory, PostgreSQL and SQLite adapters, Axum
  `oneshot` tests, migrations and explainable operation traces;
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

Pending implementation in an isolated JJ workspace.
