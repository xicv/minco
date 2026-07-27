---
id: M9-T07
title: Add OpenAPI compatibility diff and application upgrade reports
milestone: M9
status: planned
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
  - tasks/M9/M9-T07-compatibility-report.md
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
