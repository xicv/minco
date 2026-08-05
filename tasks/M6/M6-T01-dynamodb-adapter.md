---
id: M6-T01
title: Design and implement the explicit DynamoDB Orders access model
milestone: M6
status: complete
priority: medium
area: persistence/dynamodb
depends_on: [M4-T01, M5-T01, M9-T08, M12-T06]
operations: [placeOrder, listOrders, getOrder, updateOrder, deleteOrder]
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - README.md
  - minco.toml
  - crates/minco/Cargo.toml
  - crates/minco-plan/src/lib.rs
  - crates/minco-plan/src/model.rs
  - crates/minco-plan/src/sam.rs
  - crates/minco-plan/tests/**
  - crates/minco-cli/tests/plugin_cli.rs
  - extensions/minco-aws-dynamodb/**
  - plugins/catalog.toml
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
  - scripts/test/repository_truth.py
  - scripts/validate_static.py
  - docs/DECISIONS.md
  - docs/adrs/0032-access-pattern-dynamodb.md
  - docs/deployment/dynamodb-orders.md
  - docs/reference/generated/diagnostics.md
  - docs/reference/generated/features.md
  - docs/reference/generated/packages.md
  - docs/reference/generated/plugins.md
  - docs/reference/generated/schemas.md
  - docs/reference/supported-matrix.md
  - roadmap/roadmap.yaml
  - roadmap/tasks.mmd
  - tasks/M6/M6-T01-dynamodb-adapter.md
  - verification/deep-review.json
  - verification/publish-validation.json
  - verification/repository-truth.toml
  - verification/rust-dependency-hygiene.json
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
  - cargo minco explain placeOrder --json
  - cargo minco explain listOrders --json
  - cargo minco explain getOrder --json
  - cargo minco explain updateOrder --json
  - cargo minco explain deleteOrder --json
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

## Completion evidence

Completed locally on 2026-08-05 as a post-candidate descendant of the exact
M12-T06 source candidate. The immutable candidate remains bound to source-tree
SHA-256 `a678ebf52b4c7a9f60ae9adb43221984b5f5884d44f5d0400e14781a5bf28554`;
repository truth now distinguishes that evidence from the current descendant
instead of rewriting the adoption report.

Implementation:

- `minco-aws-dynamodb` is a separately selectable, publishable official
  provider extension using `aws-sdk-dynamodb` 1.120.0. Typed configuration
  rejects unknown fields, invalid provider names, insecure remote endpoints,
  userinfo, queries and paths. Debug and provider errors redact table names,
  endpoints, item data and SDK response bodies. The extension contains no
  Orders types or generic repository.
- The Orders adapter implements all five ports. Create transactionally stores
  the canonical order and immutable hashed-key idempotency response; concurrent
  retries settle to one order, different fingerprints conflict, and the replay
  snapshot survives update and soft deletion. Direct and classification reads
  are strong. Update and delete are revision-conditional and malformed items
  fail closed.
- Listing uses only GSI `Query`: 16 calculated shards, at most eight concurrent
  shard queries and 128 pages per shard. Three explicit indexes preserve every
  current `createdAt`/`id` sort/direction combination, opaque cursors and the
  status filter. Soft deletion removes all GSI attributes. GSI list consistency
  remains explicitly eventual.
- Plan schema 2 now carries a closed, optional table contract. Generic
  cost-only DynamoDB still rejects SAM rendering. The explicit profile renders
  one encrypted on-demand table, three GSIs, PITR, retention, runtime table
  injection and separate table/index IAM statements. The role contains only
  `DescribeTable`, `GetItem`, `Query`, `TransactWriteItems` and `UpdateItem`;
  it contains no unused `PutItem`, `dynamodb:*`, wildcard resource, relational
  connection, SSM parameter or KMS permission.
- Cost output remains incomplete without regional rates and calls out two-item
  transactions, three-index write/storage amplification, PITR and retained
  storage. The public support matrix, ADR, generated diagnostics/features/
  packages/plugins/schemas, facade feature and 33-package inventory are
  synchronized.

Runtime and cleanup proof:

- `scripts/dev/rustack-dynamodb-smoke.sh` passed against the repository-pinned
  Rustack revision `ab8bc61a3e45058c7d42de8443f9d215cc110b18`. It proved
  readiness, replay/conflict/concurrency, replay after deletion, every sort and
  cursor, status filtering, conditional update/stale write, soft deletion and
  hidden direct reads using the standard AWS SDK client.
- The final observed test ran in compose project
  `minco-rustack-dynamodb-26037`. Its unique table was deleted and polled absent;
  exact-label checks found no remaining container, network or volume. Test-only
  credentials and loopback endpoints were used. No real AWS API was contacted.

Passed gates:

- 18 Plan unit tests and 48 Plan integration tests; 3 provider tests; 5 Orders
  adapter unit tests; the ignored-by-default Rustack integration through the
  cleanup harness; 3 service tests; the exact Lambda `lambda,dynamodb` feature
  check; scoped all-feature strict Clippy.
- `cargo minco plugin validate --json` returned `[]`; public `plugin test
  --all` reports all 18 components passed; repository-truth regression tests
  passed 40/40; generated reference checks passed.
- `cargo minco explain` traces all five Orders operation IDs through their
  contract, handler and application modules and lists `dynamodb` alongside the
  three existing store implementations with the Rustack conformance test.
- The explicit deploy plan contains only local services `dynamodb` and `sts`.
  Its cost report is intentionally incomplete and lists every missing regional
  rate and residual-cost assumption.
- `./scripts/quality.sh` passed static/publish/deep validation, documentation
  snippets/links and desktop/mobile browser journeys, all-feature workspace
  checks, strict Clippy, all workspace tests, generated PostgreSQL and SQLite
  applications, rustdoc/docs, Cargo policy, RustSec, npm audit, gitleaks and the
  exact current source-manifest check. Deep review retained only existing
  heuristic warnings outside the new provider/Orders adapter.

Issues caught and permanently corrected:

- The security pass removed unused `PutItem` IAM and split table mutation from
  index query resources.
- Publication validation caught missing archive license files; both standard
  repository licenses and the new crate archive test are now required.
- Public plugin conformance caught a nonexistent resource subfeature and a
  stale 17-component assertion; the descriptor now models the whole opt-in
  crate and the regression proves the DynamoDB component among all 18.
- The first broad gate exposed that current-source validation would overwrite
  the frozen M12 candidate identity. Repository truth now holds the immutable
  candidate hash separately while `verification/source-manifest.json` seals
  this post-candidate source.

No AWS resource, external database, customer data, crate registry, release tag,
Git remote, deployment target or product repository was mutated. Nothing was
pushed, merged, uploaded, published, promoted or deployed.
