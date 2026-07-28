---
id: M9-T05
title: Add graph-driven cargo minco dev
milestone: M9
status: complete
priority: critical
area: developer-experience/runtime
depends_on: [M9-T02, M9-T03, M9-T04]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - README.md
  - minco.toml
  - crates/minco-dev/**
  - crates/minco-cli/**
  - scripts/dev/**
  - infra/local/**
  - docs/DECISIONS.md
  - docs/adrs/**
  - docs/development/**
  - verification/repository-truth.toml
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/publish-validation.json
  - verification/rust-dependency-hygiene.json
  - verification/source-manifest.json
  - verification/static-validation.json
  - tasks/M9/M9-T05-graph-driven-dev.md
checks:
  - cargo test -p minco-dev -p cargo-minco --all-features --locked
  - cargo clippy -p minco-dev -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco dev --dry-run --json
  - python3 scripts/dev/test_topology.py
---

## Goal

Derive a deterministic `DevPlan` that starts only declared local dependencies,
optional migrations/seed profiles, the API, selected workers, and an optional
application-defined frontend command with labelled logs, readiness, signal
handling, and coordinated shutdown.

## Acceptance

- dry-run and JSON expose every service/process before startup;
- defaults do not contact AWS, reset data, run schedules, or seed implicitly;
- selected PostgreSQL/SQLite/Rustack services and ports are deterministic;
- child-process failure and termination leave no detached Minco process;
- API and worker readiness is visible.

## Non-goals

- a frontend framework;
- automatic local schedules;
- replacing provider fidelity checks with emulator success.

## Review corrections

- The original task ownership omitted the root workspace manifests required to
  add `minco-dev`, the root development inventory, decision register, and
  generated qualification reports. Those bounded paths are now explicit.
- The primary quickstart in the root README is part of replacing the legacy
  multi-command start sequence, so that single documentation path is now
  explicitly owned.
- The repository truth inventory must include the new publishable `minco-dev`
  crate, so that bounded verification path is now explicitly owned.

## Evidence

Parent merged-main Git SHA: `1e8b781036b12a73cccf1c0d678b2deb3390d67b`.

- TDD regressions first reproduced lifecycle commands ignoring coordinated
  shutdown, API key/access-key values escaping plan serialization, and
  credential-bearing readiness queries being accepted. The corrected
  supervisor streams finite command output, terminates Unix process groups,
  waits and reaps before reporting `stopped`, rejects non-local or
  credential-bearing readiness targets, and applies one shared environment
  redaction classifier to serialized plans and runtime logs.
- `cargo test -p minco-dev -p cargo-minco --all-features --locked` and
  `cargo clippy -p minco-dev -p cargo-minco --all-targets --all-features
  --locked -- -D warnings` passed.
- `cargo minco dev --dry-run --json` emitted a complete local-only plan with
  PostgreSQL, Rustack SSM/STS, migration, API, and
  `external_aws_contact=false`; no database URL or AWS credential value was
  present.
- `python3 scripts/dev/test_topology.py` passed the default and SQLite/port
  override graph journeys. `scripts/test/generated_apps.sh` compiled and
  tested generated PostgreSQL and SQLite applications.
- `cargo package -p minco-dev --allow-dirty --locked --no-verify --list`
  included both licenses, README, library sources, supervisor sources, and
  integration tests.
- `./scripts/quality.sh` passed after refreshing the 27-package repository
  truth and deterministic source/adoption evidence. The gate includes static
  and publish validation, strict workspace Clippy, all-target/all-feature
  tests, generated consumers, rustdoc/docs, Cargo deny/audit, npm audit,
  gitleaks, and the terminal source-manifest check.
- Exact-source ARM64 adoption artifacts were rebuilt locally. Orders measured
  5,031,958 compressed bytes and the SQS worker 573,415 compressed bytes.
  Neither artifact, crate, container, cloud resource, database, or release was
  published or deployed.
