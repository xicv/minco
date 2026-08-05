---
id: M13-T03
title: Implement digest-bound agent plan, sync and doctor commands
milestone: M13
status: planned
priority: critical
area: ai/cli
depends_on: [M13-T02]
operations: []
owned_paths:
  - crates/minco-cli/src/agent_cmd.rs
  - crates/minco-cli/src/main.rs
  - crates/minco-cli/tests/agent_cli.rs
  - docs/how-to/**
  - docs/reference/**
  - tasks/M13/M13-T03-agent-plan-sync-doctor.md
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p cargo-minco --test agent_cli --locked
  - uv run --locked python scripts/validate_static.py
---

## Goal

Expose deterministic read-only planning, exact-digest synchronization and
read-only drift diagnosis for Codex and Claude project skill projections.

## Acceptance

- plans are stable JSON and include creates, owned updates, unchanged files,
  conflicts, manual actions and an exact digest;
- sync rejects a missing or stale expected digest;
- user-owned files and edited managed files are never overwritten;
- fixed destinations reject traversal, symlinks, identity changes, races and
  non-regular entries;
- publication stages privately and never deletes unmanaged content; and
- doctor reports version, digest, discovery, projection and MCP configuration
  state without writing.

## Non-goals

- user-level/global installation;
- arbitrary source or destination paths;
- parsing and rewriting existing client JSON/TOML;
- running skill instructions; or
- provider, database, release or deployment actions.

## Evidence

Pending implementation and local qualification.
