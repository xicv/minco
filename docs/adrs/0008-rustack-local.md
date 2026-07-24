# ADR-0008: Use Rustack through standard AWS endpoints

- Status: Accepted
- Date: 2026-07-23

## Context

Minco needs a small, explicit and durable foundation that can be reasoned about by humans, AI agents, local tooling and deployment planners without duplicating sources of truth.

## Decision

Local AWS adapters use the normal AWS SDK and endpoint overrides. Rustack is the preferred fast emulator for declared supported services; real AWS smoke tests remain the fidelity authority.

## Consequences

This avoids emulator-specific business code and keeps local infrastructure replaceable while exploiting Rustack’s small startup/footprint.

Changes that invalidate this decision require a superseding ADR and migration/compatibility plan.
