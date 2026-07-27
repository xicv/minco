---
id: M10-T04
title: Add rollback compatibility and optional canary aliases
milestone: M10
status: planned
priority: high
area: deployment/recovery
depends_on: [M10-T03]
operations: []
owned_paths:
  - crates/minco-deploy-aws/**
  - crates/minco-release/**
  - crates/minco-cli/**
  - infra/aws/**
  - docs/adrs/**
  - docs/deployment/**
  - tasks/M10/M10-T04-rollback-canary.md
checks:
  - cargo test -p minco-deploy-aws -p minco-release -p cargo-minco --all-features --locked
  - cargo minco rollback --dry-run
  - cargo minco promote --dry-run --canary
---

## Goal

Assess contract, configuration, resource, migration, and data compatibility
before routing traffic to an older artifact, and support optional alarm-guarded
Lambda alias canaries.

## Acceptance

- rollback reports compatible, incompatible, or operator-decision-required
  with exact reasons;
- arbitrary SQL reversal is never promised;
- canary configuration is opt-in and identifies additional cost/resources;
- pre/post-traffic verification and alarms can stop and reverse a shift;
- API and worker alias/event-source behavior is explicit.

## Non-goals

- hidden traffic shifting;
- automatic data repair;
- provisioned concurrency in the default minimal-idle profile.
