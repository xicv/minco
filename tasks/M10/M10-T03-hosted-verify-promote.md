---
id: M10-T03
title: Add hosted verification and exact-artifact promotion
milestone: M10
status: complete
priority: critical
area: deployment/verification
depends_on: [M10-T02]
operations: []
owned_paths:
  - Cargo.lock
  - minco.toml
  - crates/minco-plan/**
  - crates/minco-deploy-aws/**
  - crates/minco-test/**
  - crates/minco-cli/**
  - infra/aws/generated/**
  - scripts/generate_bootstrap_artifacts.py
  - scripts/aws/**
  - docs/deployment/**
  - verification/deep-review.json
  - verification/static-validation.json
  - verification/source-manifest.json
  - verification/adoption-measurements.json
  - tasks/M10/M10-T03-hosted-verify-promote.md
checks:
  - cargo test -p minco-plan -p minco-deploy-aws -p minco-test -p cargo-minco --all-features --locked
  - cargo minco deploy verify --manifest target/minco/release.json
  - cargo minco promote --dry-run --manifest target/minco/release.json
---

## Goal

Run hosted contract, readiness, authentication, smoke, and artifact-identity
checks after apply, then promote the already verified release without
rebuilding or replanning.

## Acceptance

- verification records endpoint, request IDs, executed artifact/version, and
  redacted results;
- failed readiness or smoke prevents promotion;
- promotion changes an explicit alias/routing boundary only;
- the deployed artifact digest matches the verified manifest;
- local, hosted, promotion, and production evidence remain distinct.

## Non-goals

- synthesizing a successful hosted result;
- automatic production promotion;
- treating a provider health check as business acceptance.

## Start evidence

Started on 2026-07-28 in the isolated `minco-task-m10-t03` JJ workspace from
exact merged-main parent `379e2522`.

- The first red test targets the fail-closed promotion boundary: a hosted
  verification report with failed readiness must not become promotable
  evidence.
- No AWS or hosted endpoint was contacted, no receipt was transitioned, and no
  alias or production routing boundary was changed.

## Review corrections

- The original ownership excluded the Plan/SAM renderer even though the
  generated reference API routes directly to the unqualified Lambda function.
  Hosted verification followed by promotion cannot be truthful without a
  candidate-versus-live routing boundary, so the bounded authoritative and
  generated paths are now explicit.
- M10-T03 owns the baseline exact promotion boundary. M10-T04 remains
  responsible for rollback compatibility, optional weighted canaries, alarms,
  and explicit worker/event-source behavior.
- Repository quality regenerates the deterministic deep-review and static
  validation inventories before checking the source manifest. Those three
  exact generated verification files, plus the adoption report's qualified
  revision pointer, are therefore included in task ownership; broader
  verification output remains out of scope.

## Completion evidence

Completed on 2026-07-28 in the isolated `minco-task-m10-t03` JJ workspace
against merged-main parent `379e2522`.

- Red-green-refactor coverage proves five required hosted checks, exact
  request/status evidence, redacted HTTPS endpoints, immutable strict reports,
  release/artifact/version binding, terminal failed deployments, and
  digest-approved promotion receipts that accept only one live-stage property
  modification.
- The generated SAM template now publishes a fixed `candidate` alias, routes
  the isolated candidate stage to that alias, and keeps the live `$default`
  stage on its prior `LiveFunctionVersion` until explicit promotion. Ordinary
  UPDATE change sets fail closed if that guarded live value cannot be
  preserved.
- `cargo test -p minco-plan -p minco-deploy-aws -p minco-test -p cargo-minco
  --all-features --locked` passed. Focused hosted-verification and promotion
  suites passed 11 and 6 tests respectively.
- `./scripts/quality.sh` passed formatting, compilation, Clippy with warnings
  denied, workspace tests, generated PostgreSQL and SQLite application checks,
  Rustdoc, dependency/license policy, vulnerability scans, package-lock audit,
  secret scanning, and the deterministic source/adoption evidence chain.
- `scripts/aws/validate.sh`, `bash -n`, and `shellcheck` passed for the changed
  AWS scripts. Re-running `scripts/generate_bootstrap_artifacts.py` under the
  locked Python environment reproduced plan SHA-256
  `8164229ae2912ae6384e6b2a5009d5597b93d72eefeb484b74860ce07bbf6c05`
  and template SHA-256
  `e25a3c0d61ad8bddc795e92067def9728d102c8090e3355a511c414ed090e372`
  byte-for-byte.
- `cargo minco deploy verify --dry-run --manifest
  target/minco/release.json` and `cargo minco promote --dry-run --manifest
  target/minco/release.json` reported their missing-evidence blockers while
  confirming no AWS/HTTP contact, receipt transition, rebuild, or replan.
- `cargo package -p minco-deploy-aws --locked --allow-dirty --no-verify`
  packaged 13 files successfully. The equivalent `cargo-minco` packaging
  attempt remains a coordinated-release limitation and failed visibly with
  `no matching package named 'minco-config' found`; this task does not publish
  or widen into a workspace release.
- No AWS API or hosted endpoint was contacted. The non-dry hosted
  `cargo minco deploy verify` check remains real post-apply runtime evidence,
  and no production runtime proof, alias change, deployment, or promotion is
  claimed by this local qualification.
