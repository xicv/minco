---
id: M11-T10
title: Define the repository-native project view
milestone: M11
status: complete
priority: high
area: documentation/ai-workbench
depends_on: [M11-T06]
operations: []
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0030-repository-native-project-view.md
  - docs/roadmap/framework-completion.md
  - roadmap/tasks.mmd
  - tasks/M11/M11-T10-project-view-design.md
  - tasks/M12/M12-T01-local-read-only-mcp.md
  - tasks/M12/M12-T02-local-workbench.md
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - uv run --locked python scripts/test/repository_truth.py
  - uv run --locked python scripts/validate_static.py
  - cargo minco roadmap status --json
  - cargo minco task graph --output roadmap/tasks.mmd
  - ./scripts/quality.sh
---

## Goal

Define the bounded, repository-native read model that future local MCP and
workbench tasks will use to explain a Minco application, visualize its graphs
and progress, and present independently qualified evidence without creating a
second project state machine.

## Acceptance

- one accepted ADR owns the source-authority, schema, status, evidence,
  projection, security and compatibility boundaries;
- the already-planned `cargo minco workbench` name remains the presentation
  surface instead of adding a competing product-specific command;
- M12-T01 owns the shared schema and bounded readers before M12-T02 consumes
  them through the CLI and local workbench;
- raw application statuses remain visible while optional semantic classes are
  explicit project-owned mappings;
- source, local verification, hosted verification, deployment, runtime and
  review/UAT evidence remain separate and cannot imply one another;
- application-specific feature meaning stays in the application repository;
- Minco and one first-party application must prove any public adapter boundary
  before it is frozen;
- narration is an accessible text projection, not an audio provider or hidden
  content-generation service;
- the local server design rejects DNS-rebinding and cross-origin access, keeps
  browser assets local and prevents repository text from becoming executable
  markup;
- the initial MCP transport opens no listener, and static export is create-only
  outside canonical inputs, rejects symlinks throughout the destination path
  and retains verified parent identity through atomic no-clobber publication.

## Non-goals

- implementing the MCP server, project-view crate, workbench or static assets;
- changing a public Rust API, serialized runtime schema or current CLI output;
- modifying CGSP or another application repository;
- advancing M12 task or milestone status before its repository prerequisites;
- contacting AWS, deploying a site, publishing crates or enabling telemetry.

## Review corrections

- Post-merge review found that the two M12 implementation tasks introduced new
  workspace crates without owning the explicit root workspace manifests or the
  deterministic verification files refreshed by complete quality. Both tasks
  now own those bounded paths so their locked checks can be completed without
  crossing task ownership.
- The original export contract rejected only a final symlink destination even
  though that destination must not exist. It now rejects symlinks in every
  existing component, proves the destination parent remains beneath the
  canonical project root and outside canonical inputs, retains parent identity
  during staging, exclusively creates and retains a private staging directory
  instead of adopting an existing entry, and requires atomic no-clobber
  installation.

## Evidence

The post-merge review correction was completed locally on 2026-08-04 in the
same isolated workspace as a fresh child of PR #96 merge commit
`5aa32b8130518accfd1295f87a4bd9f6f5fc2142`, then rebased onto exact PR #97
merge commit `0a16f435e2fdca12c90bee35b0610d9eb1a303f1`:

- both M12 task records now include the explicit Cargo workspace manifests,
  generated task graph and deterministic qualification reports needed by their
  locked checks and the repository-wide completion gate;
- ADR-0030 and M12-T02 now require component-by-component no-follow path
  validation, stable parent and staging identities, exclusive private staging,
  atomic no-clobber installation and negative tests for symlink, staging-entry,
  race, overlap and platform-safety cases;
- complete `./scripts/quality.sh` passed on the corrected tree, including the
  terminal source-manifest verification; and
- no hosted workflow, provider, database, deployment, release, tag, registry or
  documentation-site mutation was performed.

Completed locally on 2026-08-04 in the isolated `minco-task-m11-t10` JJ
workspace. The change started from `main@origin`
`94402fc4f996c65e93927e584576fcbd546d02fa`; while qualification was running,
PR #95 advanced `main` and JJ cleanly rebased the final tree onto exact commit
`344d78d318e68c0de05483a77c385cc5bddce3b6`:

- ADR-0030 defines one schema-versioned `ProjectView`, retains raw statuses,
  makes semantic mappings explicit, and keeps source, local, hosted,
  deployment, runtime and review evidence independent;
- the design keeps application feature meaning and release/UAT policy outside
  Minco, requires two first-party consumers before freezing an adapter, and
  rejects arbitrary file discovery, prose-table parsing, shell execution,
  hosted telemetry and write capabilities;
- M12-T01 now owns the shared bounded read model before M12-T02 renders it;
  both tasks and the M12 milestone remain `planned`;
- the planned CLI stays under `cargo minco workbench`, with read-only `--check`,
  deterministic explicit export and loopback-only serve boundaries;
- narration is accessible displayed text or explicit client-side speech, not a
  provider call, generated audio asset or separate source of project truth;
- all 37 repository-truth tests passed, static validation reported zero errors
  and warnings across 70 tasks, and roadmap status plus the regenerated task
  graph passed;
- complete local quality passed repository truth, generated reference,
  publish/release policy, static/deep review, generated PostgreSQL and SQLite
  applications, all workspace compiler/Clippy/test/Rustdoc profiles, 40
  Feedback browser tests, 181 documentation snippets, the VitePress build and
  links, and 15 applicable documentation browser journeys;
- the first complete quality run reached the final source-integrity gate after
  all earlier checks, then correctly rejected the stale source manifest caused
  by this task; the manifest was regenerated and the complete gate rerun on the
  final tree;
- existing deep-review warnings in untouched Rust and SQL sources remain
  unchanged. Provider-backed AWS, Rustack and unconfigured PostgreSQL tests
  retain their explicit ignored/not-run state.

No CGSP file, AWS resource, environment, database, deployment, release, tag,
registry entry or documentation site was created, modified or published.
Hosted exact-head qualification, pull-request review and merge remain separate
evidence.
