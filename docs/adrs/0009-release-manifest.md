# ADR-0009: Build once and promote immutable releases

- Status: Accepted
- Date: 2026-07-23

## Context

Minco needs a small, explicit and durable foundation that can be reasoned about by humans, AI agents, local tooling and deployment planners without duplicating sources of truth.

## Decision

A release manifest binds source revision, binary artifact, contract, migration set, lockfile, deployment plan and toolchain hashes. Migrations are explicit and deployment promotes the exact artifact.

## Consequences

Rebuilding during promotion destroys provenance. Explicit migration/deploy/verify stages reduce environment and data mistakes.

Changes that invalidate this decision require a superseding ADR and migration/compatibility plan.
