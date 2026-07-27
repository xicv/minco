---
id: M6-T10
title: Add trigger-aware multi-runtime deployment planning
milestone: M6
status: complete
priority: high
area: deployment/plan
depends_on: [M6-T09]
operations: []
owned_paths:
  - CHANGELOG.md
  - crates/minco-plan/**
  - crates/minco-cli/**
  - extensions/minco-aws-worker/**
  - infra/aws/**
  - docs/adrs/**
  - docs/DECISIONS.md
  - docs/deployment/**
  - docs/reference/cli.md
  - roadmap/tasks.mmd
  - tasks/M6/M6-T10-multi-runtime-deployment-plan.md
  - verification/deep-review.json
  - verification/adoption-measurements.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p minco-plan -p cargo-minco --all-features --locked
  - cargo clippy -p minco-plan -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco deploy plan
  - cargo minco deploy render-sam
  - sam validate --lint --template-file infra/aws/generated/template.yaml
---

## Goal

Extend Minco's schema-versioned deployment Plan IR beyond the initial single
HTTP API Lambda so it can explicitly describe one API function, worker
functions, queues, dead-letter policy, SQS mappings, partial-batch behavior,
and reviewed recovery schedules without making scheduled work a default.

## Design boundary

- keep exactly one HTTP API function in the first multi-runtime schema;
- model worker functions, SQS queues, DLQs, mappings, and schedules explicitly;
- validate FIFO, visibility timeout, redrive, concurrency, aggregate database
  connection, wake-source, cost, and stable-reference invariants;
- derive local services, exact IAM intent, and deterministic SAM from selected
  resources only;
- reject enabled schedules under the default minimal-idle policy;
- keep product event schemas, retry business policy, and processors in the
  application;
- provide deterministic schema migration or stable rejection diagnostics;
- treat the public serialized redesign as a likely `0.4.0` boundary.

## Acceptance

- the original single-API plan remains supported;
- generic fixtures cover API-only, standard/FIFO workers, DLQs, invalid
  references, partial-batch behavior, schedules, and connection budgets;
- local topology and SAM output remain deterministic;
- no queue, worker, poller, schedule, or fixed capacity appears implicitly;
- no AWS mutation is required for task completion.

## Non-goals

- Step Functions, Kinesis, Kafka, ECS/Fargate, arbitrary workflow graphs,
  multi-cloud abstractions, or multi-region deployment;
- product-specific routing, settlement, garment, scan, fulfilment, invitation,
  permission, billing, or rollback policy;
- replacing an application's live IaC before advisory parity and rollback
  evidence pass.

## Completion evidence

Completed on 2026-07-27 after prerequisite `M6-T09` merged to `main`.

- Schema 2 models exactly one HTTP API function plus explicit worker functions,
  queues, DLQs, SQS mappings, partial-batch behavior, and reviewed schedules.
  Legacy schema 1 API-only plans remain accepted and have a deterministic
  migration; ambiguous legacy worker, queue, trigger, and schedule shapes fail
  with stable diagnostics.
- Validation covers unresolved and duplicate references, CloudFormation logical
  ID collisions, worker/API role separation, FIFO/DLQ compatibility, redrive
  cycles and bounds, visibility timeout, per-mapping and aggregate concurrency,
  database connection pressure, minimal-idle schedule policy, and derived IAM.
- Local topology contains only selected services and never runs schedules.
  Database-free functions omit database parameters, environment, VPC policy,
  and SSM/KMS intent. DynamoDB remains access-pattern-specific and its generic
  SAM render path fails closed.
- SAM rendering is deterministic for API functions, workers, queues, redrive,
  partial-batch SQS mappings, `ScheduleV2`, exact queue IAM, and bounded log
  groups. The default API-only Plan/SAM output remains compatible.
- `cargo test -p minco-plan -p cargo-minco --all-features --locked` passed 65
  unit/integration tests, including the 17 required fixture and migration
  categories. Focused strict Clippy passed.
- `cargo minco deploy plan`, `cargo minco deploy render-sam`, and
  `sam validate --lint --template-file infra/aws/generated/template.yaml`
  passed for the default API-only profile. The equivalent schema 2 API/worker
  plan and rendered SAM also passed lint.
- `./scripts/quality.sh` passed static/publish/deep validation, the facade build
  matrix, strict workspace Clippy and tests, generated application tests,
  rustdoc/docs, dependency policy, RustSec, npm audit, secret scanning, and the
  exact source-manifest check.
- `scripts/test/generated_apps.sh` passed clean generated PostgreSQL and SQLite
  application builds/tests. `scripts/test/e2e.sh` passed Orders SQLite E2E.
  `scripts/dev/rustack-smoke.sh` passed local S3, SQS, SSM, and STS conformance.
- `scripts/release/package-list.sh` includes the schema 2 tests and fixtures in
  the `minco-plan` package. `scripts/release/publish.sh --skip-quality` passed
  the complete 24-crate Cargo publication dry-run; every upload was aborted by
  dry-run mode.
- Final arm64 artifacts:
  - Orders API ZIP: 5,030,945 compressed / 11,047,008 uncompressed bytes,
    SHA-256
    `7fece3ba3064c73dc6c4da6c4bd82d3f86b92f814e3cf76362e08026461fe7f7`.
  - SQS worker ZIP: 573,415 compressed / 1,203,520 uncompressed bytes, SHA-256
    `0dfea51f5e6150987fe047e046a80f7c4ec8aed17fa4909390984ede00df28eb`.
- No AWS API mutation, database migration, crate upload, version bump, release
  tag, or product-repository change was performed.

## Issues caught and permanent corrections

- The initial concurrency validation bounded each SQS mapping independently but
  did not bound their sum against worker reserved concurrency. Validation now
  rejects aggregate oversubscription with regression coverage.
- Stable raw IDs could initially collapse to the same generated CloudFormation
  logical ID. Queue, function, trigger, log-group, and OpenAPI route event
  collisions now fail before rendering.
- Local-native plans initially inherited Lambda SSM/KMS IAM, and database-free
  Lambda functions initially inherited unused database/VPC configuration.
  Derivation and SAM rendering now condition those surfaces on the selected
  runtime and actual per-function database use.
- A schema 1 plan could initially relabel its sole function as a worker before
  migration. Migration now requires the legacy HTTP API role and fails closed.
- The first full quality attempt reported that `minco-plan`'s new integration
  tests were absent from the published package. Its package include set now
  contains `tests/**`, and the package inventory proves the fixtures are
  shipped.
- Subsequent full-quality attempts correctly rejected stale source-manifest and
  adoption-measurement revisions. Both generated records now bind the same
  exact source tree; failed attempts are not counted as passes.
- Cargo Lambda emitted the existing non-fatal linker diagnostic
  `ignoring deprecated linker optimization setting '1'` for both final builds.
  The artifacts were produced and verified, but the diagnostic is recorded
  rather than represented as a warning-free build.
- The deep-review heuristic reports `expect` calls in SAM production code.
  Final review confirmed those calls only unwrap `std::fmt::Write` operations
  into an in-memory `String`, which are infallible; plan validation remains the
  fail-closed boundary for external input.
