# ADR-0001: OpenAPI 3.1 is canonical

- Status: Accepted
- Date: 2026-07-23

## Context

Minco needs a small, explicit and durable foundation that can be reasoned about by humans, AI agents, local tooling and deployment planners without duplicating sources of truth.

## Decision

All externally visible HTTP behavior is defined in a committed OpenAPI 3.1 document before handler implementation. Generated DTOs and operation metadata are checked in with a canonical digest. Axum bindings, tests, deployment routes and explanations derive from the same operation inventory.

## Consequences

This removes route/spec drift and gives humans and AI agents one inspectable contract. The initial profile is deliberately constrained and rejects unsupported ambiguity rather than silently weakening schemas.

Changes that invalidate this decision require a superseding ADR and migration/compatibility plan.
