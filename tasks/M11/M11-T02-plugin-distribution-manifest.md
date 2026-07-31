---
id: M11-T02
title: Define the plugin distribution manifest
milestone: M11
status: complete
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
  - docs/DECISIONS.md
  - docs/reference/**
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
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

## Implementation

- Every official plugin, adapter and runtime crate ships a strict schema-1
  `minco-plugin.json` selected by `[package.metadata.minco]` and explicitly
  included in its Cargo package archive.
- The record covers selection and compatibility coordinates, capabilities,
  configuration, operations and headers, database assets, resource/IAM/wake/
  idle-cost intent, health, sensitivity, retention, failure, documentation and
  inert conformance evidence.
- `cargo minco plugin list` reads local records without constructing plugin
  code. `cargo minco plugin validate` checks archive inclusion, catalog/schema/
  safety rules, current-core compatibility and overlapping linked-descriptor
  fields deterministically.
- `cargo minco plugin new` and `cargo minco make plugin` generate the package
  pointer, archive inclusion and record. Registry-backed application entries
  remain non-fetching; downloaded archives can be inspected independently.
- ADR 0027 records field authority and preserves explicit Cargo dependency plus
  typed constructor registration as the only composition path.

## Local evidence

- `cargo test -p minco-core -p cargo-minco --all-features --locked` passed the
  core, CLI and integration suites, including official-catalog and generated-app
  regressions.
- `cargo minco plugin validate --json` returned an empty finding list for all
  official catalog entries.
- `cargo package -p minco-plugin-health --allow-dirty --no-verify --locked`
  produced an archive containing the normalized metadata pointer and
  `minco-plugin.json`.
- `uv run --locked python scripts/test/repository_truth.py` passed all 22
  repository-truth checks.
