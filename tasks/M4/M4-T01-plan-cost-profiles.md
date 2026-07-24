---
id: M4-T01
title: Validate deployment plan and database cost profiles
milestone: M4
status: active
priority: critical
area: deployment-planning
depends_on: [M2-T01, M3-T01]
operations: [getLive, getReady, placeOrder, getOrder]
owned_paths:
  - crates/minco-plan/**
  - examples/orders/config/**
  - pricing/**
checks:
  - cargo test -p minco-plan
  - cargo minco deploy plan
  - cargo minco cost
---

## Goal

Model Neon, self-hosted PostgreSQL, RDS PostgreSQL, Aurora Serverless v2, DynamoDB on-demand and persistent SQLite with explicit completeness and fixed-cost diagnostics.
