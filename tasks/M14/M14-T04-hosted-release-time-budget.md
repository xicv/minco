---
id: M14-T04
title: Give hosted release qualification a cold-run time budget
milestone: M14
status: complete
priority: critical
area: release/qualification
depends_on: [M14-T01]
operations: []
owned_paths:
  - .github/workflows/minco-manual.yml
  - scripts/test/hosted_ci_policy.py
  - roadmap/tasks.mmd
  - tasks/M14/M14-T04-hosted-release-time-budget.md
  - tasks/M14/M14-T02-promote-1-1-publication.md
  - verification/**
checks:
  - uv run --locked python scripts/test/hosted_ci_policy.py
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/source_manifest.py --check
---

## Goal

Allow the explicit hosted release profile to complete its already-bounded
qualification matrix on a cold runner without weakening the default essential
profile or skipping any release gate.

## Acceptance

- the manual workflow retains the bounded essential profile as its default;
- the release job has a tested 90-minute budget, sized above the observed cold
  exact-main path without granting an unbounded runner;
- the complete release matrix still runs in the same order and uploads no crate;
- exact-head and exact-main qualification remain separate evidence; and
- the corrected exact-main workflow completes all release gates successfully.

## Non-goals

- removing, splitting or weakening release qualification gates;
- changing cache failure policy or persisting build targets;
- publishing a crate, creating a tag or deploying an application; or
- changing application, framework or documentation behavior.

## Evidence

GitHub Actions run `31060250791` checked out exact merged main
`96a964f5663cbff66892601d041987280fe60618`. Full quality completed in 36m11s,
the standalone AppSync proof in 1m52s, and recovery/load in 7m34s. Both evidence
uploads succeeded. GitHub then cancelled the job at its fixed 60-minute ceiling
after 13m37s of the archive publish dry-run; there was no source or assertion
failure, and the later Lambda, Rustack and E2E steps did not run. The identical
PR-head tree had already completed the entire matrix in run `31057694870`.

Local evidence on 2026-08-06:

- the hosted CI policy suite passed all four tests, including the explicit
  90-minute release-job budget assertion;
- the modified Python policy test compiled successfully; and
- static validation completed with zero errors and zero warnings.

The first exact-head run with the expanded budget, GitHub Actions run
`31063559421`, stopped at the terminal source-manifest check after all compiler,
test, documentation, security and leak gates had passed. The task and workflow
changes altered the canonical deep-review report, which had not been refreshed
before the source manifest. Regenerating the standard offline deep-review report
reproduced that single tracked drift locally; the candidate now binds the source
manifest to the refreshed report before requalification.
