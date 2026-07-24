---
id: M6-T01
title: Design and implement an explicit DynamoDB orders adapter
milestone: M6
status: planned
priority: medium
area: persistence/dynamodb
depends_on: [M4-T01, M5-T01]
operations: [placeOrder, getOrder]
owned_paths:
  - extensions/minco-aws-dynamodb/**
  - examples/orders/adapters/src/dynamodb.rs
checks:
  - cargo test -p minco-aws-dynamodb
  - cargo minco cost --config examples/orders/config/minco.dynamodb.toml
---

## Goal

Implement DynamoDB as a distinct access model with conditional writes and an explicit key design; do not pretend it is a SQLx PostgreSQL substitute.
