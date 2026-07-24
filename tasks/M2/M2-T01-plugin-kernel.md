---
id: M2-T01
title: Stabilize the static plugin and capability kernel
milestone: M2
status: complete
priority: critical
area: core
depends_on: [M0-T01]
operations: []
owned_paths:
  - crates/minco-core/**
  - plugins/**
checks:
  - cargo test -p minco-core --all-features
  - cargo test -p minco-plugin-health -p minco-plugin-observability -p minco-plugin-idempotency
---

## Goal

Support explicit registration, default enablement, explicit disablement, dependency resolution, typed service injection and deterministic graph validation without runtime discovery.

## Evidence

On 2026-07-24, all 24 `minco-core` plugin, graph, service and descriptor
tests passed with all features. The six scoped health, observability and
idempotency plugin tests also passed, covering deterministic finalization,
runtime selection, lease ownership and typed service injection.
