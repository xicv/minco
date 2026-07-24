# Minco Feedback draft-PR handoff

Date: 2026-07-24
Repository: `xicv/minco`
Bookmark: `agent/feedback-plugin-and-core-audit`
PR title: `feat: strengthen plugin architecture and add Feedback review loop`

## State

The source snapshot was overlaid onto current `main`, rebased across the prior
Rust `1.97.1` compiler-hardening change, and hardened until the broad local
acceptance matrix passed. The resulting change contains the plugin-kernel
improvements, official provider-neutral plugins, the Feedback vertical slice,
database/browser tests, security controls and current verification evidence.

Read:

1. `AGENTS.md`;
2. `FEEDBACK_REVIEW_STATUS.md`;
3. `VERIFICATION.md`;
4. `docs/architecture/capability-audit.md`;
5. `docs/architecture/feedback-loop.md`;
6. `docs/adrs/0014-plugin-lifecycle-and-feedback.md`;
7. `tasks/M6/` and `tasks/M8/M8-T02-compiler-package-gates.md`.

## Verified boundary

Passed locally:

- full Rust format/check/Clippy/test/doc quality gate;
- Feedback feature matrix;
- SQLite and real PostgreSQL store conformance;
- Chromium/Firefox widget E2E and repeated stability run;
- Orders generated applications and TCP E2E;
- plugin validation, contract sync, Plan IR and operation explain traces;
- cargo-deny, gitleaks and npm audit;
- ARM64 native Lambda ZIP packaging;
- SAM linting and read-only CloudFormation/IAM policy validation;
- clean-tree package contents and crates.io publish dry run.

Not performed:

- real AWS deployment or provider-adapter conformance;
- repository-wide Codex Security Deep Scan completion because the external
  scan service terminated two defensive runs before returning an acceptable
  discovery manifest;
- crate upload.

## Task state

- `M6-T02` remains active because its prerequisite `M5-T01` is planned, although
  its current provider-neutral implementation checks pass.
- `M6-T03` remains active because its prerequisite `M2-T01` is active, although
  its implementation acceptance matrix passes.
- `M6-T04` remains planned; the concrete AWS adapters are not implemented.
- `M8-T02` remains complete and its compiler/package gates were rerun against
  this candidate without publishing.

## Release boundary

The only authorized remote mutation for this work is the pushed JJ bookmark and
draft pull request. Do not merge, deploy, tag or run `scripts/release/publish.sh
--execute` without a separate approval and exact-head hosted checks.
