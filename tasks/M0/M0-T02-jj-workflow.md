---
id: M0-T02
title: Make JJ the default development VCS
milestone: M0
status: complete
priority: high
area: developer-experience
depends_on: [M0-T01]
operations: []
owned_paths:
  - config/jj/**
  - scripts/jj/**
  - crates/minco-cli/src/vcs.rs
  - docs/development/jj-workflow.md
  - docs/reference/cli.md
  - tasks/M0/M0-T02-jj-workflow.md
checks:
  - bash -n scripts/jj/init.sh scripts/jj/task-start.sh scripts/jj/task-finish.sh
  - python3 scripts/validate_static.py
---

## Goal

Use colocated Jujutsu/Git repositories, native JJ workspaces for parallel tasks, explicit bookmarks for GitHub publication, and repository commands instead of Git hooks.

## Non-goals

Replacing GitHub as the forge or requiring contributors who only review changes to use JJ.

## Evidence

On 2026-07-24, the repository configuration parsed under JJ 0.43.0 with
colocated Git enabled, automatic new-bookmark pushing disabled, and the
documented log revset and aliases accepted. The three shell entrypoints passed
Bash syntax and ShellCheck, and static repository validation passed.

The workflow was also exercised rather than inferred: `cargo minco vcs
task-start M0-T02` created the isolated `task-m0-t02` workspace from the current
integration parent, `cargo minco --json vcs status` reported that workspace,
and the repository had no unresolved JJ conflicts. GitHub remains transport
only; no remote push was performed during this verification.

A focused review of the configuration, wrappers and workflow documentation
found no remaining correctness, command-injection, conflict-handling or
recovery issue within the task's declared scope.

## 2026-07-25 regression correction

While closing dependent production tasks, `task-start` was observed creating
the new working change beside the current task because `jj workspace add`
defaults to the current change's parents. This silently omitted the completed
prerequisite until a manual rebase.

The CLI now passes `-r @`, so the new working change is always created on top of
the exact current change. A unit regression test locks the generated command,
the workflow documentation explains dependent-task ordering, and starting
`M6-T04` from the reviewed `M6-T02` descendant provides an end-to-end proof.
No remote or cloud service was touched by this correction.
