---
id: M11-T07
title: Deepen the current documentation website
milestone: M11
status: complete
priority: high
area: documentation/product
depends_on: [M9-T08, M11-T01, M11-T03]
operations: []
owned_paths:
  - .github/workflows/docs-pages.yml
  - docs-site/**
  - scripts/docs/**
  - roadmap/tasks.mmd
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
  - tasks/M11/M11-T07-deepen-documentation-site.md
checks:
  - scripts/docs/build.sh
  - scripts/docs/check-links.sh
  - scripts/docs/check-snippets.sh
  - scripts/docs/test-browser.sh
  - ./scripts/quality.sh
  - uv run --locked python scripts/validate_static.py
---

## Goal

Turn the sparse unreleased documentation area into a detailed, navigable guide
to the framework that exists on current `main`, including its public HTTP/API
conventions, CLI workflows, plugin model, testing strategy and exercised
examples.

## Acceptance

- `next` has a complete sidebar and clear learning paths for application
  developers, plugin authors and operators;
- detailed pages cover project structure, resource API request/response shapes,
  pagination and conditional writes, plugin distribution/conformance, testing,
  deployment planning and zero-idle boundaries;
- examples use existing checked source or commands and identify whether their
  evidence is offline, hosted or provider-backed;
- search, links, code snippets, keyboard navigation, responsive layout and
  stable/next switching are covered by executable checks;
- stable `0.5.0` documentation remains immutable and unreleased behavior is
  never presented as published or provider-qualified.

## Non-goals

- implementing the planned plugin mutation workflows from M11-T04;
- replacing the generated-reference work planned in M11-T06;
- creating the full provider/example matrix planned in M11-T05;
- claiming local examples prove live AWS deployment or production behavior.

## Implementation

- The unreleased site now has 12 current-development pages organized for
  application developers, plugin authors and AWS operators. The sidebar and
  landing-page cards expose framework, resource, plugin, deployment, testing,
  example and zero-idle paths without changing stable `0.5.0` content.
- One shared VitePress layout renders the unreleased warning on every `next`
  document and links back to the stable version. Search indexes the detailed
  pages and the site keeps its existing Pages base path and canonical URLs.
- Resource guides document the five-action OpenAPI family, envelopes, bounded
  cursor queries, idempotent create, strong ETags, conditional update/delete,
  Problem Details and the nearest-boundary tests. They point to the exercised
  Orders contract and public `minco-http` surface rather than inventing an ORM
  or generic repository.
- Plugin guides document archive-visible distribution records, the public
  `minco-test` builder, report/diagnostic shapes and the explicit offline-only
  assurance boundary. Deployment and zero-idle guides keep package, apply,
  hosted verification, promotion and live observation as separate evidence.
- The checked examples index labels local, compiled/ignored and provider-live
  boundaries. CLI examples were compared with the current binary help; planned
  plugin mutation commands and missing dry-run support remain clearly marked.
- Browser coverage now exercises learning paths, persistent version warnings,
  local search, labelled controls, console errors and horizontal containment
  across desktop and mobile Chromium. Focus, touch and reduced-motion behavior
  remain explicit in the theme.

## Local evidence

- `scripts/docs/build.sh` generated the production bundle and sitemap with
  VitePress `1.6.4`.
- `scripts/docs/check-links.sh` passed 30 internal links, seven external links
  and 28 canonical pages.
- `scripts/docs/check-snippets.sh` syntax-checked 74 fenced blocks and enforced
  both stable version markers and a minimum detailed `next` surface.
- `scripts/docs/test-browser.sh` passed 11 applicable desktop/mobile journeys;
  the mobile-only viewport journey remained intentionally skipped on desktop.
- Desktop `next` and mobile resource-guide production screenshots were
  visually reviewed after hydration; the checked mobile page had no horizontal
  overflow.
- `./scripts/quality.sh` passed repository truth, formatting, all-feature
  Clippy/tests, generated PostgreSQL/SQLite consumers, browser suites, Rustdoc,
  dependency/license/advisory policy, secret scanning and evidence freshness.
  Bounded real-AWS, Rustack and configured PostgreSQL tests remained explicitly
  ignored/not run by this offline change.
