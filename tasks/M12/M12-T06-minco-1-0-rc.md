---
id: M12-T06
title: Prepare the Minco 1.0 release candidate
milestone: M12
status: complete
priority: critical
area: release/1.0
depends_on: [M8-T03, M12-T02, M12-T05]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - README.md
  - PUBLISHING.md
  - CHANGELOG.md
  - VERIFICATION.md
  - CODEX_HANDOFF.md
  - docs-site/release.json
  - docs/**
  - plugins/*/minco-plugin.json
  - extensions/*/minco-plugin.json
  - examples/plugins/third-party-minimal/**
  - crates/minco-cli/tests/plugin_cli.rs
  - crates/minco-test/tests/plugin_conformance.rs
  - examples/orders/api/src/generated.rs
  - infra/aws/generated/plan.json
  - roadmap/roadmap.yaml
  - scripts/test/repository_truth.py
  - tasks/M12/M12-T06-minco-1-0-rc.md
  - verification/**
checks:
  - ./scripts/quality.sh
  - scripts/release/package-list.sh
  - scripts/release/publish.sh --skip-quality
  - cargo install cargo-minco --path crates/minco-cli --locked
  - uv run --locked python scripts/source_manifest.py --check
---

## Goal

Prepare an exact, reviewable 1.0 release candidate after all completion,
adoption, compatibility, ownership, and qualification gates pass.

## Acceptance

- workspace version, lock-step internal dependencies, changelog, migration
  guides, docs, source manifest, package inventory, and candidate evidence
  agree;
- a fresh external generated application and facade consumer compile and test;
- the candidate source, package archives, docs, and artifact digests are exact;
- hosted exact-head qualification is ready for a separately authorised tag and
  publication task;
- no release claim is made before registry and tag actions actually occur.

## Non-goals

- uploading crates or creating the final tag without explicit authority;
- bypassing a blocked ownership, security, provider, or documentation gate;
- calling an RC a production release.

## Evidence

The complete workspace and all lock-step internal dependency requirements are
`1.0.0`; Cargo metadata resolves all 37 local workspace packages at that
version and identifies exactly 32 publishable packages. All archive-visible
official plugin core requirements accept `^1.0.0`. Component contract versions
remain independently versioned where their linked descriptors require it.

The candidate has a dated substantive changelog record, a direct
`0.6.0`-to-`1.0.0` upgrade guide, the two detailed intermediate guides, current
release metadata, generated package/plugin reference, OpenAPI Rust binding and
provider-free Plan. Static release truth, contract drift, plugin validation and
reference drift checks pass.

Focused version-boundary verification passed 20 Cargo plugin CLI tests, 17
public plugin-conformance tests and the standalone third-party plugin package.
The source manifest and adoption report record the exact final source and local
arm64 artifact digests. Default and no-default facade dependency counts remain
unchanged from the published baseline, while the Orders artifact remains below
the reviewed native ZIP budget.

The schema-closed candidate qualification record under `verification/` binds
the final source identity and all ten mandatory commands. It covers the full
quality/security matrix, desktop and small-screen documentation journeys,
generated PostgreSQL and SQLite applications, Rustack and HTTP E2E, all 32
package archives, configured unpacked-package tests, four external facade
consumers, an unpacked `cargo-minco` installation, restore/rollback and bounded
API/worker load. No exact-current AWS call was made; historical M10 provider
proof remains bound to its original revisions.

No tag, crate upload, GitHub release, documentation publication, merge, push,
deployment, promotion or application adoption was performed. Hosted exact-head
qualification and every irreversible release action require separate explicit
authority.

The first clean truth run was retained as failed red evidence. Closing M12
revealed that `test_planned_milestone_rejects_ready_task_evidence` still tried
to rewrite M12 from `active` and M12-T01 from `in_progress`; both were now
`complete`, so the fixture made no mutation and failed to prove its intended
diagnostic. The fixture now changes the actual completed states to `planned`
and `ready`, and all 40 repository-truth tests pass. The following standalone
Feedback browser gate also failed in that red run because the early quality
exit occurred before its canonical browser setup installed local dependencies;
it was not accepted as independent browser evidence. The final full runner
starts from a fresh clean workspace and must pass both gates.

A later clean quality run reached its final manifest check with compiler,
generated-application, browser and security checks green, then rejected the
source because the canonical publish-validation report still identified the
pre-bump `0.7.0` workspace. The report is now regenerated as `1.0.0` before the
source manifest is frozen. That failed truth attempt was stopped and was not
converted into accepted evidence.

## Post-candidate inventory note

M12-T06 qualified the exact 32-package source tree described above. The later
M6-T01 merge added `minco-aws-dynamodb` as package 33. M12-T07 owns the new
exact-source qualification, versioned documentation freeze, AppSync proof lock,
modern MCP regression, first-publication preflight and release evidence; this
historical task does not retroactively claim those later bytes.
