# ADR-0011: Use Jujutsu as the default VCS interface

- Status: Accepted
- Date: 2026-07-23

## Context

Minco needs a small, explicit and durable foundation that can be reasoned about by humans, AI agents, local tooling and deployment planners without duplicating sources of truth.

## Decision

Minco uses colocated JJ/Git repositories for GitHub compatibility, creates one JJ workspace per task, records immutable commit IDs in releases and uses bookmarks only for collaboration.

## Consequences

JJ workspaces permit parallel task development and long-running tests; change identity, operation log and first-class conflicts improve safe rewrite/recovery workflows.

Changes that invalidate this decision require a superseding ADR and migration/compatibility plan.
