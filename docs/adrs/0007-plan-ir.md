# ADR-0007: Use a provider-neutral deployment Plan IR

- Status: Accepted
- Date: 2026-07-23

## Context

Minco needs a small, explicit and durable foundation that can be reasoned about by humans, AI agents, local tooling and deployment planners without duplicating sources of truth.

## Decision

Compiled operation/resource intent and environment configuration produce a versioned deployment plan. Renderers, local drivers, cost/performance policy and inspection consume the plan.

## Consequences

A stable plan separates application semantics from SAM/Pulumi syntax and makes cost-affecting resources inspectable before deployment.

Changes that invalidate this decision require a superseding ADR and migration/compatibility plan.
