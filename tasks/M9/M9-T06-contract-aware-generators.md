---
id: M9-T06
title: Add contract-aware generators and customizable stubs
milestone: M9
status: planned
priority: high
area: developer-experience/generation
depends_on: [M9-T05]
operations: []
owned_paths:
  - crates/minco-cli/src/**
  - crates/minco-cli/templates/**
  - scripts/test/scaffold_templates.py
  - scripts/test/generated_apps.sh
  - docs/development/**
  - docs/reference/**
  - tasks/M9/M9-T06-contract-aware-generators.md
checks:
  - cargo test -p cargo-minco --all-features --locked
  - cargo clippy -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - uv run --locked python scripts/test/scaffold_templates.py
  - scripts/test/generated_apps.sh
---

## Goal

Add module, operation, migration, seeder, worker, adapter, test, plugin, and
app-owned stub generators that plan deterministic Rust/TOML/YAML-aware edits
and preserve OpenAPI authority.

## Acceptance

- every command supports dry-run and JSON change plans;
- an operation requires an existing operation ID unless a separately reviewed
  contract-stub mode is selected;
- generated application and HTTP tests fail until business behavior exists;
- no command overwrites unreviewed files or generates a fake success result;
- generated PostgreSQL and SQLite applications compile and test.

## Non-goals

- source scanning as registration;
- generating product domain rules;
- making generated files opaque framework runtime state.
