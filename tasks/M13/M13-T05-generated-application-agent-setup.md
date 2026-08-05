---
id: M13-T05
title: Integrate agent projections with freshly generated Minco applications
milestone: M13
status: planned
priority: high
area: ai/generator
depends_on: [M13-T04]
operations: []
owned_paths:
  - crates/minco-cli/src/agent_cmd.rs
  - crates/minco-cli/src/new_cmd.rs
  - crates/minco-cli/templates/**
  - crates/minco-cli/tests/agent_cli.rs
  - crates/minco-cli/tests/generator_cli.rs
  - docs/how-to/**
  - docs/reference/**
  - tasks/M13/M13-T05-generated-application-agent-setup.md
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p cargo-minco --test agent_cli --test generator_cli --locked
  - uv run --locked python scripts/test/scaffold_templates.py
  - uv run --locked python scripts/validate_static.py
---

## Goal

Make a fresh PostgreSQL or SQLite Minco application ready for explicit
repository-scoped Codex and Claude setup without a global install or framework-
contributor assumptions.

## Acceptance

- generated projects expose application-mode instructions and no framework-only
  task/JJ/release policy;
- setup remains explicit and plan-first;
- absent instruction files may be created, while existing user-owned files
  produce conflicts or manual actions;
- both database profiles produce the same skill asset version and client parity;
  and
- generated-project tests prove discovery paths and byte-preserving conflict
  behavior.

## Non-goals

- modifying user-level Codex or Claude configuration;
- automatically enabling MCP without a reviewed project action;
- choosing a cloud environment; or
- deploying a generated application.

## Evidence

Pending implementation and local qualification.
