---
id: M13-T04
title: Add bounded operation and task context projections for agents
milestone: M13
status: ready
priority: high
area: ai/context
depends_on: [M13-T03]
operations: []
owned_paths:
  - crates/minco-cli/src/agent_cmd.rs
  - crates/minco-cli/src/main.rs
  - crates/minco-cli/tests/agent_cli.rs
  - docs/how-to/**
  - docs/reference/**
  - tasks/M13/M13-T04-agent-context.md
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p cargo-minco --test agent_cli --locked
  - cargo minco agent context --json
  - uv run --locked python scripts/validate_static.py
---

## Goal

Project bounded project, operation or task context plus versioned documentation
identifiers from existing Minco authorities without adding a second graph or
changing the MCP tool catalog.

## Acceptance

- context schema and bounds are explicit;
- project, operation and task selection reuse ProjectView and task readiness;
- unknown exact IDs produce stable absent results or diagnostics rather than
  guessed context;
- documentation identifiers resolve only to packaged matching-version Minco
  references; and
- context never runs checks or reads secrets, arbitrary files or remote URLs.

## Non-goals

- document retrieval over the network;
- adding MCP tools;
- runtime process, trace, endpoint or database access; or
- mutation of project state.

## Evidence

Pending implementation and local qualification.
