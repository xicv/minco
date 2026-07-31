---
id: M8-T09
title: Prepare the Minco 0.5.0 resource API and zero-idle release candidate
milestone: M8
status: ready
priority: critical
area: release/crates-io
depends_on: [M9-T09, M10-T07]
operations: [placeOrder, listOrders, getOrder, updateOrder, deleteOrder]
owned_paths:
  - .github/workflows/minco-manual.yml
  - .github/workflows/publish-crates.yml
  - Cargo.toml
  - Cargo.lock
  - CHANGELOG.md
  - README.md
  - CODEX_HANDOFF.md
  - PUBLISHING.md
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - crates/**
  - extensions/**
  - plugins/**
  - examples/orders/**
  - infra/aws/generated/**
  - minco.toml
  - docs/adoption/0.4.0-to-0.5.0.md
  - docs/adoption/incremental-adoption.md
  - docs/development/publishing.md
  - docs/development/testing.md
  - docs/development/using-minco-crate.md
  - docs/vision/minco-framework-definition.md
  - roadmap/**
  - scripts/release/**
  - scripts/test/publish_validation.py
  - scripts/test/repository_truth.py
  - scripts/validate_publish.py
  - scripts/validate_static.py
  - tasks/M8/M8-T09-minco-0.5.0-release.md
  - verification/**
checks:
  - uv run --locked python scripts/test/publish_validation.py
  - uv run --locked python scripts/test/repository_truth.py
  - uv run --locked python scripts/validate_publish.py --check-registry --require-registry
  - cargo minco contract check
  - cargo minco contract sync --check
  - cargo minco make resource order --dry-run --json
  - MINCO_ORDERS_TEST_POSTGRES_URL='<local-test-postgres-url>' cargo test -p orders-adapters --features postgres --test postgres -- --ignored
  - ./scripts/quality.sh
  - scripts/test/e2e.sh
  - scripts/dev/rustack-smoke.sh
  - scripts/aws/plan.sh
  - scripts/aws/validate.sh
  - scripts/aws/build-lambda.sh
  - scripts/aws/build-worker-lambda.sh
  - scripts/release/package-list.sh
  - scripts/release/publish.sh --skip-quality
---

## Goal

Prepare one exact, reviewable `0.5.0` source and package candidate for the
accepted zero-idle cost model, standardized resource API conventions and
local-authoritative CI boundary added after `0.4.0`.

## Release boundary

- Published baseline: `0.4.0`, 28 packages.
- Candidate: `0.5.0`, the same 28-package lock-step family.
- Included: M10-T07, M9-T08 and M9-T09.
- Deferred: M10-T04 through M10-T06, M11, M12 and the 1.0 freeze.
- MSRV: Rust `1.97.1`.

## Acceptance

- the workspace and every publishable internal dependency use lock-step
  `0.5.0`, while historical `0.4.0` release records remain immutable;
- the changelog and `0.4.0` to `0.5.0` guide explain the resource response,
  cursor, conditional-write, idempotency, Plan/cost and CI boundaries;
- repository truth distinguishes the published `0.4.0` baseline from the
  untagged, unpublished `0.5.0` candidate;
- the complete five-action Orders resource family is contract-valid,
  explainable, generator-selectable and exercised through memory, SQLite,
  PostgreSQL and real-service HTTP tests;
- all 28 archives pass the coordinated Cargo dry run, configured unpacked
  tests, external consumers and unpacked `cargo-minco` installation;
- local quality, browser, generated applications, Rustack, Orders E2E,
  deterministic Plan/SAM and both native ARM64 Lambda artifact gates pass;
- the exact pushed candidate head passes the explicit hosted `release`
  profile without contacting AWS or publishing crates;
- all evidence states whether it is local, hosted, package, registry,
  deployment or publication proof.

## Non-goals

- merging the candidate pull request without a separate exact-head decision;
- creating or pushing `v0.5.0`;
- publishing to crates.io or creating a GitHub release;
- creating, modifying, promoting or deleting AWS resources;
- claiming the planned M10, M11 or M12 program is complete.

## Evidence

Pending implementation and qualification.
