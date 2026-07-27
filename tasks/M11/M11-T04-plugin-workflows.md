---
id: M11-T04
title: Add plugin add init explain test and remove workflows
milestone: M11
status: planned
priority: high
area: plugins/developer-experience
depends_on: [M11-T03]
operations: []
owned_paths:
  - crates/minco-cli/**
  - crates/minco-core/**
  - plugins/catalog.toml
  - docs/how-to/**
  - docs/reference/**
  - tasks/M11/M11-T04-plugin-workflows.md
checks:
  - cargo test -p cargo-minco -p minco-core --all-features --locked
  - cargo clippy -p cargo-minco -p minco-core --all-targets --all-features --locked -- -D warnings
  - cargo minco plugin doctor
  - cargo minco plugin add minco-plugin-health --dry-run --json
---

## Goal

Plan deterministic Cargo, catalog, config, and composition-root changes for
plugin add/init/explain/test/remove/doctor while keeping code registration
explicit.

## Acceptance

- every mutating workflow supports dry-run and JSON;
- compatible explicit Cargo versions are resolved before edits;
- Rust/TOML edits fail before overwrite or ambiguity;
- explain shows capabilities, dependencies, resources, cost, config, and
  conformance evidence;
- remove reports application operations/data/migrations that prevent safe
  removal.

## Non-goals

- downloading or executing plugins dynamically at runtime;
- automatic source scanning for constructors;
- treating catalog metadata as executable discovery.
