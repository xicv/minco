---
id: M11-T04
title: Add plugin add init explain test and remove workflows
milestone: M11
status: complete
priority: high
area: plugins/developer-experience
depends_on: [M11-T03]
operations: []
owned_paths:
  - crates/minco-cli/**
  - crates/minco-core/**
  - plugins/catalog.toml
  - docs/architecture/plugin-authoring.md
  - docs/how-to/**
  - docs/reference/**
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - tasks/M11/M11-T04-plugin-workflows.md
checks:
  - cargo test -p cargo-minco -p minco-core --all-features --locked
  - cargo clippy -p cargo-minco -p minco-core --all-targets --all-features --locked -- -D warnings
  - cargo minco plugin doctor
  - cargo minco plugin add minco-plugin-health --dry-run --json
---

## Goal

Plan deterministic Cargo, catalog, config, and composition-root changes for
plugin add/init/explain/test/remove/doctor while keeping code registration
explicit.

## Acceptance

- every mutating workflow supports dry-run and JSON;
- compatible explicit Cargo versions are resolved before edits;
- Rust/TOML edits fail before overwrite or ambiguity;
- explain shows capabilities, dependencies, resources, cost, config, and
  conformance evidence;
- remove reports application operations/data/migrations that prevent safe
  removal.

## Non-goals

- downloading or executing plugins dynamically at runtime;
- automatic source scanning for constructors;
- treating catalog metadata as executable discovery.

## Implementation

- `cargo minco plugin` now exposes deterministic `add`, `init`, `explain`,
  targeted `test`, `remove`, and `doctor` workflows alongside dry-run forms of
  `new`, `enable`, and `disable`. JSON plans contain semantic paths/actions and
  registration evidence, never rewritten file contents or secret values.
- Official add/remove edits remain limited to reviewed Minco facade features
  and manifest selection. App-owned constructors remain explicit Rust code;
  selection reports them as unverified and Doctor fails closed until the
  application supplies separate composition evidence.
- Local package adoption validates normalized in-project paths, regular bounded
  distribution records, package inclusion, exact Cargo versions, current-core
  compatibility, catalog drift, and the version-matched CLI before any write.
- Removal blocks on traced operations, enabled dependents, migrations, seeds,
  data classes, declared resources, or unavailable/invalid distribution
  metadata. It does not treat source or feature deletion as data or
  infrastructure teardown evidence.
- Every multi-file edit captures its input bytes, rejects symlinked parents and
  ambiguous Cargo declarations, preflights the complete batch for concurrent
  changes, and writes only after the full preflight succeeds.
- The authoring, management, conformance, distribution, testing, and CLI
  documentation now distinguishes local metadata, static registration,
  application tests, provider checks, deployment, and production readiness.

## Local evidence

- `cargo test -p cargo-minco -p minco-core --all-features --locked` passed 64
  CLI unit tests, 20 plugin CLI journeys, all other CLI integration suites, 38
  core tests, and doctests. Review regressions cover version mismatch,
  contradictory selection, missing Cargo features, symlinked parents,
  unverified constructors, resource ownership, and fail-before-write removal.
- `cargo clippy -p cargo-minco -p minco-core --all-targets --all-features
  --locked -- -D warnings` and `cargo fmt --all -- --check` passed.
- `cargo minco plugin doctor --json` returned `passed` for catalog,
  distribution, selection, exact version, active Cargo feature, and static
  composition checks.
- `cargo minco plugin add minco-plugin-health --dry-run --json` resolved Minco
  `0.6.0`, verified facade registration, returned an empty idempotent change
  list in the framework workspace, and wrote nothing.
- `cargo minco plugin remove feedback --dry-run --json` returned `safe: false`
  with ordered operation, migration, and data-class blockers and wrote nothing.
- `./scripts/quality.sh` passed static/publish/deep-review checks, 40 feedback
  browser journeys, 112 documentation snippets, the VitePress build and links,
  13 applicable documentation browser journeys, all-feature workspace
  Clippy/tests, generated PostgreSQL/SQLite consumers, Rustdoc, dependency and
  license policy, the advisory audit, npm audit, secret scanning, and the final
  source-manifest check.
- Bounded real-AWS, Rustack, configured PostgreSQL, deployment, registry,
  hosted, and production checks were not run and are not implied by this local
  workflow task.
