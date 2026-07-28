---
id: M9-T07
title: Add OpenAPI compatibility diff and application upgrade reports
milestone: M9
status: complete
priority: high
area: compatibility
depends_on: [M9-T06]
operations: []
owned_paths:
  - crates/minco-contract/**
  - crates/minco-cli/**
  - crates/minco/**
  - docs/adoption/**
  - docs/reference/**
  - scripts/test/publish_validation.py
  - tasks/M9/M9-T07-compatibility-report.md
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p minco-contract -p cargo-minco --all-features --locked
  - cargo clippy -p minco-contract -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco contract diff --against main
  - cargo minco upgrade report
---

## Goal

Classify detectable breaking and non-breaking OpenAPI changes and report
application-facing Rust, CLI, feature, configuration, plugin, and serialized
upgrade boundaries with stable diagnostics.

## Acceptance

- local/reference resolution follows Minco's constrained OpenAPI profile;
- reports identify evidence and uncertainty instead of claiming semantic
  business compatibility;
- JSON output is deterministic;
- release notes and migration guides can consume the report;
- fixtures cover versioned schema and feature-boundary changes.

## Non-goals

- proving all behavioral compatibility;
- automatically rewriting application business logic;
- treating a green contract diff as deployment or data-migration proof.

## Review corrections

- The provisional library diff classified only operation addition/removal.
  Stable binding, authentication, idempotency, schema, property, type, enum and
  constraint changes now have explicit bounded rules. Unresolved references
  and unclassified operation/schema structure fail to `uncertain` rather than
  disappearing or claiming compatibility.
- `contract diff` reads a validated baseline with `jj file show` or `git show`
  and never checks out the requested revision. Revision input is bounded,
  option-like and shell-shaped input is rejected, and project contract paths
  are validated before VCS access.
- The upgrade report is parsed before strict `minco.toml` loading so an
  unsupported manifest schema remains reportable. Configuration values and
  defaults are omitted, linked plugin versions are bounded metadata, and
  malformed auxiliary plugin/deployment TOML produces stable redacted
  diagnostics instead of an early unstructured error.
- Adding the publishable compatibility integration test correctly changed the
  package-inclusion regression fixture. The task boundary now names that
  fixture and the deterministic verification/source-identity evidence refreshed
  by the required repository gate.

## Evidence

Completed on 2026-07-28 in the isolated `minco-task-m9-t07` JJ workspace
against merged-main parent `db40e6096847920bc6708a0047a138501eabecc3`.

- RED began with `E0432` because no compatibility report API existed. Later
  RED cases proved silent handling of binding/auth/idempotency changes,
  schema/reference changes, whole type/enum constraint changes, unclassified
  schema keywords, unsafe revision shapes, malformed auxiliary upgrade
  boundaries and the missing public CLI commands.
- `cargo test -p minco-contract -p cargo-minco --all-features --locked`
  passes. Evidence includes 39 CLI binary tests, three compatibility CLI
  integration tests, 22 contract-compatibility tests, 11 existing
  contract-policy tests and the generated-app/dev/seed/generator suites.
- `cargo clippy -p minco-contract -p cargo-minco --all-targets --all-features
  --locked -- -D warnings` passes.
- `cargo minco task verify M9-T07 --json` passes all four declared checks.
  `cargo minco contract diff --against main` reports identical baseline and
  candidate SHA-256 values with `non_breaking`, empty change arrays and
  explicit semantic/deployment limitations. `cargo minco upgrade report`
  emits schema 1 with `review_required`, deterministic boundary evidence and
  no diagnostics for the reference application.
- `./scripts/quality.sh` passes the complete repository gate: static/truth and
  publish validation; deep review and security fixtures; SQLite, scaffold,
  dependency and SQLx-isolation checks; every facade/workspace compiler,
  Clippy and test matrix; PostgreSQL/SQLite generated applications; Rustdoc and
  docs; Cargo deny/audit; npm audit; redacted Gitleaks; and exact source-manifest
  verification.
- Documentation defines the bounded classifications, VCS behavior,
  deterministic schema, redaction rules and an evidence-led application
  upgrade workflow consumable by release notes and migration guides.

No crate, release, deployment, database, cloud resource or application feature
was published, mutated or enabled by this task.
