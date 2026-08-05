---
id: M13-T01
title: Define the version-matched agent-native development contract
milestone: M13
status: complete
priority: critical
area: ai/architecture
depends_on: [M12-T08]
operations: []
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0033-agent-native-development.md
  - roadmap/roadmap.yaml
  - roadmap/roadmap.mmd
  - roadmap/tasks.mmd
  - tasks/M13/**
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - uv run --locked python scripts/validate_static.py
  - cargo minco task show M13-T01 --json
  - cargo minco task ready --json
---

## Goal

Turn the Encore, SRS, Codex, Claude, Agent Skills and MCP research into one
accepted Minco contract, active milestone and bounded task sequence before
writing skills or CLI behavior.

## Acceptance

- ADR-0033 makes canonical assets, client projections, versioning, modes,
  plan/sync ownership and no-clobber behavior explicit;
- existing ProjectView, MCP and evidence authorities remain unchanged;
- application and framework-contributor workflows are distinct;
- every implementation task owns its source, tests, documentation and coupled
  generated evidence;
- mutation, release, provider and global-client non-goals are explicit; and
- completing this task exposes exactly M13-T02 as the next ready task.

## Non-goals

- creating a skill or client projection;
- implementing `cargo minco agent`;
- changing the MCP tool catalog;
- installing client plugins or global skills;
- committing, publishing, releasing, deploying or contacting a provider.

## Evidence

Completed on 2026-08-05 in the isolated `minco-task-m13-t01` JJ workspace
from exact `main@origin` `2aa1278e7755`:

- ADR-0033 records the research-derived client, asset, CLI, compatibility and
  security decisions without changing ProjectView schema 1 or its six-tool
  read-only MCP catalog;
- the M13 roadmap and six task contracts separate portable skills, guarded
  projection writes, bounded context, generated-application setup and
  cross-client qualification;
- deterministic roadmap/task diagrams were regenerated from the authoritative
  YAML and task front matter, including previously omitted completed M12-T07
  and M12-T08 nodes;
- static validation reported 14 milestones, 80 tasks, zero errors and zero
  warnings; and
- after this task moved to `complete`, `cargo minco task ready --json` returned
  exactly M13-T02.

The repository-wide `cargo minco check --with-cargo` closeout command was
started, then stopped when inspection confirmed that its configured Rust gate
includes workspace-wide `cargo fmt --check` and Clippy beyond the user's
file-scoped lint boundary. It made no source formatting changes and is not
claimed as completion evidence. This documentation/task-only change instead
uses its declared static, schema, generated-graph and changed-file checks.

No skill, CLI behavior, client file, MCP tool, database, provider, hosted
workflow, release, registry entry or deployment was created or changed.
