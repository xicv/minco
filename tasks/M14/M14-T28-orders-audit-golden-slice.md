---
id: M14-T28
title: Prove semantic auditing in the Orders golden project
milestone: M14
status: done
priority: critical
area: examples/orders/audit
depends_on: [M14-T27]
operations: [placeOrder, updateOrder, deleteOrder, listOrderAuditHistory]
owned_paths:
  - Cargo.lock
  - crates/minco-cli/src/main.rs
  - crates/minco-plan/src/cost.rs
  - crates/minco-plan/src/lib.rs
  - crates/minco-plan/src/model.rs
  - crates/minco-plan/src/sam.rs
  - crates/minco-plan/tests/fixtures/api_dynamodb_explicit_v2.toml
  - crates/minco-plan/tests/multi_runtime.rs
  - crates/minco-project-view/src/reader.rs
  - examples/orders/**
  - extensions/minco-aws-dynamodb/src/audit_v2.rs
  - docs/deployment/dynamodb-orders.md
  - docs/deployment/aws-plugin-adapters.md
  - docs/development/quickstart.md
  - docs/reference/generated/schemas.md
  - docs/research/audit-ledger-costs-2026-08.md
  - infra/aws/generated/**
  - minco.toml
  - plugins/minco-plugin-audit/src/lib.rs
  - scripts/dev/rustack-dynamodb-smoke.sh
  - scripts/generate_bootstrap_artifacts.py
  - scripts/test/quality_assurance.py
  - scripts/test/release_identity.py
  - roadmap/tasks.mmd
  - tasks/M14/M14-T28-orders-audit-golden-slice.md
  - verification/source-manifest.json
  - verification/static-validation.json
  - verification/1.5-performance-baseline.json
  - verification/operational-evidence-validation.json
  - verification/release-identity.json
  - verification/quality-assurance-policy.toml
  - verification/cost-regression-baseline.json
checks:
  - cargo minco contract check
  - cargo minco contract sync --check
  - cargo test -p orders-domain -p orders-application -p orders-adapters -p orders-api -p orders-service --all-features --locked
  - cargo test -p cargo-minco --bin cargo-minco --locked
  - cargo clippy -p orders-domain -p orders-application -p orders-adapters -p orders-api -p orders-service --all-targets --all-features --locked -- -D warnings
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/source_manifest.py --check
---

## Goal

Prove ADR-0043 through one complete reference slice: authorized Orders commands
create attributable semantic audit intents, adapters commit them with domain
state under each provider's declared atomicity model, and an independently
authorized history use case reads the separate schema-agnostic ledger.

## Acceptance

- place, update and soft-delete actions retain actor, operation, request
  correlation, resource revision and allowlisted privacy-safe changes;
- PostgreSQL and SQLite enqueue the intent inside the source mutation
  transaction and an explicit bounded relay delivers to a physically separate
  ledger without an implicit worker or schedule;
- DynamoDB uses one `TransactWriteItems` call across the Orders table and its
  distinct audit table, including the resource-history projection;
- idempotent place retries do not create a second semantic action, conditional
  races leave neither a false audit event nor a partial source mutation, and
  provider failures remain redacted;
- `GET /orders/{orderId}/audit` is contract-first, permission-gated, bounded and
  cursor-paginated, including after the operational row is soft-deleted; and
- configuration, Plan/SAM/IAM, `minco explain`, application fake-port tests,
  Axum contract tests and adapter behavioral tests expose the complete slice.
- the repository-wide release identity regression follows the authoritative
  repository and documentation state instead of retaining the pre-publication
  1.5.0 candidate literals that blocked quality on current `main`.

## Non-goals

- database triggers, ORM hooks, DynamoDB Streams or CDC as the authoritative
  actor-aware audit source;
- an implicit relay schedule, distributed SQL transaction or synchronous
  cross-database SQL write;
- generic CRUD repositories or a generic database abstraction; or
- automatic archive deletion, legal-hold policy or an audit export endpoint.

## Evidence

- `cargo minco contract check`, `cargo minco contract sync --check`, and
  `cargo minco explain listOrderAuditHistory --json` passed for contract digest
  `f43837b7e41a03b311588dd5466f7029d4bf1e8940685bd194bc1bc97abd796c`.
- The locked ten-crate audit-slice command passed. It executed 6 adapter unit tests,
  8 SQLite behavioral tests, 4 HTTP tests, 4 application tests, 3 domain tests
  and 4 composition-root tests. Five PostgreSQL tests remain explicitly
  environment-gated in the ordinary suite.
- The SQLite golden test proved source+journal commit, explicit bounded relay,
  idempotent replay, stale-revision rollback, three-event history after soft
  deletion, and physical separation of source and ledger files.
- The matching PostgreSQL golden test passed against two disposable PostgreSQL
  18 databases, proving the same transactional journal, bounded relay,
  idempotency, stale-race and post-deletion history behavior; its isolated
  container, network and volume were then absence-verified as removed.
- Three parallel PostgreSQL plugin-store regression runs passed after removing
  a host/database clock-skew boundary from outbox claims.
- `scripts/dev/rustack-dynamodb-smoke.sh` passed against pinned Rustack
  `0.9.1@sha256:18cd91395e17453e2c34b299e45f4679dc2427473dc1db6541bbe212fd70a104`.
  It exercised cross-table transactions, retries, conditional races, queries,
  soft deletion and absence-verified cleanup for both disposable tables.
- Strict Clippy passed for `minco-aws-dynamodb` and all five Orders crates with
  all targets/features. `cargo test -p minco-aws-dynamodb --all-features
  --locked` passed its seven local tests with one Rustack-gated test ignored.
- DynamoDB Plan and SAM rendering passed with two retained, deletion-protected,
  PITR-enabled tables; exact separate table IAM; no DynamoDB wildcard; the
  audit environment reference; and the new authenticated history route.
- The relational Plan/SAM path now requires a distinct audit database SSM
  `SecureString`, with exact parameter IAM and an independently constrained
  optional KMS key; the Lambda loads both database URLs without placing either
  secret value in Plan, SAM, manifests or static environment configuration.
- The complete 147-test `cargo-minco` binary suite passed after its manifest
  graph and bounded-registration expectations were updated for the selected
  audit plugin and both V1/V2 audit service registrations.
- Static validation passed with 0 errors and 0 warnings using the repository's
  locked `uv 0.12.3`; generated references, Plan/SAM, task graph and source
  manifest were refreshed and checked.
- The measured nextest policy was advanced from 126 to 127 executable tests
  after the audit SAM boundary added one test inside the policy's reviewed
  `minco-plan` scope; Cargo listed the same 127 executable tests plus one
  doctest, and the policy regression now binds that intentional inventory.
- The release SemVer gate initially found that audit-table, deletion-protection
  and PITR-pricing fields had been added to exhaustively constructible public
  `minco-plan` types. The audit table is now derived deterministically from the
  selected `audit.ledger` capability and explicit operational table instead.
  The separate table, exact IAM, PITR, retention and deletion protection remain
  visible in Plan/SAM, while all 196 compatibility checks for `minco-plan`
  pass against immutable `v1.4.0`. The audit-aware cost total remains
  truthfully incomplete until a separate regional PITR price is supplied.
- `MINCO_QUALITY_TOOL_ROOT=/Users/xicao/.cargo scripts/ci/local-assurance.sh
  --ephemeral` passed the complete measured nextest, coverage, mutation,
  34-package SemVer and release-candidate load policy after the compatibility
  repair; its receipt was generated and independently verified.
- A security pass confirmed the dedicated `orders.audit.read` permission,
  bounded cursor pages and record sizes, digested customer/idempotency values,
  parameterized SQL, hashed DynamoDB resource partitions, redacted provider
  failures and no committed credentials or secret values.
- Fresh RustSec and dependency-policy scans passed with one existing
  informational panic-safety warning in transitive `lru 0.16.4` through
  `aws-sdk-s3`; the current compatible AWS SDK release has no patched `lru`
  requirement, and the audit path does not use that dependency.
- `scripts/quality.sh` initially stopped at `checked operational-evidence
  receipt is stale`. After explicit user authorization, the existing `NOT RUN`
  performance baseline was rebound to this exact verified source tree without
  claiming a performance run or provider contact, and the deterministic
  operational-evidence receipt was regenerated and checked.
