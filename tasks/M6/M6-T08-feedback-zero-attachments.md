---
id: M6-T08
title: Support an enforced text-only Feedback profile
milestone: M6
status: complete
priority: high
area: plugins/feedback
depends_on: [M6-T03]
operations:
  - createFeedback
owned_paths:
  - plugins/minco-plugin-feedback/**
  - scripts/test/feedback_contract.py
  - tasks/M6/M6-T08-feedback-zero-attachments.md
checks:
  - uv run --with pyyaml python3 scripts/test/feedback_contract.py
  - node --check plugins/minco-plugin-feedback/assets/widget.js
  - cargo test -p minco-plugin-feedback --all-features --locked
  - npm --prefix plugins/minco-plugin-feedback run test:browser
---

## Goal

Allow an application to select a real text-only Feedback boundary by setting
`max_attachments` to zero.

## Non-goals

- changing the default attachment count;
- changing screenshot, voice, transcription, storage, or authorization defaults;
- publishing a new Minco release.

## Acceptance

- Configuration and the OpenAPI widget contract accept zero through eight attachments.
- The bundled widget renders no screenshot, file, or voice controls when the limit is zero.
- The HTTP parser rejects every multipart attachment field when the limit is zero.
- Existing attachment-enabled profiles remain unchanged.

## Evidence

- `FeedbackConfig` and the plugin OpenAPI contract accept zero through eight
  attachments. The browser-safe widget projection preserves the configured
  zero.
- The bundled widget hides screenshot, file, and voice controls when the limit
  is zero. Chromium and Firefox pass all 40 browser tests.
- The HTTP parser returns `422 Unprocessable Entity` before reading or
  persisting a multipart attachment under the zero-attachment profile.
- `cargo test -p minco-plugin-feedback --all-features --locked` passes 44 unit
  tests and both persistence-adapter tests.
- `cargo minco plugin validate` reports no findings and `cargo minco deploy
  plan` reports no diagnostics.
- `./scripts/quality.sh` passes compiler, Clippy, workspace, generated-app,
  documentation, dependency, audit, and secret gates after deterministic
  verification artifacts are refreshed.
- Publication remains a separate release operation; this task changes source
  only.
