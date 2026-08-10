---
id: M14-T12
title: Bound GitHub Actions to platform-only work
milestone: M14
status: complete
priority: critical
area: developer-experience/quality
depends_on: [M9-T09, M14-T11]
operations: []
owned_paths:
  - AGENTS.md
  - .github/workflows/local-dev-runtime-validation.yml
  - .github/workflows/minco-manual.yml
  - docs/DECISIONS.md
  - docs/adrs/0013-quality-and-update.md
  - docs/adrs/0038-local-first-actions-boundary.md
  - docs/development/testing.md
  - scripts/aws/test-multi-release-phase-result.sh
  - scripts/ci/**
  - scripts/release/publish.py
  - scripts/test/hosted_ci_policy.py
  - scripts/test/repository_truth.py
  - tasks/M14/M14-T12-local-first-actions-boundary.md
  - verification/**
checks:
  - uv run --locked python scripts/test/hosted_ci_policy.py
  - uv run --locked python scripts/test/repository_truth.py
  - shellcheck -x scripts/aws/test-multi-release-phase-result.sh scripts/ci/local-release.sh scripts/ci/local-runtime.sh
  - actionlint .github/workflows/minco-manual.yml .github/workflows/publish-crates.yml
  - scripts/aws/validate.sh
  - uv run --locked python scripts/source_manifest.py --check
---

# M14-T12 - Bound GitHub Actions to platform-only work

## Goal

Keep only platform-required work on GitHub: path-filtered Pages deployment,
crates.io OIDC publication, and one short manually dispatched clean-Linux
compatibility check. Run complete quality, release qualification, local service
lifecycle, Rustack conformance and E2E locally.

## Acceptance

- the repository contains exactly the three approved workflow files;
- hosted qualification has no release profile, Rustack, E2E, native build,
  browser artifact or complete-quality path;
- one local command reproduces the removed release matrix and includes owned
  local runtime plus Rustack conformance;
- policy tests reject extra workflow files or a broadened hosted boundary;
- agent instructions prohibit temporary branch workflows;
- documentation keeps local, hosted compatibility, publication, provider and
  deployment evidence distinct.

## Non-goals

- changing application, plugin, deployment or publication behavior;
- registering a self-hosted GitHub Actions runner;
- deleting workflow history, artifacts, branches or pull requests;
- claiming that local emulation proves real AWS behavior.

## Research and observed baseline

Research was completed on 2026-08-10 before repository edits. Official GitHub
documentation confirms that standard hosted runners are free for public
repositories, Pages runner use is free, cache storage above the repository's
10 GB allowance can be billed, manual workflows can be disabled through the
REST API, and self-hosted runners are unsafe for untrusted public pull requests.

Current evidence showed four workflow files on `main`, sixteen active workflow
registrations, 116 Minco hosted-qualification runs consuming about 1,503 runner
wall-minutes from 2026-08-01 through 2026-08-10, and 69 caches occupying
10,693,043,757 bytes. Before source changes, thirteen nonessential workflow
registrations were disabled, two in-progress nonessential runs were cancelled,
and 58 regenerable non-`main` caches totalling 4,479,098,973 bytes were deleted.
All eleven retained caches are scoped to `main` and total 6,213,944,784 bytes.

## Evidence

Research preceded implementation. GitHub's official billing, runner, cache,
workflow, execution-protection and self-hosted-runner documentation established
the platform boundary recorded in ADR-0038. The repository is public, so the
observed standard-runner minutes are not a current runner charge; cache storage,
queueing and workflow sprawl were the actionable costs.

Remote maintenance on 2026-08-10:

- cancelled the two active nonessential runs `31343110517` and `31343105395`;
- disabled thirteen nonessential workflow registrations and temporarily
  disabled the old full hosted-qualification registration `319308520` until
  this source change reaches the default branch;
- left only Pages workflow `324530729` and crates.io publication workflow
  `319308521` active; enable `319308520` only after its bounded replacement is
  merged;
- deleted 58 regenerable non-`main` caches totalling 4,479,098,973 bytes;
- retained all workflow history, artifacts, branches, pull requests and eleven
  `main` caches totalling 6,213,944,784 bytes.

Focused verification:

- `uv run --locked python scripts/test/hosted_ci_policy.py`: PASS, 8 tests;
- `uv run --locked python scripts/test/repository_truth.py`: PASS, 41 tests;
- `uv run --locked python -m py_compile scripts/test/hosted_ci_policy.py scripts/test/repository_truth.py scripts/release/publish.py`: PASS;
- `shellcheck -x scripts/aws/test-multi-release-phase-result.sh scripts/ci/local-release.sh scripts/ci/local-runtime.sh`: PASS;
- `actionlint .github/workflows/minco-manual.yml .github/workflows/publish-crates.yml`: PASS;
- static validation: PASS, 0 errors and 0 warnings;
- publish validation: PASS, 0 errors and 0 warnings;
- source manifest: PASS for 1,095 files;
- `scripts/ci/local-runtime.sh`: PASS for owned PostgreSQL readiness/restart
  and Rustack STS lifecycle;
- `scripts/dev/rustack-smoke.sh`: PASS for S3, SQS, SSM and STS plus the Minco
  SSM adapter;
- `scripts/test/e2e.sh`: PASS for Orders E2E;
- `scripts/aws/plan.sh`: PASS;
- `scripts/aws/validate.sh`: PASS, including rehearsal regression checks,
  static validation and SAM lint;
- `scripts/aws/build-lambda.sh`: PASS, 5,116,543-byte ZIP;
- `scripts/aws/build-worker-lambda.sh`: PASS, 576,357-byte ZIP.

The first validator run exposed an interactive cleanup prompt for a
write-protected Git object under a `mktemp`-owned fixture. Changing that exact
fixture cleanup from `rm -r` to `rm -rf` removed the prompt; the focused phase
result regression and complete validator then passed. Full
`scripts/ci/local-release.sh` and `scripts/quality.sh` are **NOT RUN** because
the user explicitly prohibited repository-wide formatting/linting. No formatter
was run. No hosted workflow was dispatched, no public AWS endpoint was used,
and no publication, deployment or promotion claim is made.
