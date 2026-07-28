---
id: M10-T01
title: Bind package and deployment receipts to exact releases
milestone: M10
status: complete
priority: critical
area: deployment/release
depends_on: [M9-T07]
operations: []
owned_paths:
  - crates/minco-release/**
  - crates/minco-plan/**
  - crates/minco-cli/**
  - minco.toml
  - roadmap/roadmap.yaml
  - verification/source-manifest.json
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/static-validation.json
  - scripts/aws/**
  - scripts/test/scaffold_templates.py
  - docs/adrs/**
  - docs/deployment/**
  - docs/reference/cli.md
  - tasks/M10/M10-T01-deployment-receipts.md
checks:
  - cargo test -p minco-release -p minco-plan -p cargo-minco --all-features --locked
  - cargo clippy -p minco-release -p minco-plan -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco package
  - cargo minco release verify target/minco/release.json
---

## Goal

Extend the immutable release boundary with package and deployment receipts that
bind exact source, artifact, contract, Plan IR, template, configuration digest,
migration/seed plans, environment identity, verification, and toolchain.

## Acceptance

- receipts are deterministic, schema-versioned, redacted, and independently
  verifiable;
- package refuses dirty, conflicted, or mismatched source;
- a deployment controller cannot replan or rebuild a verified release;
- failed attempts remain recorded and cannot become success receipts;
- signature or attestation extension points do not require a hosted service.

## Non-goals

- crate publication;
- deploying during package creation;
- storing credentials or secret values in a receipt.

## Completion evidence

Completed on 2026-07-28 in the isolated `minco-task-m10-t01` JJ workspace
against merged-main parent `71089f8a`.

- Red-green-refactor coverage proves deterministic release sealing, manifest
  tamper detection, dirty-Git rejection, package-output containment, terminal
  deployment states, cross-process terminal-transition serialization, durable
  failure preservation, symlink-escape rejection, and independent verification
  of the exact release, database plan, and success evidence.
- `cargo test -p minco-release -p minco-plan -p cargo-minco --all-features
  --locked` passed, including generated-app and multi-runtime integration tests.
- `cargo clippy -p minco-release -p minco-plan -p cargo-minco --all-targets
  --all-features --locked -- -D warnings` passed.
- The real repository `cargo minco package` flow built the ARM64 Orders Lambda,
  emitted a schema-3 digest-sealed manifest, and `cargo minco release verify
  target/minco/release.json` independently verified every bound file.
- The final qualification sequence is `./scripts/quality.sh`, `cargo minco
  package`, `cargo minco release verify target/minco/release.json`, and `jj log
  -r 'conflicts()'`.

Failed attempts retained:

- the first full-quality attempt stopped at `STATIC-TRUTH-ROADMAP-002` until M10
  was activated;
- the next attempt found the independent scaffold verifier lacked the new
  `{{PACKAGE_COMMAND}}` fixture;
- the following attempt passed compilation, tests, generated apps, docs,
  dependency and leak audits, then correctly stopped on the stale deterministic
  source/adoption evidence chain;
- none of those attempts is counted as a pass.

No crate was published, no deployment environment or cloud resource was
contacted, and no credential or secret value was written to a receipt.
