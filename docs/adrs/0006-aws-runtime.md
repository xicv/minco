# ADR-0006: Default to native ARM64 Lambda and HTTP API

- Status: Accepted
- Date: 2026-07-23

## Context

Minco needs a small, explicit and durable foundation that can be reasoned about by humans, AI agents, local tooling and deployment planners without duplicating sources of truth.

## Decision

The minimal AWS runtime is one native `provided.al2023` ARM64 Lambda ZIP behind API Gateway HTTP API. It uses no provisioned concurrency, NAT Gateway, fixed compute or schedule by default.

## Consequences

This minimizes idle compute and operational surfaces while retaining JWT authorizers, throttling and standard Lambda events. Containers/ECS remain future profiles based on measured needs.

Changes that invalidate this decision require a superseding ADR and migration/compatibility plan.
