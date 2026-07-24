# ADR-0005: Use statically linked plugins and typed injection

- Status: Accepted
- Date: 2026-07-23

## Context

Minco needs a small, explicit and durable foundation that can be reasoned about by humans, AI agents, local tooling and deployment planners without duplicating sources of truth.

## Decision

Plugins are Rust crates implementing the Minco plugin contract. They declare descriptors and install typed services into the composition context. Catalog selection enables/disables defaults; code is explicitly linked and constructed.

## Consequences

Rust has no stable dynamic-library ABI suitable for a durable ecosystem. Static composition gives compile-time types, reviewable dependencies and deterministic deployment/resource analysis.

Changes that invalidate this decision require a superseding ADR and migration/compatibility plan.
