---
id: M6-T04
title: Implement production AWS adapters for official plugin ports
milestone: M6
status: planned
priority: high
area: extensions/aws
depends_on: [M5-T01, M6-T02]
operations: []
owned_paths:
  - extensions/minco-aws-*/**
  - infra/aws/**
checks:
  - cargo test --workspace --all-features
  - cargo minco deploy plan
  - scripts/test/e2e.sh
---

## Goal

Add S3 object storage/signing, SQS event publication, transaction-integrated
PostgreSQL outbox recovery, SES and signed-webhook notifications, Cognito user
administration, and static-site S3/CloudFront rendering without changing Minco
core.

## Acceptance

- Every adapter implements an existing provider-neutral official plugin port.
- IAM and cost intents are derived from selected capabilities.
- No adapter introduces a hidden schedule or fixed-capacity default.
- Local emulator and bounded real-AWS conformance evidence is recorded.

## Current evidence

No production provider adapter in this task was implemented. Plan generation
and the existing native ARM64 Lambda ZIP build pass. SAM linting, live
CloudFormation template validation and IAM Access Analyzer policy validation
also pass without creating resources. No real AWS deployment or provider
conformance run was performed. Status therefore remains `planned`.
