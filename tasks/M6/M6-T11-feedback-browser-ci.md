---
id: M6-T11
title: Unify local and hosted Feedback browser quality
milestone: M6
status: complete
priority: high
area: developer-experience
depends_on: [M6-T03]
operations: []
owned_paths:
  - plugins/minco-plugin-feedback/package.json
  - plugins/minco-plugin-feedback/package-lock.json
  - plugins/minco-plugin-feedback/playwright.config.cjs
  - scripts/bootstrap.sh
  - scripts/quality.sh
  - scripts/test/feedback_browser.sh
  - quality.toml
  - .github/workflows/minco-manual.yml
  - docs/development/testing.md
  - tasks/M6/M6-T11-feedback-browser-ci.md
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - scripts/test/feedback_browser.sh
  - shellcheck scripts/bootstrap.sh scripts/quality.sh scripts/test/feedback_browser.sh
  - actionlint .github/workflows/minco-manual.yml
  - ./scripts/quality.sh
---

## Goal

Make the Feedback browser matrix a single deterministic gate that developers
and the optional manual GitHub workflow run the same way, with useful reports
and traces retained by hosted CI.

## Non-goals

- make hosted CI automatic or authoritative;
- add Playwright sharding or another browser project without measured need;
- cache Playwright browser binaries contrary to the upstream recommendation;
- change Feedback widget behavior.

## Acceptance

- A clean local checkout installs the locked npm dependencies and the exact
  Chromium headless shell and Firefox binaries before running the matrix.
- `scripts/quality.sh` includes the browser matrix instead of relying on a
  hosted-only step.
- GitHub failures produce inline annotations plus an uploaded HTML, JUnit,
  trace, screenshot and video evidence bundle.
- The workflow retains its manual-only trigger and immutable action pins.
- Hosted CI pins a supported Node runtime instead of depending on the mutable
  `ubuntu-latest` tool cache.
- Chromium and Firefox still run fully parallel with the measured stable
  two-worker hosted policy; no retry masks a first-run failure.
- The locked Playwright release supports the repository's current Node runtime
  on developer machines and hosted runners.

## Current evidence

The exact merged-main workflow run `30414524718` spent 61 seconds in the browser
step: browser installation dominated setup, while 40 tests used two workers and
passed in 22 seconds. After the local Playwright cache was cleaned, the direct
test command failed because the exact Chromium headless shell and Firefox
binaries were absent. The first self-bootstrapping run then exposed Playwright
1.59.1 hanging after its Chromium download under Node 26.5.0; the same command
completed in 29 seconds under Node 24.18.0. Playwright 1.62.0 is the current
release and raises its declared Node floor from 18 to 20 while supporting
current Node 22, 24 and 26 runtimes.

## Completion evidence

- `scripts/test/feedback_browser.sh` self-bootstrapped the exact Chromium
  headless shell and Firefox revisions and passed all 40 tests on Node 26.5.0.
  The focused browser run completed in 10.1 seconds after installation.
- The manual GitHub workflow remains `workflow_dispatch` only, pins Node
  24.18.0 and every action by immutable SHA, runs the same browser gate through
  `scripts/quality.sh`, emits GitHub annotations, and retains the complete
  browser evidence directory for 14 days.
- `bash -n`, `shellcheck`, `actionlint`, the Playwright configuration syntax
  check, npm audit, static validation and the full authoritative
  `./scripts/quality.sh` gate passed.
- No browser cache or sharding was added. The npm dependency install takes less
  than one second locally, and upstream recommends downloading browser binaries
  rather than caching them.
- No release, deployment, registry publication, AWS mutation or automatic
  hosted-CI trigger was performed.
