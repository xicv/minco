---
id: M5-T01
title: Build deploy and verify the real AWS development stack
milestone: M5
status: planned
priority: critical
area: deployment/aws
depends_on: [M1-T02, M3-T01, M4-T01]
operations: [getLive, getReady, placeOrder, getOrder]
owned_paths:
  - infra/aws/**
  - scripts/aws/**
checks:
  - scripts/aws/build-lambda.sh
  - scripts/aws/validate.sh
  - scripts/aws/deploy.sh
---

## Goal

Build one native ARM64 ZIP, deploy it behind API Gateway HTTP API, use an existing pooled PostgreSQL URL from SSM, and retain hosted verification evidence.

## Safety

This task is intentionally not marked complete until it runs in a reviewed AWS account with no secrets in committed output.
