---
id: M13-T06
title: Qualify Minco agent workflows across Codex and Claude
milestone: M13
status: complete
priority: critical
area: ai/qualification
depends_on: [M13-T05]
operations: []
owned_paths:
  - crates/minco-cli/src/agent_cmd.rs
  - crates/minco-cli/assets/agent/**
  - crates/minco-cli/tests/agent_cli.rs
  - crates/minco-cli/tests/agent_skills.rs
  - docs/how-to/**
  - docs/reference/**
  - roadmap/roadmap.yaml
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

Completed on 2026-08-05 in the isolated `minco-task-m13-t06` JJ workspace,
stacked on M13-T05:

- three CLI tests first failed because the ADR-declared `agent eval` subcommand
  did not exist;
- `agent eval --target codex|claude|all` now validates packaged portable skill
  metadata, exact installed fixed-path bytes, cross-client parity and every
  checked-in trigger/boundary contract without writing or invoking a model;
- its schema reports zero writes, child commands, network requests and model
  invocations, while `forward_model.status` remains `not_run` so deterministic
  contract evidence cannot be mistaken for model quality or hosted proof;
- 16 scenarios cover all eight focused skills with one positive trigger and one
  negative boundary apiece, including application/framework mode separation,
  OpenAPI authority, lifecycle planning, diagnosis, evidence-lane separation,
  review-only behavior and explicit-only release preparation;
- the deterministic workflow harness projects all 24 canonical files to each
  client and proves byte parity, read-only evaluation, stale-digest rejection,
  stale destination preservation and byte-preserving user-owned `CLAUDE.md`
  integration;
- `verification/agent-workflows.json` records three passing targets, eight
  trigger and eight boundary contracts, zero model/network calls and explicit
  absent hosted, deployment, runtime and review lanes; and
- 19 agent CLI tests, two portable skill tests, the workflow harness,
  generated-reference check, source-manifest check and zero-error static
  validation pass.

Rust formatting was applied and checked only on the three modified/created Rust files;
no workspace-wide formatter or Clippy pass is claimed. No interactive client,
hosted model, MCP configuration, commit outside this task, merge, release,
registry publication, database, provider, deployment, runtime or production
action was performed.
