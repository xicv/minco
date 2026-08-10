---
id: M14-T06
title: Guard minimal-cost deployment support and current product truth
milestone: M14
status: complete
priority: critical
area: architecture/assurance
depends_on: [M14-T01]
operations: []
owned_paths:
  - verification/deployment-assurance.toml
  - verification/repository-truth.toml
  - scripts/validate_deployment_assurance.py
  - scripts/docs/generate_reference.py
  - scripts/test/deployment_assurance.py
  - scripts/test/current_product_truth.py
  - scripts/validate_static.py
  - scripts/ci/hosted-essential.sh
  - scripts/test/hosted_ci_policy.py
  - scripts/quality.sh
  - quality.toml
  - docs/research/minimal-cost-framework-review-2026-08.md
  - docs/reference/generated/diagnostics.md
  - docs/vision/minco-framework-definition.md
  - REVIEW_STATUS.md
  - tasks/M14/M14-T06-minimal-cost-assurance.md
  - verification/source-manifest.json
checks:
  - python -m py_compile scripts/validate_deployment_assurance.py scripts/test/deployment_assurance.py scripts/test/current_product_truth.py
  - uv run --locked python scripts/validate_deployment_assurance.py
  - uv run --locked python scripts/test/deployment_assurance.py
  - uv run --locked python scripts/test/current_product_truth.py
  - uv run --locked python scripts/validate_static.py
  - bash scripts/ci/hosted-essential.sh
  - uv run --locked python scripts/source_manifest.py --check
---

## Goal

Make Minco's narrow AWS product promise executable after the 1.1 release:
every runtime/ingress topology must state whether it is supported, which evidence
backs it, which wake sources and residual cost classes remain, and whether it is
eligible to be the default. Current release documentation must remain bound to
one exact published source rather than drifting back to historical version
claims.

## Acceptance

- one machine-readable ledger covers every current `RuntimePlan` and
  `IngressPlan` variant, and its top-level default selector must match the
  single profile marked as the default;
- generated diagnostic reference includes the stable `ASSURANCE-*` code
  family used by the validator and its mutation tests;
- supported AWS profiles require contract, code, cost, security, performance,
  recovery and provider evidence;
- the default profile rejects fixed-monthly and scheduled-wakeup cost classes and
  undeclared polling/schedule wake sources;
- `LambdaFunctionUrl` remains explicitly declared but unsupported until its SAM,
  authentication, cost, verification and promotion boundaries exist;
- an implementation marker appearing without an assurance-status update fails
  closed;
- repository truth records the exact published 1.1.0 commit, and the framework
  maturity heading plus review status are checked against it;
- mutation tests prove missing variants, evidence, dimensions, renderer support
  and current-release markers fail closed;
- the bounded hosted essential gate executes the assurance and current-truth
  tests before Rust workspace checking and source-manifest verification;
- the hosted-CI behavioral policy pins the exact bounded essential command list;
- the post-1.0 research review records AWS choices, Laravel lessons, harness
  engineering, quality controls and prioritised follow-on work; and
- no live AWS resource, crate publication, release tag or production deployment
  is created or modified.

## Non-goals

- implementing Lambda Function URL rendering or changing the default ingress;
- changing the public Plan IR schema or serialized diagnostic contract;
- adding Aurora, DSQL, AppConfig, coverage or mutation tooling to the default
  product in this task;
- broadening Minco into a provider-neutral or Laravel-compatible framework; or
- claiming that zero provisioned compute means a zero provider bill.

## Evidence

The targeted Python files compile, the assurance validator passes against the
current repository, eight deployment-assurance mutation tests and five
current-product-truth tests pass, static validation remains clean, the bounded
hosted essential profile passes on a clean runner, and the canonical source
manifest is regenerated and verified for the exact final tree.
