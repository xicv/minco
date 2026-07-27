---
id: M12-T04
title: Audit and freeze public APIs and Cargo features
milestone: M12
status: planned
priority: critical
area: compatibility/release
depends_on: [M11-T06, M12-T03]
operations: []
owned_paths:
  - Cargo.toml
  - crates/**
  - plugins/**
  - extensions/**
  - docs/adrs/**
  - docs/adoption/**
  - docs/reference/**
  - CHANGELOG.md
  - tasks/M12/M12-T04-api-feature-freeze.md
checks:
  - cargo semver-checks --workspace --all-features
  - cargo test --workspace --all-targets --all-features --locked
  - cargo doc --workspace --all-features --no-deps --locked
  - scripts/docs/generate-reference.sh --check
---

## Goal

Review and explicitly freeze the 1.0 Rust API, Cargo feature matrix, CLI
surface, configuration schema, Plan IR, release/deployment receipts, plugin
distribution contract, diagnostics, MSRV, and compatibility policy.

## Acceptance

- public types and features have evidence-backed stability classifications;
- default and opt-in dependency surfaces are measured;
- deprecated pre-1.0 paths have migration guidance;
- unsupported promises are removed before the freeze;
- the compatibility policy states how Rust, CLI, serialized, and behavioral
  changes are versioned after 1.0.

## Non-goals

- preserving accidental undocumented internals;
- raising the MSRV without an explicit compatibility decision;
- freezing known unsafe or unverified behavior merely to meet a date.
