---
id: M4-T03
title: Verify the source-workspace update command
milestone: M4
status: active
priority: medium
area: maintenance
depends_on: [M0-T02]
operations: []
owned_paths:
  - crates/minco-cli/src/update.rs
  - docs/development/update.md
checks:
  - cargo minco update check
  - cargo test -p cargo-minco
---

## Goal

Check pinned toolchains and dependencies without mutation, and require an explicit clean-workspace `--yes` flow before applying updates and rerunning gates.
