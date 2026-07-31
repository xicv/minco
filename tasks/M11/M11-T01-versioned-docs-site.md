---
id: M11-T01
title: Build the versioned Diataxis documentation site
milestone: M11
status: active
priority: critical
area: documentation
depends_on: [M9-T07, M10-T03]
operations: []
owned_paths:
  - .github/workflows/docs-pages.yml
  - docs/**
  - docs-site/**
  - scripts/docs/**
  - scripts/quality.sh
  - scripts/source_manifest.py
  - README.md
  - roadmap/roadmap.yaml
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
  - tasks/M11/M11-T01-versioned-docs-site.md
checks:
  - scripts/docs/build.sh
  - scripts/docs/check-links.sh
  - scripts/docs/check-snippets.sh
  - scripts/docs/test-browser.sh
  - ./scripts/quality.sh
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

## Implementation

- Stable VitePress `1.6.4` renders versioned `0.5.0` and visibly unreleased
  `next` documentation at the repository Pages base path.
- Local search, canonical links, sitemap generation, stable/next navigation,
  Laravel-inspired typography, Minco branding, dark mode, reduced motion and
  small-screen layouts are part of the checked site shell.
- The first API, AWS deployment and plugin tutorials pin Minco `0.5.0` and Rust
  `1.97.1`. Resource API, environment, deployment-plan, CLI, testing,
  architecture and zero-idle pages keep one primary Diataxis mode each.
- The README is now a concise product and quick-start entry point. Existing
  repository documents were not moved, so their paths require no redirects.
- The locked dependency tree overrides Vite to patched `6.4.3` while retaining
  stable VitePress; npm audit reports zero vulnerabilities. Dependency
  lifecycle scripts are disabled for local and hosted documentation installs.

## Local evidence

- `scripts/docs/build.sh`: 17 canonical HTML pages and sitemap generated.
- `scripts/docs/check-links.sh`: 13 internal links, 3 external links and all 17
  production canonical links passed.
- `scripts/docs/check-snippets.sh`: 33 fenced blocks passed syntax checks and
  every tutorial carries exact version markers.
- `scripts/docs/test-browser.sh`: 9 desktop/mobile journeys passed; the
  mobile-only journey is intentionally skipped in the desktop project.
- The browser configuration accepts `MINCO_DOCS_BASE_URL` so the same journeys
  can qualify the deployed Pages artifact without starting a local server.
- Desktop home and mobile tutorial screenshots were visually reviewed from the
  production bundle; no horizontal page overflow or browser-console error was
  observed.

GitHub Pages deployment and production-URL verification remain pending until
the exact source change is merged.
