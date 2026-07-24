# ADR-0013: Keep local quality and update workflows authoritative

- Status: Accepted
- Date: 2026-07-23

## Context

Minco needs a small, explicit and durable foundation that can be reasoned about by humans, AI agents, local tooling and deployment planners without duplicating sources of truth.

## Decision

Unit, feature, e2e and complete quality commands run locally. GitHub Actions is optional and manual by default. `minco update` checks and applies reviewed toolchain/dependency changes only from a clean workspace.

## Consequences

The project must remain usable without hosted CI and must not perform unsigned self-replacement. Evidence, not automation venue, determines release readiness.

Changes that invalidate this decision require a superseding ADR and migration/compatibility plan.
