# ADR-0010: Make AI support transparent and machine-readable

- Status: Accepted
- Date: 2026-07-23

## Context

Minco needs a small, explicit and durable foundation that can be reasoned about by humans, AI agents, local tooling and deployment planners without duplicating sources of truth.

## Decision

Stable paths, operation IDs, checked-in generation, JSON CLI output, architecture rules, roadmap/task records and `AGENTS.md` form the AI interface. Hidden runtime discovery and generated business logic are rejected.

## Consequences

Agents are most reliable when they can inspect canonical sources, deterministic graphs and executable acceptance commands.

Changes that invalidate this decision require a superseding ADR and migration/compatibility plan.
