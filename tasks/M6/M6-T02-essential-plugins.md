---
id: M6-T02
title: Stabilize provider-neutral essential plugin contracts
milestone: M6
status: active
priority: medium
area: plugins
depends_on: [M5-T01]
operations: []
owned_paths:
  - plugins/**
  - extensions/**
checks:
  - cargo minco plugin validate
  - cargo test --workspace --all-targets --all-features --locked
---

## Goal

Stabilize independently selectable sessions, identity, object-storage, events/outbox, notifications, audit, and static-site contracts without adding provider dependencies, fixed capacity, or hidden schedules to the core. Concrete AWS implementations are tracked separately in M6-T04.

## Current evidence

The provider-neutral plugin implementations, catalog validation and full
all-feature workspace tests pass on Rust 1.97.1. This task remains active
because prerequisite `M5-T01` is still planned; the concrete AWS adapters remain
separate in `M6-T04`.
