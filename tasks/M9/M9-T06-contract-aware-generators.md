---
id: M9-T06
title: Add contract-aware generators and customizable stubs
milestone: M9
status: complete
priority: high
area: developer-experience/generation
depends_on: [M9-T05]
operations: []
owned_paths:
  - crates/minco-cli/src/**
  - crates/minco-cli/templates/**
  - scripts/test/scaffold_templates.py
  - scripts/test/generated_apps.sh
  - docs/development/**
  - docs/reference/**
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/publish-validation.json
  - verification/source-manifest.json
  - verification/static-validation.json
  - tasks/M9/M9-T06-contract-aware-generators.md
checks:
  - cargo test -p cargo-minco --all-features --locked
  - cargo clippy -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - uv run --locked python scripts/test/scaffold_templates.py
  - scripts/test/generated_apps.sh
---

## Goal

Add module, operation, migration, seeder, worker, adapter, test, plugin, and
app-owned stub generators that plan deterministic Rust/TOML/YAML-aware edits
and preserve OpenAPI authority.

## Acceptance

- every command supports dry-run and JSON change plans;
- an operation requires an existing operation ID unless a separately reviewed
  contract-stub mode is selected;
- generated application and HTTP tests fail until business behavior exists;
- no command overwrites unreviewed files or generates a fake success result;
- generated PostgreSQL and SQLite applications compile and test.

## Non-goals

- source scanning as registration;
- generating product domain rules;
- making generated files opaque framework runtime state.

## Review corrections

- The provisional tracer command was `generate operation`; the approved
  framework-completion contract is the coherent `cargo minco make ...` family.
  The public CLI now follows that contract. Contract-stub authoring remains
  absent because its required path, method, security, success, Problem,
  examples, and idempotency semantics need a separate contract review.
- The original task ownership omitted the deterministic verification reports
  and source/adoption identity refreshed by the required repository quality
  gate. Those bounded evidence paths are now explicit.
- Generated module files are made compiler-visible through boundary tests, and
  generated worker filenames exactly match their registered kebab-case binary
  names.
- The final review found that apply originally reported its plan only after
  writing and used a replacing rename for creates. Apply now flushes a redacted
  pre-write plan and installs creates with an atomic no-clobber hard link before
  any reviewed inventory update.
- Contract paths are no longer inserted directly into Rust format strings.
  Rust HTTP stubs receive a quoted, escaped path literal, so ordinary
  parameterized paths such as `/widgets/{id}` remain valid generated Rust.

## Evidence

Completed on 2026-07-28 in the isolated `minco-task-m9-t06` JJ workspace
against merged-main parent `76efacc2ed48c7544627fed1ea233b2725583ed8`.

- TDD began with the public `make operation` command missing. Focused
  integration tests now prove deterministic no-write JSON dry runs, existing
  OpenAPI operation selection, unknown-operation rejection, explicit failing
  application/HTTP specifications, no-overwrite behavior, app-owned stub
  customization, strict names, and symlink containment. A planner unit test
  proves an input changed after planning prevents every create.
- `cargo test -p cargo-minco --all-features --locked` and
  `cargo clippy -p cargo-minco --all-targets --all-features --locked -- -D
  warnings` passed. The CLI suite includes 36 binary unit tests, six generator
  integration tests, the development-plan test, and four seed CLI tests.
- `uv run --locked python scripts/test/scaffold_templates.py` passed both
  application profiles and all 20 publishable generator stubs with no
  unresolved placeholders.
- `scripts/test/generated_apps.sh` passed clean PostgreSQL and SQLite scaffold
  compile/test journeys. After applying stubs, module, migration, seeder,
  worker, adapter, operation, and plugin generators, both workspaces still
  compiled all targets; the edited migration and seed catalogs planned
  successfully; and `--no-fail-fast` test runs reached both explicit
  `getPlatform` application and HTTP TODO failures.
- Plans contain only ordered paths/actions/formats and reviewed contract
  metadata, never generated source, configuration values, database URLs, or
  secrets. Apply preflights all edits, prints the plan before writing, installs
  creates without clobbering race-created files before inventory updates, and
  rolls installed changes back on a later installation failure.
  No crate, release, deployment, database, or cloud resource was published or
  mutated.
