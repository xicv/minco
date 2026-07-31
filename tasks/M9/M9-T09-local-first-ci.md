---
id: M9-T09
title: Make local quality authoritative and hosted CI essential-only
milestone: M9
status: ready
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
