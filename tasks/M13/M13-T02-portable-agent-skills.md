---
id: M13-T02
title: Add the portable Minco workflow skill bundle and scenario contracts
milestone: M13
status: ready
priority: high
area: ai/skills
depends_on: [M13-T01]
operations: []
owned_paths:
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

Pending implementation and local qualification.
