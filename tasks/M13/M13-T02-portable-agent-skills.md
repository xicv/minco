---
id: M13-T02
title: Add the portable Minco workflow skill bundle and scenario contracts
milestone: M13
status: complete
priority: high
area: ai/skills
depends_on: [M13-T01]
operations: []
owned_paths:
  - crates/minco-cli/Cargo.toml
  - crates/minco-cli/assets/agent/**
  - crates/minco-cli/tests/agent_skills.rs
  - docs/how-to/**
  - docs/reference/**
  - tasks/M13/M13-T02-portable-agent-skills.md
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - uv run --locked python scripts/validate_static.py
  - cargo test -p cargo-minco --test agent_skills --locked
  - cargo package -p cargo-minco --allow-dirty --list
---

## Goal

Create concise Agent Skills and negative/positive scenario contracts for Minco
web applications, operations, plugins, lifecycle, diagnosis, review, framework
tasks and explicitly requested release preparation.

## Acceptance

- each skill uses portable `name` and `description` front matter and stays
  progressively disclosed;
- references point to version-matched Minco concepts instead of copying large
  documentation payloads;
- application skills never assume Minco's framework task/JJ structure;
- release and other side-effect workflows retain explicit user authority; and
- evaluations assert required concepts, ordering and forbidden actions.

## Non-goals

- writing client directories;
- implementing plan or synchronization behavior;
- invoking a hosted model;
- changing MCP; or
- publishing a plugin package.

## Evidence

Completed on 2026-08-05 in the isolated `minco-task-m13-t02` JJ workspace,
stacked on M13-T01:

- the first package-level tracer test failed because
  `assets/agent/bundle.json` did not exist, proving the new contract red before
  the skill assets were added;
- eight focused skills now cover web applications, operations, plugins,
  lifecycle, diagnosis, review, framework tasks and explicitly requested
  releases with portable two-field front matter and one-level references;
- the bundle binds the skills and documentation identifiers to Minco 1.0.0,
  while 16 scenario contracts give every skill one trigger and one boundary
  case with required concepts and forbidden actions;
- the package integration tests pass both bundle/skill validation and scenario
  coverage; all eight skill-creator validations pass;
- `cargo package -p cargo-minco --allow-dirty --list` includes the complete
  `assets/agent` tree; and
- static validation reports zero errors and warnings. File-scoped Rust format
  checking is limited to the new integration test; no workspace formatter or
  Clippy run is claimed.

No client projection was written, no hosted model was invoked, and no MCP,
database, provider, release, registry, publication or deployment action was
performed.
