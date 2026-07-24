---
id: M8-T01
title: Prepare the publishable Minco crate family
milestone: M8
status: complete
priority: critical
area: release/crates-io
depends_on: [M0-T01]
operations: []
owned_paths:
  - Cargo.toml
  - crates/minco/**
  - crates/minco-cli/**
  - crates/*/Cargo.toml
  - plugins/*/Cargo.toml
  - extensions/*/Cargo.toml
  - scripts/validate_publish.py
  - scripts/release/**
  - docs/development/publishing.md
checks:
  - python3 scripts/validate_publish.py
  - python3 scripts/test/scaffold_templates.py
---

## Goal

Create a lock-step crates.io package family with a `minco` facade, `cargo-minco`
subcommand, explicit publication order, complete package metadata, checked-in
licenses, deterministic application templates, and guarded release automation.

## Evidence

Static publication validation identifies 14 publishable packages and five
private reference-application packages. The release driver defaults to a dry
run and requires an explicit `--execute` flag for upload.
