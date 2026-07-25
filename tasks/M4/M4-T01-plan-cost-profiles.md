---
id: M4-T01
title: Validate deployment plan and database cost profiles
milestone: M4
status: complete
priority: critical
area: deployment-planning
depends_on: [M2-T01, M3-T01]
operations: [getLive, getReady, placeOrder, getOrder]
owned_paths:
  - crates/minco-plan/**
  - examples/orders/config/**
  - pricing/**
checks:
  - cargo test -p minco-plan --locked
  - cargo clippy -p minco-plan --all-targets --locked -- -D warnings
  - cargo minco deploy plan --output <temporary-plan-path> --json
  - cargo minco cost --json
---

## Goal

Model Neon, self-hosted PostgreSQL, RDS PostgreSQL, Aurora Serverless v2, DynamoDB on-demand and persistent SQLite with explicit completeness and fixed-cost diagnostics.

## Evidence

On 2026-07-24, every committed database profile produced a deterministic plan
and cost report. Neon rates and allowances were reconciled with the dated
provider snapshot; regional AWS rates remain explicit inputs, so RDS, Aurora
and DynamoDB estimates correctly stay incomplete when those rates are absent.
Planner regressions reject negative or non-finite numeric inputs, invalid
multi-AZ multipliers, and Aurora auto-pause settings outside the documented
zero-ACU and 300-to-86400-second boundary. Minimal-idle fixed-compute policy
diagnostics remain explicit.
