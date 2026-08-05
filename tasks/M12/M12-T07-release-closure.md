---
id: M12-T07
title: Close the exact Minco 1.0 release boundary
milestone: M12
status: complete
priority: critical
area: release/1.0
depends_on: [M6-T01, M12-T06]
operations: []
owned_paths:
  - .github/workflows/**
  - CHANGELOG.md
  - PUBLISHING.md
  - VERIFICATION.md
  - crates/minco-cli/tests/mcp_cli.rs
  - docs/**
  - docs-site/**
  - proofs/realtime-appsync/**
  - roadmap/roadmap.yaml
  - scripts/**
  - tasks/M12/M12-T07-release-closure.md
  - verification/**
checks:
  - ./scripts/quality.sh
  - proofs/realtime-appsync/scripts/test-local.sh
  - scripts/release/publish.sh --skip-quality
  - scripts/release/package-list.sh
  - scripts/release/qualify-candidate.sh
  - uv run --locked python scripts/source_manifest.py --check
---

## Goal

Close the post-candidate gaps introduced by the realtime, ProjectView/MCP,
workbench, lifecycle, documentation-site and DynamoDB slices, then bind one
reviewable Minco 1.0 source tree to complete local and hosted release evidence.

## Acceptance

- the standalone AppSync proof builds from its committed lockfile and remains a
  mandatory release gate;
- checked-in MCP integration coverage exercises the `2026-07-28`
  `server/discover` lifecycle and per-request metadata;
- release truth, package inventory and publishing guidance agree on the exact
  33-package family and explicitly handle all first-publication crates without
  allowing a partial OIDC upload;
- the candidate version link resolves to a complete frozen `1.0.0`
  documentation tree covering realtime, DynamoDB, ProjectView/MCP/workbench and
  the preview/apply/verify/promote/rollback lifecycle;
- regenerated release, recovery, load and source-manifest evidence bind the
  exact final source tree;
- full local quality and exact-head hosted release qualification pass before
  merge, and exact-main qualification remains a separate release gate.

## Non-goals

- treating local or emulator proof as fresh AWS managed-service evidence;
- placing registry tokens, credentials, customer data or provider secrets in
  source or generated evidence;
- publishing, tagging, deploying or promoting before exact-main qualification;
- describing an ordered crates.io upload as atomic.

## Evidence

The release closure began from exact remote main
`e4c098ff3e65c5038b5e205618af1558e464f9e7` in the isolated
`minco-task-m12-t07` JJ workspace. Red-first checks proved that the standalone
AppSync consumer lock still named 0.7 packages and failed under `--locked`, the
mandatory candidate command set omitted that proof, and the configured 1.0
candidate documentation URL had no versioned source directory.

The regenerated standalone lock names the 1.0 Minco dependencies and its full
Rust, browser, template, authority and shell gate passes. Candidate and hosted
release policies now run that proof explicitly. The CLI child-process
regression uses MCP `2026-07-28` `server/discover`, requires the modern
per-request metadata envelope, lists exactly six read-only tools, redacts the
repository root, and shuts down cleanly.

The documentation product freezes the complete `next` manual under `1.0.0`,
adds version-aware sidebars and candidate navigation, and covers all 18
components plus realtime, DynamoDB, ProjectView/MCP/workbench, preview review
environments, exact static-site publication, compatibility rollback and
alarm-guarded canaries. Snippet, build, internal/external/canonical link, and
desktop/mobile browser gates exercise the candidate route and feature pages.

Current publishing guidance and changelog agree on 33 packages and five
first-publication crates. The manual-token path publishes the complete exact-tag
family in dependency order. The OIDC workflow checks repository truth before
requesting a token and refuses to publish while any first-publication package
remains, preventing a partial existing-family upload before new-crate
ownership exists.

`verification/1.0-candidate-release-gates.json` is the final source-bound local
record. It requires the complete quality/security matrix, standalone AppSync
proof, Feedback browser suite, HTTP E2E, Rustack, 33-package publish dry run and
archive inventory, recovery, load, and final source-manifest verification.
Recovery/load reports remain separately schema-validated and excluded from the
source digest to avoid self-reference.

No real AWS resource, database, deployment, promotion, registry upload, tag,
GitHub release, stable documentation promotion or product runtime was created
or changed by this source-closure task. Hosted exact-head qualification, merge,
exact-main qualification and every irreversible publication action remain
separate gates.
