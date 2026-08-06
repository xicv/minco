---
id: M14-T05
title: Resume partial Minco 1.1.0 crate publication safely
milestone: M14
status: complete
priority: critical
area: release/1.1
depends_on: [M14-T01, M14-T04]
operations: []
owned_paths:
  - .github/workflows/publish-crates.yml
  - docs/reference/generated/diagnostics.md
  - scripts/test/publish_validation.py
  - roadmap/tasks.mmd
  - tasks/M14/M14-T02-promote-1-1-publication.md
  - tasks/M14/M14-T05-resume-partial-crate-publication.md
  - verification/**
checks:
  - uv run --locked python -m py_compile scripts/test/publish_validation.py
  - uv run --locked python scripts/test/publish_validation.py
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/source_manifest.py --check
---

## Goal

Resume an interrupted trusted publication from the immutable `v1.1.0` tag
without moving the tag or attempting to upload versions already accepted by
crates.io.

## Acceptance

- the standard full-family path remains the default when no recovery package
  list is supplied;
- recovery package names enter the shell through an environment value, are
  syntax checked, remain individually quoted and are revalidated against the
  tag's declared release-package allowlist;
- a registry preflight requires the selected set to equal the 28 absent exact
  versions and independently requires every unselected exact version to be
  present and non-yanked;
- the workflow still checks out and verifies the immutable release tag, runs
  the complete release gate, and obtains a short-lived OIDC token before a
  selected upload;
- exact registry reconciliation determines the recovery package list instead
  of assuming that a failed multi-package command was atomic; and
- publication truth is promoted only after all 33 exact versions are present
  and non-yanked.

## Non-goals

- moving or recreating `v1.1.0`;
- changing any crate archive or workspace version;
- silently retrying every package after a partial upload; or
- weakening compiler, test, documentation, dry-run or trusted-publishing
  gates.

## Evidence

The first tag-bound publication run, GitHub Actions run `31068913557`, passed
tag verification, static/package checks, formatting-as-check, facade, Clippy,
workspace tests, generated-application smoke, documentation and the complete
33-crate dry run. The OIDC upload then accepted `minco-contract`, `minco-core`,
`minco-db`, `minco-dev` and `minco-release` at `1.1.0` before crates.io rejected
`minco-aws-dynamodb` with HTTP 403: its trusted-publisher configuration was
missing. Registry validation independently confirmed exactly five published
and 28 absent versions.

Authenticated read-only reconciliation found the established
`xicv/minco` / `publish-crates.yml` / `crates-io` trusted-publisher contract on
28 crates and no configuration on `minco-aws-dynamodb`, `minco-mcp`,
`minco-plugin-realtime`, `minco-project-view` or `minco-workbench`. The official
crates.io API created only those five missing configurations as IDs
`15500`-`15504`; no credential value was printed or written to the repository.

The existing tag script already accepts repeatable allowlisted `--package`
arguments. This task exposes that capability through a quoted, syntax-checked
workflow input and adds a registry-complement preflight, so the same immutable
tag can publish only registry-absent packages with a new short-lived OIDC
token.

Local recovery-control evidence on 2026-08-06:

- the modified Python policy file compiled and its complete release fixtures
  passed;
- the exact registry-complement step verified five present and 28 absent
  `1.1.0` packages, while a shell-injection-shaped package name was rejected
  before the publisher was invoked;
- static validation completed with zero errors and zero warnings, and canonical
  deep-review/source-manifest evidence was regenerated;
- task inspection parsed `M14-T05` and the ancestor conflict query was empty;
  and
- `uv run --locked ruff check scripts/test/publish_validation.py` could not run
  because the locked environment has no `ruff` executable (`Failed to spawn:
  ruff`); no unpinned formatter or linter was installed, and no formatting
  command was run.

The first task-finish gate stopped because the added policy assertions moved
diagnostic fixture line references and made
`docs/reference/generated/diagnostics.md` stale. The deterministic reference
generator changed only those expected line numbers; the generated path is now
owned by this task and will be rechecked before push.
