---
id: M10-T03
title: Add hosted verification and exact-artifact promotion
milestone: M10
status: planned
priority: critical
area: deployment/verification
depends_on: [M10-T02]
operations: []
owned_paths:
  - crates/minco-deploy-aws/**
  - crates/minco-test/**
  - crates/minco-cli/**
  - scripts/aws/**
  - docs/deployment/**
  - tasks/M10/M10-T03-hosted-verify-promote.md
checks:
  - cargo test -p minco-deploy-aws -p minco-test -p cargo-minco --all-features --locked
  - cargo minco deploy verify --manifest target/minco/release.json
  - cargo minco promote --dry-run --manifest target/minco/release.json
---

## Goal

Run hosted contract, readiness, authentication, smoke, and artifact-identity
checks after apply, then promote the already verified release without
rebuilding or replanning.

## Acceptance

- verification records endpoint, request IDs, executed artifact/version, and
  redacted results;
- failed readiness or smoke prevents promotion;
- promotion changes an explicit alias/routing boundary only;
- the deployed artifact digest matches the verified manifest;
- local, hosted, promotion, and production evidence remain distinct.

## Non-goals

- synthesizing a successful hosted result;
- automatic production promotion;
- treating a provider health check as business acceptance.
