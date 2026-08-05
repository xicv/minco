---
id: M13-T05
title: Integrate agent projections with freshly generated Minco applications
milestone: M13
status: complete
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
  - crates/minco-project-view/src/reader.rs
  - crates/minco-project-view/tests/project_view.rs
  - scripts/test/scaffold_templates.py
  - docs/how-to/**
  - docs/reference/**
  - tasks/M13/M13-T05-generated-application-agent-setup.md
  - tasks/M13/M13-T06-cross-client-agent-qualification.md
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

Completed on 2026-08-05 in the isolated `minco-task-m13-t05` JJ workspace,
stacked on M13-T04:

- focused tests first failed because fresh applications had framework-only JJ
  guidance, no explicit agent setup next step, no Claude instruction bridge and
  no user-owned instruction preservation behavior;
- PostgreSQL and SQLite scaffolds now generate identical application-mode
  `AGENTS.md` instructions, list the read-only all-client plan as an explicit
  next command and do not install any projection during `cargo minco new`;
- the Claude projection manages only a minimal `CLAUDE.md` import when a regular
  application-owned `AGENTS.md` exists; an existing unowned Claude file is
  preserved byte-for-byte and reported for manual integration, while a missing
  or unsafe import dependency cannot produce a dangling bridge;
- the bridge bytes participate in the bundle and exact plan digests, and the
  combined ownership receipt records 49 client files without adopting
  `AGENTS.md` or user-owned client configuration;
- generated-project coverage proves equal cross-database projection digests,
  discovery paths, application ProjectView mode, digest-bound sync, instruction
  preservation and explicit setup reporting;
- the generated application check exposed and fixed ProjectView's mismatch with
  the documented plugin catalog: registry plugins may omit a local `path`, while
  workspace plugins retain their explicit repository-relative path; and
- the focused cargo-minco, ProjectView and scaffold-template checks pass, along
  with generated-reference, source-manifest and zero-error static validation.

Rust formatting was applied and checked only on the six modified Rust files; no
workspace-wide formatter or Clippy pass is claimed. No global client setup, MCP
configuration, commit outside this task, merge, release, registry publication,
database, provider, deployment, hosted, runtime or production action was
performed.
