---
id: M9-T02
title: Add a typed environment and secret-reference graph
milestone: M9
status: planned
priority: critical
area: configuration
depends_on: [M6-T10, M9-T01]
operations: []
owned_paths:
  - crates/minco-config/**
  - crates/minco-core/**
  - crates/minco-cli/**
  - crates/minco/**
  - examples/orders/config/**
  - docs/adrs/**
  - docs/reference/**
  - tasks/M9/M9-T02-typed-configuration.md
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
