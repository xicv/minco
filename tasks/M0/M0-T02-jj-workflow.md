---
id: M0-T02
title: Make JJ the default development VCS
milestone: M0
status: active
priority: high
area: developer-experience
depends_on: [M0-T01]
operations: []
owned_paths:
  - config/jj/**
  - scripts/jj/**
  - docs/development/jj-workflow.md
checks:
  - bash -n scripts/jj/init.sh scripts/jj/task-start.sh scripts/jj/task-finish.sh
  - python3 scripts/validate_static.py
---

## Goal

Use colocated Jujutsu/Git repositories, native JJ workspaces for parallel tasks, explicit bookmarks for GitHub publication, and repository commands instead of Git hooks.

## Non-goals

Replacing GitHub as the forge or requiring contributors who only review changes to use JJ.
