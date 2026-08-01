---
id: M11-T08
title: Release Minco 0.6.0 plugin ecosystem and documentation
milestone: M11
status: active
priority: critical
area: release/crates-io
depends_on: [M11-T01, M11-T02, M11-T03, M11-T07]
operations: []
owned_paths:
  - .github/workflows/docs-pages.yml
  - .github/workflows/minco-manual.yml
  - .github/workflows/publish-crates.yml
  - Cargo.toml
  - Cargo.lock
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - PUBLISHING.md
  - README.md
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - crates/**
  - extensions/**
  - plugins/**
  - examples/**
  - infra/aws/generated/**
  - docs/**
  - docs-site/**
  - roadmap/**
  - scripts/docs/**
  - scripts/release/**
  - scripts/test/**
  - scripts/validate_publish.py
  - scripts/validate_static.py
  - tasks/M11/M11-T08-minco-0.6.0-release.md
  - verification/**
checks:
  - uv run --locked python scripts/test/repository_truth.py
  - uv run --locked python scripts/test/publish_validation.py
  - cargo minco plugin validate
  - cargo minco plugin test --all
  - cargo test --manifest-path examples/plugins/third-party-minimal/Cargo.toml --all-features --locked
  - scripts/docs/build.sh
  - scripts/docs/check-links.sh
  - scripts/docs/check-snippets.sh
  - scripts/docs/test-browser.sh
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

Qualify and publish one exact `0.6.0` source and 28-package family for the
archive-visible plugin distribution contract, public conformance kit and
versioned detailed documentation added after `0.5.0`.

## Release boundary

- Previous published baseline: `0.5.0`, 28 packages.
- Candidate release: `0.6.0`, the same 28-package lock-step family.
- Included: M11-T01, M11-T02, M11-T03 and M11-T07.
- Deferred: M10-T04 through M10-T06, M11-T04 through M11-T06, M12 and the 1.0 freeze.
- MSRV: Rust `1.97.1`.

## Acceptance

- the workspace and every publishable internal dependency use lock-step
  `0.6.0`, while historical release records and stable `0.5.0` docs remain
  immutable;
- a `0.5.0` to `0.6.0` guide explains plugin distribution metadata,
  conformance, CLI additions and compatibility boundaries;
- the detailed `next` documentation is promoted into immutable `0.6.0` docs,
  augmented with stable installation/tutorial paths, and becomes the website
  default only after registry publication succeeds;
- all official plugin distribution records and the external-style fixture
  target core `0.6.0`, validate deterministically and ship in package archives;
- all 28 archives pass coordinated dry run, configured unpacked tests,
  external consumers and unpacked `cargo-minco` installation;
- local quality, browser, generated applications, Rustack, Orders E2E,
  deterministic Plan/SAM and native ARM64 Lambda artifact gates pass;
- the exact merged candidate passes the explicit hosted `release` profile;
- tag, GitHub release, registry, docs.rs, website and external-consumer proof
  are independently recorded without implying live AWS deployment.

## Non-goals

- creating, modifying, promoting or deleting AWS resources;
- implementing planned plugin mutation commands, generated reference or the
  broad examples matrix;
- claiming the M10, M11 or M12 programs are complete.

## Evidence

Local source, runtime and package qualification passed on 2026-08-01 in the
isolated `task-m11-t08` JJ workspace:

- `./scripts/quality.sh` passed repository/static/publish/deep-review,
  contract, architecture, complete workspace test/Clippy/rustdoc, generated
  PostgreSQL/SQLite application, 40-test Feedback browser, documentation
  build/link/browser, dependency-policy, audit, secret and source-manifest
  gates. Cargo audit reported no vulnerabilities and one explicitly allowed
  upstream `event-listener 5.4.1` warning;
- all 16 official distribution records validated and the public conformance
  runner produced 16 deterministic reports; the standalone third-party-style
  plugin passed its locked tests and documentation build;
- the Orders lifecycle E2E passed, and four ignored adapter tests passed
  against a disposable local PostgreSQL 18 database;
- Rustack passed S3, SQS, SSM and STS plus Minco S3/SQS/SSM adapter
  conformance without AWS contact;
- deterministic Plan and SAM validation passed. Native ARM64 artifacts built
  as a 5,102,303-byte Orders Lambda ZIP with SHA-256
  `7864a2533e14dbb21abec1d7757e1ace047dc1c2b9c9b4c7e3081ff08288a5f7`
  and a 574,199-byte worker ZIP with SHA-256
  `80d7f8bb3c82a4ead305696437dcad88f5c1473b82373e8a606e5d61749b11f8`;
- crates.io was reachable and all 28 exact `0.6.0` versions were absent;
- the clean-workspace release driver simulated all 28 uploads, tested every
  configured unpacked archive, compiled no-default/default/all-feature and
  new-package external consumers, and installed the unpacked CLI, which
  reported `minco 0.6.0`. `package-list.sh` exposed all 28 archive manifests.
- candidate hosted run `30687931439` reproduced a clean-runner-only evidence
  mismatch: static validation counted a generated VitePress `dist/release.json`
  locally but not in a clean checkout. The validator now excludes the exact
  VitePress cache/dist prefixes shared by the source-manifest policy, with a
  regression proving source `docs-site/release.json` remains included.

Exact PR-head hosted release qualification, merge, merged-main release
qualification, tag, OIDC publication, independent registry/consumer/docs.rs
proof and stable website promotion remain pending. No AWS resource was
created, modified, promoted or deleted.
