# ADR-0004: Use SQLx with explicit PostgreSQL and SQLite adapters

- Status: Accepted
- Date: 2026-07-23

## Context

Minco needs a small, explicit and durable foundation that can be reasoned about by humans, AI agents, local tooling and deployment planners without duplicating sources of truth.

## Decision

Persistence is implemented with SQLx and visible SQL. PostgreSQL and SQLite use separate adapters/migrations with shared behavioral tests only where semantics are portable. No ORM or generic CRUD repository is part of core.

## Consequences

Use-case-shaped ports preserve business transaction boundaries and permit database-specific strengths without a least-common-denominator model.

Changes that invalidate this decision require a superseding ADR and migration/compatibility plan.
