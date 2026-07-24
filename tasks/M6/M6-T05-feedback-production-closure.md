---
id: M6-T05
title: Close Feedback production and security release gates
milestone: M6
status: ready
priority: high
area: plugins/feedback
depends_on: [M6-T03, M6-T04]
operations:
  - createFeedback
  - getClientFeedback
  - replyToFeedback
  - listDeveloperFeedback
  - getFeedbackAiContext
owned_paths:
  - plugins/minco-plugin-feedback/**
  - plugins/catalog.toml
  - docs/adrs/0014-plugin-lifecycle-and-feedback.md
  - docs/architecture/capability-audit.md
  - tasks/M6/M6-T03-feedback-loop.md
  - tasks/M6/M6-T05-feedback-production-closure.md
checks:
  - cargo minco plugin validate
  - cargo test -p minco-plugin-feedback --all-features --locked
  - cargo test --workspace --all-features --locked
  - cargo minco deploy plan
---

## Goal

Make the explicit production-release decision for the official Feedback plugin
after its compiler, browser, database, provider-adapter, bounded real-AWS,
cleanup, and repository-wide security gates have all produced reviewable
evidence.

## Non-goals

- create an SES identity when the account has no pre-existing verified sender;
- create a slow, cost-bearing CloudFront distribution solely to change a
  lifecycle label;
- represent local emulation, template validation, or compiler coverage as a
  live provider operation;
- stabilize unrelated optional plugins.

## Acceptance

- The completed M2-T01, M6-T03, and M6-T04 prerequisites are reflected
  consistently in task and architecture evidence.
- The repository-wide Deep Security Scan is completed or rejoined without
  launching a duplicate scan, and every validated finding is fixed and
  reverified.
- Feedback's runtime descriptor and catalog stability labels agree.
- Exact-head compiler, plugin, test, deployment, dependency, license, and
  secret checks pass.
- Live-cloud boundaries remain explicit, and no cloud service is touched
  without an append-only action and cleanup record.
- A focused single-task review finds no remaining release-blocking defect.

## Current evidence

M6-T03 and M6-T04 are complete. Feedback's compiler, HTTP, memory, PostgreSQL,
SQLite, CLI, Chromium, and Firefox gates pass. The selected AWS adapter suite
has exact-resource IAM, local Rustack conformance, bounded real-AWS provider
proof, and verified cleanup. Stability remains beta until this task completes
the repository-wide scan and exact-head release review.
