---
id: M9-T09
title: Make local quality authoritative and hosted CI essential-only
milestone: M9
status: complete
priority: high
area: developer-experience/quality
depends_on: [M9-T08]
operations: []
owned_paths:
  - .github/workflows/minco-manual.yml
  - docs/adrs/0013-quality-and-update.md
  - docs/development/testing.md
  - roadmap/roadmap.yaml
  - scripts/ci/**
  - scripts/quality.sh
  - scripts/test/hosted_ci_policy.py
  - scripts/test/repository_truth.py
  - tasks/M9/M9-T09-local-first-ci.md
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - uv run --locked python scripts/test/hosted_ci_policy.py
  - shellcheck scripts/ci/hosted-essential.sh
  - actionlint .github/workflows/minco-manual.yml
  - ./scripts/quality.sh
---

## Goal

Keep Minco's complete quality matrix authoritative and runnable locally while
reducing optional GitHub Actions to a bounded clean-runner check by default and
an explicit release-qualification profile only when that additional evidence is
actually required.

## Acceptance

- `./scripts/quality.sh` retains the complete static, compiler, test, browser,
  documentation, dependency, security and source-identity matrix;
- the manual hosted workflow defaults to an `essential` profile that runs only
  repository truth, format, source identity and a clean Linux all-workspace
  all-target all-feature compiler check;
- a separately selected `release` profile retains the full quality, packaging,
  native ARM64, Plan/SAM and optional Rustack/E2E qualification;
- the essential profile does not install browsers, JJ, ripgrep, security tools,
  Cargo Lambda or Zig, upload browser artifacts, run publish dry-runs, or cache
  workspace target directories;
- same-workflow, same-ref stale runs are cancelled and workflow permissions
  remain read-only;
- a policy regression test fails if a costly release-only step becomes part of
  the default essential path or if the local authoritative gate is weakened;
- documentation records the measured baseline, the two profiles, their
  evidence boundaries, and that standard GitHub-hosted runner minutes are
  currently unbilled because the repository is public.

## Non-goals

- making hosted CI authoritative or a substitute for local quality;
- automatically running hosted CI on every push or pull request;
- using a self-hosted runner for untrusted public pull requests;
- changing application behavior, deployment behavior, release authorization or
  crates.io publication;
- deleting historical workflow runs or artifacts.

## Evidence

Implemented and locally qualified on 2026-07-31 in the isolated
`minco-task-m9-t09` JJ workspace.

- TDD first proved the missing bounded hosted script, missing workflow profile,
  and missing local-policy integration as separate red failures. Three policy
  tests now exercise the script command boundary, manual workflow profile and
  complete local matrix.
- The prior twenty full hosted jobs consumed 320.5 runner wall-minutes with a
  16.2-minute median. The repository is public, so standard runner minutes are
  currently unbilled; the reduction targets runner time, queueing, cache and
  artifact pressure, and future private-repository cost.
- Existing GitHub caches showed a 4,594,842,607-byte task-branch Rust cache and
  a 4,458,880,196-byte main Rust cache. The workflow now sets
  `cache-targets: false`; registry caching remains enabled and failures do not
  save caches.
- `scripts/ci/hosted-essential.sh` passed end to end. A cold local target took
  55.9 seconds and the warmed run took 29.3 seconds.
- The focused policy suite passed 3 tests, repository-truth passed 19 tests,
  and shellcheck plus actionlint passed.
- `./scripts/quality.sh` passed the complete authoritative local matrix in
  17m37s, including 40 browser tests, workspace compilation/tests/Clippy,
  generated PostgreSQL and SQLite applications, documentation, RustSec with
  zero vulnerabilities, npm audit and gitleaks with zero leaks.
- Four live Orders PostgreSQL adapter tests remain ignored without
  `MINCO_ORDERS_TEST_POSTGRES_URL`; no live PostgreSQL claim is made.
- No GitHub workflow was dispatched by this local qualification. Exact-head
  essential hosted evidence is required before merge. No AWS call, deployment,
  publication, promotion or release was performed.
