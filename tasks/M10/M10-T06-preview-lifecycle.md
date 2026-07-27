---
id: M10-T06
title: Add preview environment TTL cost and cleanup
milestone: M10
status: planned
priority: high
area: deployment/preview
depends_on: [M10-T02, M10-T05]
operations: []
owned_paths:
  - crates/minco-deploy-aws/**
  - crates/minco-plan/**
  - crates/minco-cli/**
  - infra/aws/**
  - docs/deployment/**
  - tasks/M10/M10-T06-preview-lifecycle.md
checks:
  - cargo test -p minco-deploy-aws -p minco-plan -p cargo-minco --all-features --locked
  - cargo minco deploy plan --environment preview
  - cargo minco destroy --environment preview --dry-run
---

## Goal

Model preview ownership, expiry, cost, retained-data policy, verification, and
guarded cleanup without introducing an implicit scheduler or hidden resource
deletion.

## Acceptance

- preview plans declare owner, TTL, expected account/region, resources, data
  retention, and incomplete pricing;
- expiry is visible but does not create a default scheduled wakeup;
- cleanup requires the exact preview identity and shows retained/deleted
  resources before apply;
- cleanup emits a receipt and verifies absence;
- production and persistent staging targets cannot use preview destroy.

## Non-goals

- unattended deletion by default;
- treating tags as sufficient deletion authority;
- preview environments with unbounded lifetime or cost.
