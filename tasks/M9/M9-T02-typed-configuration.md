---
id: M9-T02
title: Add a typed environment and secret-reference graph
milestone: M9
status: complete
priority: critical
area: configuration
depends_on: [M6-T10, M9-T01]
operations: []
owned_paths:
  - .gitignore
  - Cargo.lock
  - Cargo.toml
  - CHANGELOG.md
  - README.md
  - crates/minco-config/**
  - crates/minco-core/**
  - crates/minco-cli/**
  - crates/minco/**
  - examples/orders/config/**
  - minco.toml
  - docs/DECISIONS.md
  - docs/adrs/**
  - docs/development/publishing.md
  - docs/reference/**
  - roadmap/tasks.mmd
  - scripts/test/scaffold_templates.py
  - tasks/M9/M9-T02-typed-configuration.md
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/publish-validation.json
  - verification/repository-truth.toml
  - verification/rust-dependency-hygiene.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p minco-config -p minco-core -p cargo-minco --all-features --locked
  - cargo clippy -p minco-config -p minco-core -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco config check
  - cargo minco config diff --from dev --to production
---

## Goal

Create one provider-neutral typed configuration graph with documented
precedence, strict unknown-field rejection, plugin schema integration, opaque
secret references, redacted provenance, and a deterministic effective digest.

## Acceptance

- application code receives typed configuration through constructors;
- config check, explain, diff, and schema commands support JSON and stable
  diagnostics;
- secret values never enter graph, Plan IR, logs, or command output;
- local, test, staging, and production classes fail closed on invalid
  combinations;
- existing environment profiles have a documented migration path.

## Non-goals

- a hosted secret manager;
- arbitrary environment-variable reads throughout business code;
- resolving provider secrets during graph composition.

## Evidence

Completed on 2026-07-27 in the isolated `minco-task-m9-t02` JJ workspace
against merged-main parent `b3f7fc29d0c1b24820c587714e43fae831e3f234`.

- The focused all-feature test and strict-Clippy commands in `checks` passed.
  Regressions cover fixed caller-independent precedence, duplicate/unknown/type
  rejection, environment-class policy, enabled-plugin schema selection,
  constructor deserialization, path containment, deterministic environment
  normalization, secret-reference validation, and explain/diff redaction. A
  merge review follow-up also proves that custom typed-deserializer errors
  cannot echo secret-reference names and that prefixed non-UTF-8 environment
  variable names fail closed instead of disappearing from validation.
- `cargo minco --json config check` passed for local, test, development,
  staging, and production profiles. The exact task check produced a valid
  development graph and a 64-character SHA-256 digest.
- `cargo minco --json config diff --from dev --to production` emitted a
  redacted `database.url` entry with neither reference name nor before/after
  value, while retaining typed non-secret differences.
- Both generated PostgreSQL and SQLite project structures passed the scaffold
  validator; a generated SQLite project also passed `config check`.
- `cargo package -p minco-config --allow-dirty --locked` packaged and compiled
  the crate archive. No crate was uploaded and no release or tag was created.
- Exact-source native ARM64 Orders and SQS worker ZIPs were rebuilt locally for
  the adoption report. No AWS resource, external database, or registry was
  touched.
