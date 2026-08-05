---
id: M13-T04
title: Add bounded operation and task context projections for agents
milestone: M13
status: complete
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
  - tasks/M13/M13-T05-generated-application-agent-setup.md
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p cargo-minco --test agent_cli --locked
  - cargo minco agent context --json
  - uv run --locked python scripts/docs/generate_reference.py --check
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

Completed on 2026-08-05 in the isolated `minco-task-m13-t04` JJ workspace,
stacked on M13-T03:

- four initial context tests failed because the subcommand did not exist,
  preserving the new boundary's test-first red state;
- project context now returns ProjectView identity, framework/application mode,
  summary, diagnostics, source digest, input usage and explicit limits;
- operation and task selectors return only exact ProjectView nodes, related
  edges and task readiness, while unknown IDs return stable `found: false`
  diagnostics without guessed content;
- every projection reads its Minco 1.0 documentation identifiers from the
  matching packaged skill bundle, and every identifier resolves to checked-in
  versioned documentation;
- identifiers are bounded and path-free, selectors are mutually exclusive and
  compact JSON is capped at 64 KiB; and
- 12 agent CLI integration tests pass, including four context tests for
  deterministic project output, exact operation/task selection, unknown IDs,
  documentation resolution and input bounds; and
- generated CLI reference, source-manifest checking and static validation are
  current with zero static errors or warnings. Rust formatting was checked only
  on the three modified/created Rust files; no workspace-wide formatter or
  Clippy pass is claimed.

Context reports and performs zero writes, child commands, network requests and
arbitrary file reads. No check, skill instruction, remote document, runtime,
database, provider, release, registry, publication, deployment or production
action was invoked.
