# ADR-0012: Model database deployment as explicit cost profiles

- Status: Accepted
- Date: 2026-07-23

## Context

Minco needs a small, explicit and durable foundation that can be reasoned about by humans, AI agents, local tooling and deployment planners without duplicating sources of truth.

## Decision

Minco models Neon, self-hosted PostgreSQL, RDS PostgreSQL, Aurora Serverless v2, DynamoDB on-demand and persistent SQLite separately. PostgreSQL/SQLite have official adapters; DynamoDB requires access-pattern-specific ports/adapters.

## Consequences

Database choice changes correctness, operational ownership, networking, wake behavior and cost. One generic database abstraction or guessed price would hide these trade-offs.

Changes that invalidate this decision require a superseding ADR and migration/compatibility plan.
