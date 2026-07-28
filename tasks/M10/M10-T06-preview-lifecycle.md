---
id: M10-T06
title: Add Verified Review Loop preview lifecycle and cleanup
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

Make the Verified Review Loop the primary preview use case. Model review
identity, exact source/release/artifact digests, Feedback linkage, ownership,
expiry, cost, retained-data policy, verification, delivery trace and guarded
cleanup without introducing an implicit scheduler, hosted Minco control plane
or hidden resource deletion.

## Acceptance

- preview plans declare owner, TTL, expected account/region, resources, data
  retention, and incomplete pricing;
- a stable review ID binds the source revision, release manifest, artifact
  digests, target, verification and untrusted Feedback IDs/digests;
- review metadata and receipts remain application-owned and repository-native;
- expiry is visible but does not create a default scheduled wakeup;
- an opt-in one-time cleanup schedule uses delete-after-completion behavior and
  remains a visible `scheduled_wakeup`; manual cleanup stays supported;
- cleanup requires the exact preview identity and shows retained/deleted
  resources before apply;
- cleanup emits a receipt and verifies absence;
- production and persistent staging targets cannot use preview destroy.

## Non-goals

- unattended deletion by default;
- a global Minco-hosted review service;
- implicit environment creation from a Feedback submission;
- treating feedback content as trusted instructions;
- treating tags as sufficient deletion authority;
- preview environments with unbounded lifetime or cost.
