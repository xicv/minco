---
id: M14-T21
title: Guard golden-topology cost regressions
milestone: M14
status: complete
priority: high
area: cost/quality
depends_on: [M14-T20]
operations: []
owned_paths:
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - quality.toml
  - docs/DECISIONS.md
  - docs/adrs/0041-topology-cost-regression-baseline.md
  - docs/deployment/zero-idle-service-research.md
  - docs/development/quality-assurance.md
  - scripts/cost_regression.py
  - scripts/docs/generate_reference.py
  - scripts/quality.sh
  - scripts/ci/hosted-essential.sh
  - scripts/test/cost_regression.py
  - scripts/test/hosted_ci_policy.py
  - scripts/test/repository_truth.py
  - scripts/validate_static.py
  - tasks/M14/M14-T21-topology-cost-regression.md
  - verification/cost-regression-baseline.json
  - docs/reference/generated/diagnostics.md
  - verification/deep-review.json
  - verification/operational-evidence-validation.json
  - verification/1.4-performance-baseline.json
  - verification/provider-evidence.toml
  - verification/release-identity.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - uv run --locked python scripts/test/cost_regression.py
  - uv run --locked python scripts/cost_regression.py --check
  - cargo test -p minco-plan --all-features --locked
  - scripts/quality.sh
---

# M14-T21 - Guard golden-topology cost regressions

## Goal

Turn the reviewed Orders topology portfolio into a deterministic cost-model
regression gate. Compare the existing `cargo minco cost --json` projection for
local SQLite, Neon, Aurora Serverless v2, RDS, self-hosted PostgreSQL and
DynamoDB configurations without fetching live prices or claiming a complete
provider bill.

## Acceptance

- the baseline records stable profile IDs, exact configuration bytes and the
  canonical cost projection for every reviewed topology;
- selected fixed/request resources, wake sources, worker/queue pressure,
  missing regional rates, cost classes, pricing confidence and explicit dollar
  inputs fail closed on unreviewed drift;
- malformed, duplicate, missing, stale and non-canonical baseline records fail
  with stable `COST-REGRESSION-*` diagnostics;
- the validator invokes the existing CLI without a shell, never contacts a
  provider and never invents regional rates;
- local-native topology remains free of AWS runtime rates while AWS profiles
  preserve their explicit incomplete-rate classifications; and
- the normal quality gate verifies, but does not silently regenerate, the
  reviewed baseline.

## Non-goals

- embedding current AWS prices, forecasting a complete cloud bill or defining
  a production budget;
- adding a provider, resource, schedule, poller, hosted control plane or AWS
  contact;
- changing Plan IR, the public Rust API, CLI output or plugin compatibility; or
- releasing, publishing, tagging or deploying.

## Recovery and workspace

The task was created in a dedicated JJ workspace from exact merged `main`
`b9e2cf3b0621cfe67487142e609a6c26cf7391ee`. The task did not exist at
workspace creation, so the workspace was bootstrapped directly and records
that recovery path instead of fabricating task-start metadata.

## Evidence

Complete. Nine focused tests pass for canonical projection,
semantic/configuration drift, duplicate identity, malformed/non-finite JSON,
missing inputs, critical zero-idle invariants, symlinked inputs, bounded CLI
execution and secret-free failures. The seven-profile baseline reproduces
exactly, generated diagnostics include `COST-REGRESSION-001` through `009`, all
15 hosted-CI policy regressions pass, the bounded hosted-essential script
passes locally, all 53 Plan integration tests pass, and the complete local
quality matrix passes. Exact-head clean-Linux execution remains the external
pre-merge gate. No provider was contacted.
