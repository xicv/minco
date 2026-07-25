---
id: M2-T02
title: Validate plugin enable disable and scaffolding commands
milestone: M2
status: complete
priority: high
area: cli
depends_on: [M2-T01]
operations: []
owned_paths:
  - crates/minco-cli/src/plugin_cmd.rs
  - plugins/catalog.toml
checks:
  - cargo minco plugin validate
  - cargo test -p cargo-minco
---

## Goal

Ensure the plugin catalog and `minco plugin list|enable|disable|new|validate` workflows are deterministic and safe to review.

## Evidence

On 2026-07-24, catalog validation returned no findings and all eight
`cargo-minco` unit tests passed, including deterministic JSON, plugin-owned
operation tracing, argument normalization and layered application scaffolding.
