---
id: M7-T03
title: Reconcile milestone truth and expose the real-AWS closure gate
milestone: M7
status: complete
priority: critical
area: stabilization/governance
depends_on: [M7-T02, M9-T09, M10-T07, M11-T05, M11-T06, M11-T08]
operations: []
owned_paths:
  - docs-site/playwright.config.mts
  - docs/reference/generated/diagnostics.md
  - docs/roadmap/**
  - roadmap/**
  - scripts/validate_static.py
  - scripts/test/hosted_ci_policy.py
  - scripts/test/repository_truth.py
  - tasks/M7/M7-T03-roadmap-transition.md
  - tasks/M10/M10-T08-real-aws-controller-rehearsal.md
  - tasks/M12/M12-T03-adoption-completion.md
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - uv run --locked python scripts/test/repository_truth.py
  - uv run --locked python scripts/validate_static.py
  - cargo minco task ready --json
  - ./scripts/quality.sh
---

## Goal

Reconcile roadmap status with exact completed-task and prerequisite evidence,
retain every external proof boundary, and expose the next executable task
without treating source readiness as deployment authority.

## Acceptance

- M9 becomes complete only after its task set, prerequisite and exit signals
  are re-audited;
- M7 and M11 remain active while their M10 milestone prerequisite is active;
- M10 remains active and gains one ready task for the missing bounded real-AWS
  controller and rollback rehearsal;
- M12 remains planned and its adoption task depends on the exact GarmentIQ
  evidence task rather than the earlier gap audit;
- repository truth rejects a stale active milestone whose tasks and milestone
  prerequisites are all complete;
- repository truth rejects ready task evidence inside a planned milestone;
- `cargo minco task ready --json` returns only M10-T08 after the transition.

## Non-goals

- contacting AWS or another provider;
- deploying, promoting, rolling back or deleting an environment;
- marking M10, M7, M11 or M12 complete without their prerequisite evidence;
- publishing a crate, tag, release or documentation site.

## Completion evidence

Completed on 2026-08-03 in the isolated `minco-task-m7-t03` JJ workspace
against merged main `b6a9cafa8f1d622306f7ed103c9259158e5e50f7`.

- The evidence audit found that M9's nine tasks, M4 prerequisite and every
  stated lifecycle/developer-experience exit signal are complete, so M9 moves
  to `complete`. M7 and M11 retain `active` because M10 remains an active
  milestone prerequisite; M12 remains `planned`.
- M10's seven implementation/research tasks contain explicit statements that
  the post-M10 controller, promotion and rollback path was not exercised in
  AWS. M10-T08 now owns that missing rehearsal and remains separately blocked
  on an exact operator authority gate.
- Red-green repository-truth coverage first failed because neither
  `STATIC-TRUTH-ROADMAP-003` nor planned-milestone rejection of a `ready` task
  existed. Both focused tests and the complete 37-test repository-truth suite
  now pass.
- `cargo minco task ready --json` returns exactly M10-T08. M12-T03 now depends
  on M7-T02, the merged GarmentIQ evidence task, instead of M7-T01's earlier
  gap audit.
- `./scripts/quality.sh` passed after refreshing the deterministic diagnostics,
  task graph, static/deep-review, adoption and 751-file source-manifest chain.
  The gate covered static/publish/deep review, all workspace compiler/Clippy/
  test/rustdoc profiles, generated PostgreSQL and SQLite applications, 40
  Feedback browser tests, 112 documentation snippets, VitePress build and
  links, 13 applicable documentation browser journeys, dependency/license/
  advisory and npm audits, Gitleaks and exact source identity.
- Deep review added no error. Its existing two Rust unwrap/expect warnings,
  one SQLite migration `DROP TABLE` warning and one informational example
  error-boundary item remain unchanged outside this task's source scope.
- A repeated exact-tree run exposed a local browser-server ownership race:
  VitePress could silently leave occupied port 4173 while Playwright accepted
  another application's page at that URL. A red policy regression now requires
  a configurable Minco-specific port and Vite `--strictPort`; the browser gate
  fails at server startup rather than testing an unrelated application.

No AWS API or hosted application endpoint was contacted. No environment,
database, resource, release, tag, registry entry or documentation site was
created, modified or published. Exact-head hosted and post-merge qualification
remain required before this transition is accepted on main.
