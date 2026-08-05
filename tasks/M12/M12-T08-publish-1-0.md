---
id: M12-T08
title: Record and promote the published Minco 1.0 release
milestone: M12
status: complete
priority: critical
area: release/1.0
depends_on: [M12-T07]
operations: []
owned_paths:
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - PUBLISHING.md
  - README.md
  - VERIFICATION.md
  - docs/**
  - docs-site/**
  - scripts/validate_static.py
  - scripts/test/repository_truth.py
  - tasks/M12/M12-T08-publish-1-0.md
  - verification/**
checks:
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/validate_publish.py --expect-published --check-registry --require-registry
  - uv run --locked python scripts/source_manifest.py --check
  - npm --prefix docs-site run build
  - npm --prefix docs-site run test:browser
---

## Goal

Record the independently verified 1.0.0 registry and GitHub release state,
promote the already-qualified versioned manual from candidate to stable, and
deploy that documentation state without rewriting the immutable release tag.

## Acceptance

- repository truth names 1.0.0 as the published 33-package baseline and retains
  no first-publication candidates;
- changelog, publishing guidance, handoff, README and verification records keep
  source, hosted qualification, tag, registry, documentation and live AWS
  evidence as separate states;
- the documentation landing page, version selector, version index and frozen
  1.0.0 manual identify 1.0.0 as stable while `next` remains unreleased;
- static, registry, source-manifest, documentation build/link and desktop/mobile
  browser gates pass before merge;
- the stable Pages deployment is verified from the exact merged main SHA.

## Non-goals

- changing the immutable `v1.0.0` tag or rebuilding its released crate family;
- claiming trusted-publisher configuration merely because first publication
  established package ownership;
- creating, deploying, promoting or deleting any live AWS application resource;
- converting historical candidate or provider evidence into current live proof.

## Evidence

This post-publication task started in the isolated `minco-task-m12-t08` JJ
workspace from exact merged main `39a69e36b051724c383da75d5907a824cbd2765b`.
The immutable `v1.0.0` tag resolves to that SHA. Exact-head release
qualification run `30986838335` and exact-main release qualification run
`30990218161` passed. The dependency-ordered manual publication completed for
all 33 crates, and the independent registry validator reported zero errors and
33 successful exact-version checks. GitHub release `v1.0.0` is published.

The existing Pages run `30990196620` deployed the candidate site from exact
main. Stable source promotion and its resulting Pages deployment remain the
separate work of this task. No live AWS resource was created or changed during
the release or this documentation promotion.

The repository-truth RED check failed on all stale 0.6.0/candidate markers and
on an adoption-budget assumption that reused the current first-publication
list for an immutable candidate measurement. The published state now keeps
`new_publishable_packages` empty while a separate qualified-candidate list
preserves the historical measurement input. Its regression fixture also uses
the real previous 0.6.0 baseline after publication. The stable docs build
passes, and the desktop/mobile suite exercises the 1.0 installation, realtime,
DynamoDB, ProjectView/MCP/workbench and lifecycle pages in published mode.
