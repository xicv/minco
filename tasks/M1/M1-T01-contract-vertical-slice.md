---
id: M1-T01
title: Complete the orders contract-to-router vertical slice
milestone: M1
status: active
priority: critical
area: reference-application
depends_on: [M0-T01]
operations: [getLive, getReady, placeOrder, getOrder]
owned_paths:
  - examples/orders/openapi/**
  - examples/orders/domain/**
  - examples/orders/application/**
  - examples/orders/api/**
checks:
  - cargo minco contract check
  - cargo minco contract sync --check
  - cargo test -p orders-domain -p orders-application -p orders-api
---

## Goal

Prove a complete OpenAPI-first feature through generated DTOs, business rules, use cases, explicit ports, Axum delivery and in-process HTTP tests.
