# ADR-0002: Use a modular monolith with inward dependencies

- Status: Accepted
- Date: 2026-07-23

## Context

Minco needs a small, explicit and durable foundation that can be reasoned about by humans, AI agents, local tooling and deployment planners without duplicating sources of truth.

## Decision

Minco applications begin as one deployable modular monolith. Dependency direction is `delivery -> application -> domain`; adapters implement ports owned by the application layer. Feature names remain aligned across layers.

## Consequences

This minimizes deployments, IAM surfaces and cold-start fragmentation while preserving boundaries that permit later extraction after measured need.

Changes that invalidate this decision require a superseding ADR and migration/compatibility plan.
