# ADR-0003: Use Axum and Tower directly

- Status: Accepted
- Date: 2026-07-23

## Context

Minco needs a small, explicit and durable foundation that can be reasoned about by humans, AI agents, local tooling and deployment planners without duplicating sources of truth.

## Decision

Minco provides conventions, operation binding, middleware configuration and tests on top of Axum/Tower. It does not create a second router or proprietary middleware abstraction.

## Consequences

Axum is already a thin, modular HTTP layer and Tower provides interoperable middleware. A wrapper would duplicate ecosystem capability and obscure performance behavior.

Changes that invalidate this decision require a superseding ADR and migration/compatibility plan.
