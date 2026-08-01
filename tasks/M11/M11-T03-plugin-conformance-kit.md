---
id: M11-T03
title: Publish one plugin conformance kit
milestone: M11
status: complete
priority: critical
area: plugins/testing
depends_on: [M11-T02]
operations: []
owned_paths:
  - Cargo.lock
  - crates/minco-cli/**
  - crates/minco-test/**
  - crates/minco-core/**
  - plugins/**
  - extensions/**
  - examples/plugins/**
  - scripts/test/generated_apps.sh
  - docs/how-to/**
  - docs/reference/**
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
  - tasks/M11/M11-T03-plugin-conformance-kit.md
checks:
  - cargo test -p minco-test -p minco-core --all-features --locked
  - cargo minco plugin test --all
  - cargo minco plugin validate
  - cargo test --manifest-path examples/plugins/third-party-minimal/Cargo.toml --all-features --locked
  - cargo package -p minco-test --allow-dirty --no-verify --locked
---

## Goal

Make official and third-party-style plugins use the same public tests for
descriptor validity, config defaults/unknown fields, graph/provenance, HTTP
ownership, migrations/seeds, health, resources/IAM/wake/cost, package contents,
docs examples, and provider leakage.

## Acceptance

- the kit is usable from outside the workspace against a published version;
- at least one intentionally minimal third-party-style fixture passes;
- negative fixtures emit stable diagnostics;
- provider/live integration requirements remain separately labelled;
- plugin success does not imply application or provider production readiness.

## Non-goals

- forcing identical backend semantics;
- executing remote calls during composition;
- certifying a plugin's business or privacy policy automatically.

## Implementation

- `minco-test` now publishes one strict, deterministic conformance report API
  for plugin, adapter and runtime packages. It checks archive-visible metadata,
  linked descriptor overlap, configuration, HTTP ownership, database assets,
  resource/IAM/wake/cost declarations, provider leakage and current-core
  compatibility without executing evidence labels or contacting providers.
- Concrete plugin tests additionally compose twice, compare metadata-only
  registration provenance, probe unknown configuration rejection and keep
  application, provider/live and production readiness as separate assurance
  states.
- `cargo minco plugin test --all` applies the same public package boundary to
  all 16 official catalog entries. New plugin scaffolds and generated plugins
  import the public kit by default.
- `examples/plugins/third-party-minimal` is a standalone locked workspace with
  versioned dependencies and repository path overrides, proving the published
  API shape from outside the root workspace.
- The reference and how-to documentation define stable diagnostic codes,
  archive requirements, lifecycle usage and the explicit provider/live
  boundary.

## Local evidence

- `cargo test -p minco-test -p minco-core --all-features --locked` passed 38
  core tests, 21 minco-test tests including 17 conformance regressions, and all
  doctests.
- `cargo minco plugin test --all --json` returned 16 passed offline contract
  reports; lifecycle remained `not_assessed` for descriptor-only catalog
  checks, application and production remained `not_assessed`, and
  provider/live remained `not_run`.
- `cargo minco plugin validate --json` returned an empty finding list.
- `cargo test --manifest-path examples/plugins/third-party-minimal/Cargo.toml
  --all-features --locked` passed the standalone concrete-plugin lifecycle
  test, including deterministic provenance.
- `cargo package -p minco-test --allow-dirty --no-verify --locked` produced a
  ten-file archive containing the public implementation and tests.
- `scripts/test/generated_apps.sh` compiled and tested fresh PostgreSQL and
  SQLite applications, then compiled generated plugin code and observed only
  the intentional operation specification failures.
- `./scripts/quality.sh` passed static, formatting, clippy, full workspace test,
  documentation, browser, package-policy, dependency, advisory and secret-scan
  gates. Bounded real-AWS, Rustack and configured PostgreSQL tests remained
  explicitly ignored/not run by the offline suite.
