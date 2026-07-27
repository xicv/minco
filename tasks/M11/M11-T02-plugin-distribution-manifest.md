---
id: M11-T02
title: Define the plugin distribution manifest
milestone: M11
status: planned
priority: critical
area: plugins/ecosystem
depends_on: [M9-T04, M10-T01]
operations: []
owned_paths:
  - crates/minco-core/**
  - crates/minco-cli/**
  - plugins/**
  - extensions/**
  - docs/adrs/**
  - docs/reference/**
  - tasks/M11/M11-T02-plugin-distribution-manifest.md
checks:
  - cargo test -p minco-core -p cargo-minco --all-features --locked
  - cargo minco plugin validate
  - uv run --locked python scripts/test/repository_truth.py
---

## Goal

Define static Cargo/package metadata for core compatibility, capabilities,
configuration, runtimes/databases, operations/headers, migrations/seeds,
resources/IAM/wake/cost, health, sensitivity, failure policy, documentation,
and conformance evidence.

## Acceptance

- static distribution metadata and runtime descriptors have one documented
  authority per field;
- deterministic drift validation covers overlapping fields;
- crates.io consumers can inspect compatibility without executing plugin code;
- secret values and provider credentials cannot appear;
- normal explicit Cargo dependency and constructor registration remain required.

## Non-goals

- a hosted plugin registry;
- runtime package discovery or dynamic loading;
- duplicating application-owned provider policy into plugin metadata.
