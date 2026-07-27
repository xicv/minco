---
id: M11-T01
title: Build the versioned Diataxis documentation site
milestone: M11
status: planned
priority: critical
area: documentation
depends_on: [M9-T07, M10-T03]
operations: []
owned_paths:
  - docs/**
  - docs-site/**
  - scripts/docs/**
  - README.md
  - tasks/M11/M11-T01-versioned-docs-site.md
checks:
  - scripts/docs/build.sh
  - scripts/docs/check-links.sh
  - scripts/docs/check-snippets.sh
  - uv run --locked python scripts/validate_static.py
---

## Goal

Implement the accepted tutorials/how-to/reference/explanation map as a
searchable, versioned documentation product for application developers, plugin
authors, operators, contributors, and AI coding agents.

## Acceptance

- stable releases and `next` are clearly separated;
- the first API, AWS deployment, and plugin tutorials run against exact
  versions;
- internal/external links and executable snippets are checked;
- redirects preserve existing document links during mechanical moves;
- README becomes a concise entry point rather than an exhaustive reference.

## Non-goals

- mixing all four documentation modes in every page;
- rewriting every page in one unreviewable move;
- claiming local tutorial success is live deployment proof.
