---
id: M0-T01
title: Establish Minco architecture constitution
milestone: M0
status: complete
priority: critical
area: architecture
depends_on: []
operations: []
owned_paths:
  - docs/architecture/**
  - docs/DECISIONS.md
checks:
  - python3 scripts/validate_static.py
---

## Goal

Record the contract-first, AI-native, AWS-native, minimal-idle and static-plugin invariants that all later changes must preserve.

## Evidence

The decision register, architecture guides, `AGENTS.md`, workspace dependency boundaries and static validator are committed.
