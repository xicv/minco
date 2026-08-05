---
id: M13-T03
title: Implement digest-bound agent plan, sync and doctor commands
milestone: M13
status: complete
priority: critical
area: ai/cli
depends_on: [M13-T02]
operations: []
owned_paths:
  - Cargo.lock
  - crates/minco-cli/Cargo.toml
  - crates/minco-cli/src/agent_cmd.rs
  - crates/minco-cli/src/main.rs
  - crates/minco-cli/tests/agent_cli.rs
  - docs/how-to/**
  - docs/reference/**
  - tasks/M13/M13-T03-agent-plan-sync-doctor.md
  - tasks/M13/M13-T04-agent-context.md
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p cargo-minco --test agent_cli --locked
  - cargo test -p cargo-minco --bin cargo-minco agent_cmd::tests --locked
  - uv run --locked python scripts/docs/generate_reference.py --check
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

Completed on 2026-08-05 in the isolated `minco-task-m13-t03` JJ workspace,
stacked on M13-T02:

- five initial integration tests failed because `agent` was not yet a CLI
  command, preserving the test-first red boundary;
- deterministic plans now cover 24 canonical files per client plus the shared
  ownership manifest, manual MCP configuration actions, conflicts and a digest
  that binds current file identities and intended bytes;
- exact-digest sync preserves unowned and edited files, retains projections
  for unselected clients, stages privately, revalidates root/parent/file
  identity and rolls back exact-owned creates or replacements on failure;
- eight integration tests cover read-only planning/doctor behavior, missing and
  stale digests, repeated sync, user-owned, absent and edited managed files,
  symlinks, exact Codex/Claude parity, owned updates and unmanaged neighboring
  files;
- three writer unit tests cover a concurrent no-clobber destination, a replaced
  staging identity that is neither published nor deleted, and a replaced
  parent identity without following the replacement;
- the generated CLI reference and human workflow reference describe the new
  command and its ownership boundary; and
- a read-only framework-project plan reports 49 creates, two manual actions and
  zero conflicts, while doctor reports `not_installed`, zero writes and MCP
  state `unknown`; the package list contains all 26 bundle assets plus the new
  command and integration-test sources; and
- static validation reports zero errors and warnings. File-scoped Rust format
  checks cover only the three modified/created Rust files; no workspace-wide
  formatter or Clippy pass is claimed.

The first integration relink was blocked by `ld: write() failed, errno=28 (No
space left on device)`. Only the current task workspace's derived `target/`
cache was removed with `cargo clean`; the tests then passed with debug symbols
disabled for the test profile. No other JJ workspace, source data or user-owned
file was cleaned.

No skill instruction was executed, no client MCP/configuration file was parsed
or changed, and no database, provider, release, registry, publication,
deployment or production action was performed.
