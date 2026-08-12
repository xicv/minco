---
id: M14-T22
title: Provide typed side-effect fakes for application tests
milestone: M14
status: complete
priority: high
area: testing/developer-experience
depends_on: [M14-T21]
operations: []
owned_paths:
  - CHANGELOG.md
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - docs/DECISIONS.md
  - docs/adrs/0042-typed-side-effect-fakes.md
  - docs/development/testing.md
  - docs/research/minimal-cost-framework-review-2026-08.md
  - extensions/minco-aws-worker/Cargo.toml
  - extensions/minco-aws-worker/README.md
  - extensions/minco-aws-worker/src/**
  - extensions/minco-aws-worker/tests/**
  - plugins/minco-plugin-events/README.md
  - plugins/minco-plugin-events/Cargo.toml
  - plugins/minco-plugin-events/src/**
  - plugins/minco-plugin-events/tests/**
  - plugins/minco-plugin-feedback/README.md
  - plugins/minco-plugin-feedback/Cargo.toml
  - plugins/minco-plugin-feedback/src/**
  - plugins/minco-plugin-feedback/tests/**
  - plugins/minco-plugin-notifications/README.md
  - plugins/minco-plugin-notifications/Cargo.toml
  - plugins/minco-plugin-notifications/src/**
  - plugins/minco-plugin-notifications/tests/**
  - plugins/minco-plugin-object-storage/README.md
  - plugins/minco-plugin-object-storage/Cargo.toml
  - plugins/minco-plugin-object-storage/src/**
  - plugins/minco-plugin-object-storage/tests/**
  - crates/minco-cli/assets/agent/**
  - scripts/docs/generate_reference.py
  - scripts/test/repository_truth.py
  - scripts/validate_static.py
  - tasks/M14/M14-T22-typed-side-effect-fakes.md
  - docs/reference/generated/diagnostics.md
  - verification/1.4-performance-baseline.json
  - verification/agent-scenario-results.json
  - verification/agent-workflows.json
  - verification/deep-review.json
  - verification/operational-evidence-validation.json
  - verification/provider-evidence.toml
  - verification/publish-validation.json
  - verification/release-identity.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p minco-aws-worker --all-features --locked
  - cargo test -p minco-plugin-events --all-features --locked
  - cargo test -p minco-plugin-object-storage --all-features --locked
  - cargo test -p minco-plugin-feedback --all-features --locked
  - cargo test -p minco-plugin-notifications --all-features --locked
  - scripts/quality.sh
---

# M14-T22 - Provide typed side-effect fakes for application tests

## Goal

Complete the next unfulfilled P2 controlled pilot by giving application tests
official, port-specific fakes for SQS message handling, domain-event publishing,
object storage, feedback persistence and rich mail submission. Make success,
failure and retry behavior observable through the real public ports without a
generic mocking framework, provider contact or runtime service locator.

## Acceptance

- every fake implements the exact public port owned by its package and records
  attempts in deterministic call order;
- tests can enqueue explicit one-shot failures and prove retry, fallback,
  partial-batch and fail-before-persistence behavior;
- default behavior succeeds without network, credentials, sleeps, background
  work, hidden schedules or provider resources;
- fake `Debug` output excludes message bodies, object bytes, feedback content,
  access-token material, recipients, mail bodies, attachments and metadata
  values;
- application tests exercise the fakes only through public interfaces and
  prove failure scripts are consumed exactly once;
- memory reference adapters retain their current success-path behavior and no
  production adapter or deployment topology changes;
- documentation and portable agent guidance route new behavior tests to the
  typed fakes and retain live-provider evidence as a separate lane; and
- focused package tests and the authoritative local quality gate pass.

## Non-goals

- a general mock framework, generic repository, dynamic service locator or
  runtime plugin discovery;
- storing customer fixtures, credentials, bearer tokens or secret values in
  committed evidence;
- changing Plan IR, provider renderers, IAM, cost topology, application domain
  contracts or stable CLI output;
- contacting AWS or another provider, creating resources, benchmarking,
  deploying, tagging, publishing, releasing or merging without the later
  guarded delivery gates.

## Recovery and workspace

The task did not exist when P2 was authorised. Its isolated JJ workspace was
bootstrapped directly from exact merged `main`
`958e6ebf40db1f63614cf9a3da0e0af65188eafe` at
`/private/tmp/minco-task-m14-t22`. The detached primary checkout and unrelated
`task-m12-t09` workspace remain untouched.

## Evidence

Complete. The first three P2 pilots—pinned SemVer checks, measured
selected-crate coverage and bounded mutation testing—already landed through
M14-T20. This task completes the next controlled pilot with five public-port
fakes and 95 focused tests. Each tracer test first failed because its public
fake was absent, then passed through the real port after the minimum
implementation.

The focused five-package test matrix, targeted five-package Clippy with
warnings denied and diff-only `rustfmt --check` pass. The complete
`./scripts/quality.sh` gate also passes, including workspace Clippy/tests,
generated PostgreSQL and SQLite applications, both browser suites, docs and
rustdoc, package-inclusion validation, dependency audits, gitleaks and the
final source-manifest check. The package validator initially failed closed
because all five explicit `package.include` lists omitted the new integration
tests; the manifests now publish those test sources and all 34 publishable
packages validate.

Operational validation remains truthful: exact-tree hosted Linux performance
is `NOT RUN` and no current exact-source live-provider evidence qualifies this
source. No provider was contacted and no resource, deployment, tag, crate,
release or production state was created.
