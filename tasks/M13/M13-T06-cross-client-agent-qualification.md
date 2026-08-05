---
id: M13-T06
title: Qualify Minco agent workflows across Codex and Claude
milestone: M13
status: planned
priority: critical
area: ai/qualification
depends_on: [M13-T05]
operations: []
owned_paths:
  - crates/minco-cli/assets/agent/**
  - crates/minco-cli/tests/agent_cli.rs
  - crates/minco-cli/tests/agent_skills.rs
  - docs/how-to/**
  - docs/reference/**
  - scripts/test/agent_workflows.py
  - tasks/M13/M13-T06-cross-client-agent-qualification.md
  - verification/agent-workflows.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p cargo-minco --test agent_cli --test agent_skills --locked
  - uv run --locked python scripts/test/agent_workflows.py
  - uv run --locked python scripts/validate_static.py
---

## Goal

Prove that current Codex and Claude clients discover the same versioned Minco
skills and route representative application work correctly without unauthorized
side effects or evidence inflation.

## Acceptance

- deterministic checks prove canonical/projected asset parity and portable
  skill validation;
- scenario checks cover web application, operation, plugin, lifecycle,
  diagnosis, review, framework contribution and explicit release preparation;
- negative cases reject framework/application mode confusion, stale plans,
  user-owned file replacement, evidence upgrades and implicit side effects;
- any model-driven forward-test result is separately labelled with client,
  version, prompt, source revision and limitations; and
- exact local evidence is recorded without claiming hosted, deployment,
  runtime or review proof.

## Non-goals

- publishing a Codex or Claude marketplace plugin;
- changing the read-only MCP catalog;
- release, registry, hosted CI or provider mutation; or
- claiming production application behavior.

## Evidence

Pending implementation and local qualification.
