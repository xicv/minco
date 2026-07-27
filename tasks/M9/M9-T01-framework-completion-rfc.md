---
id: M9-T01
title: Define the framework-completion golden path and documentation map
milestone: M9
status: complete
priority: critical
area: architecture/roadmap
depends_on: [M6-T09]
operations: []
owned_paths:
  - README.md
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - VERIFICATION.md
  - docs/DECISIONS.md
  - docs/adrs/0018-framework-golden-path.md
  - docs/vision/**
  - docs/roadmap/**
  - roadmap/**
  - tasks/M7/M7-T01-two-app-validation.md
  - tasks/M8/M8-T03-first-crates-io-release.md
  - tasks/M9/**
  - tasks/M10/**
  - tasks/M11/**
  - tasks/M12/**
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/test/repository_truth.py
  - uv run --locked python scripts/validate_publish.py
  - uv run --locked python scripts/deep_review.py
  - cargo minco roadmap status
  - cargo minco task graph --output roadmap/tasks.mmd
  - cargo minco check
  - uv run --locked python scripts/source_manifest.py --check
  - git diff --check
  - jj log -r 'conflicts()'
---

## Goal

Define Minco's product identity, five-plane application graph, developer and
deployment golden paths, 1.0 completion criteria, explicit non-goals,
documentation information architecture, compatibility boundaries, and bounded
M9-M12 task sequence before further runtime implementation.

## Acceptance

- the decision is repository-native and linked from the decision register;
- current maturity and proven documentation drift are recorded accurately;
- M6-T10 remains the next independent runtime task and likely `0.4.0` boundary;
- M9-M12 milestones and dependency-valid task records exist;
- Diátaxis entry paths cover developers, plugin authors, operators,
  contributors, and AI coding agents;
- generated roadmap, task graph, and source manifest are current;
- the PR contains no runtime implementation or external mutation.

## Non-goals

- implementing M6-T10 or another lifecycle task;
- moving the entire documentation tree;
- publishing crates, creating a release tag, or mutating AWS/databases;
- modifying CGSP, GarmentIQ, or any product repository.

## Evidence

Qualified on 2026-07-27 from remote `main`
`cb6ffd702a65a59a3195caa64c3709a471b4c21f` in JJ workspace
`/Users/xicao/Projects/minco-task-m9-t01` (change `wqrlkrrx`).

| Check | Result |
|---|---|
| `./scripts/quality.sh` | PASS — compiler, Clippy, workspace tests, generated consumers, docs, dependency/security, and secret checks |
| `uv run --locked python scripts/validate_static.py` | PASS — 13 milestones, 56 tasks, 0 errors, 0 warnings |
| `uv run --locked python scripts/test/repository_truth.py` | PASS — 12 tests |
| `uv run --locked python scripts/validate_publish.py` | PASS — 29 workspace packages, 24 publishable, 5 private |
| `uv run --locked python scripts/deep_review.py` | PASS — status `ok`; pre-existing source warnings retained |
| `cargo minco roadmap status` | PASS — dependency-valid M0-M12 roadmap |
| `cargo minco roadmap render --format mermaid --output roadmap/roadmap.mmd` | PASS |
| `cargo minco task graph --output roadmap/tasks.mmd` | PASS |
| `cargo minco check` | PASS |
| `uv run --locked python scripts/source_manifest.py --check` | PASS |
| `git diff --check origin/main...task/framework-completion-rfc` | PASS |
| `jj log -r 'conflicts()'` | PASS — no conflicts |

The source manifest and adoption-report revision were regenerated together.
Existing dependency/build/artifact observations were carried forward because
this task changes no Cargo manifest, lockfile, Rust source, Plan IR, SAM
template, or recorded artifact; that limitation is explicit in the report.

No AWS, database, crate registry, release tag, product repository, or secret
boundary was mutated.
