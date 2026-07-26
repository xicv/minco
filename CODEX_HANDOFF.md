# Minco plugin-registration provenance handoff

Date: 2026-07-26
Task: `M6-T07`
Published baseline: `0.2.0`
Workspace candidate: `0.3.0`
Base Git SHA: `c5b7749cec295fddd795827733e2889d6f1f896b`

## Objective

Make typed singleton-service and ordered-contribution ownership diagnosable
before the formal GarmentIQ/CGSP adoption pilot, without adding a service
locator, serializing values, or changing runtime provider ownership.

## Design

- Direct `ServiceCollection`/`ContributionCollection` registrations are
  application-owned.
- `PluginContext` and `PluginFinalizeContext` return owner-bound registrars.
  The opaque plugin owner is created from the effective descriptor by
  `PluginManager`; plugins cannot pass or forge an owner.
- Duplicate singletons retain the first value and report Rust type, first
  owner and attempted owner.
- Contributions retain a global deterministic installation index and are
  summarized by Rust type in deterministic order.
- Provenance is available only after successful composition through frozen
  registries and `ComposedApplication::registration_provenance()`.
- `cargo minco inspect --json` emits types, owners and contribution indices
  only. Service values, configuration, URLs, credentials and provider
  diagnostics are not serialized.

## Compatibility boundary

Normal chained plugin registrations remain source-compatible. The context
accessors now return `ServiceRegistrar`/`ContributionRegistrar`, so third-party
code that explicitly annotated the old mutable collection reference must adapt.
`ServiceError::Duplicate` retains its variant name but now carries
`DuplicateServiceRegistration`. ADR 0017 records the pre-1.0 impact.

The review found that all 24 `0.2.0` packages had already been accepted by
crates.io from tag `v0.2.0`. Because Cargo treats `0.2.x` releases as one
compatible line, this public API change advances the workspace candidate to
`0.3.0`; it does not attempt to overwrite the immutable `0.2.0` archives.

## Validation boundary

Passed on the completed task source:

- focused `minco-core` and `cargo-minco` checks, strict Clippy and tests;
- workspace all-target/all-feature check, strict Clippy, tests and Rustdoc;
- bounded JSON inspection;
- native ARM64 Orders Lambda and SQS worker builds;
- authoritative `./scripts/quality.sh`, including generated PostgreSQL and
  SQLite consumers, dependency/advisory/license checks and Gitleaks;
- `cargo minco plugin validate`, the 24-package inventory and the complete
  24-package publication dry run;
- reverse-apply whitespace, source-manifest and JJ conflict checks.

The first publication dry run failed during packaged `minco-http` verification
with `No space left on device`. It had already packaged all 24 crates. Only the
isolated workspace's generated Cargo target was cleared; retrying the unchanged
clean source passed and Cargo aborted every upload because of `--dry-run`.

No AWS resource, database, product repository, crate registry, release tag or
deployment is modified by this task. The publication command remains dry-run
only.

## Next boundary

`M7-T01` remains planned and depends on this task. The next migration-program
workstream is the bounded CGSP adoption plan, but product work must wait for the
draft M6-T07 PR to be reviewed and merged unless an exact unmerged dependency
is explicitly authorized.
