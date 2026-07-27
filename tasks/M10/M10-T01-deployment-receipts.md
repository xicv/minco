---
id: M10-T01
title: Bind package and deployment receipts to exact releases
milestone: M10
status: planned
priority: critical
area: deployment/release
depends_on: [M9-T07]
operations: []
owned_paths:
  - crates/minco-release/**
  - crates/minco-plan/**
  - crates/minco-cli/**
  - scripts/aws/**
  - docs/adrs/**
  - docs/deployment/**
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
