---
id: M6-T02
title: Add identity storage queue and email plugins
milestone: M6
status: planned
priority: medium
area: plugins
depends_on: [M5-T01]
operations: []
owned_paths:
  - plugins/**
  - extensions/**
checks:
  - cargo minco plugin validate
  - cargo test --workspace --all-features
---

## Goal

Add independently selectable OIDC/Cognito, S3, SQS/outbox and SES implementations without adding fixed capacity or hidden schedules to the core.
