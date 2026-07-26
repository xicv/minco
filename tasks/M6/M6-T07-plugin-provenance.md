---
id: M6-T07
title: Add bounded provenance to typed plugin registrations
milestone: M6
status: complete
priority: high
area: core/plugins
depends_on: [M6-T06]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - README.md
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - VERIFICATION.md
  - roadmap/roadmap.yaml
  - scripts/test/repository_truth.py
  - verification/**
  - crates/minco/**
  - crates/minco-core/**
  - crates/minco-cli/**
  - docs/DECISIONS.md
  - docs/adoption/incremental-adoption.md
  - docs/architecture/extensions.md
  - docs/architecture/plugin-authoring.md
  - docs/adrs/**
  - docs/reference/cli.md
  - tasks/M6/M6-T07-plugin-provenance.md
checks:
  - cargo test -p minco-core -p cargo-minco --all-features --locked
  - cargo clippy -p minco-core -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - cargo minco inspect --json
---

## Goal

Make singleton-service and ordered-contribution ownership inspectable without
changing Minco's typed registries into string-key lookup or a global service
locator.

## Design boundary

- preserve `TypeId`-based retrieval and static plugin composition;
- add plugin-context registration helpers that attach the current `PluginId`;
- distinguish application-provided registrations from plugin-provided ones;
- include both first and attempted owners in duplicate-service diagnostics;
- preserve deterministic contribution order and expose bounded type/owner/index
  summaries through inspection;
- do not expose `Any` values, service instances, configuration values, secrets,
  or provider diagnostics;
- keep existing low-level registry APIs only where an explicit
  application-owned provenance default is unambiguous.

## Acceptance

- two plugins attempting the same singleton produce a deterministic diagnostic
  naming both owners and the Rust type;
- contribution summaries retain plugin owner and installation index;
- application seed services/contributions are distinguishable from plugins;
- composition, graph planning, no-feature facade, official plugins and
  third-party plugin examples remain source-compatible where practical;
- an ADR records any unavoidable pre-1.0 public API break;
- tests prove ownership cannot be spoofed through `PluginContext`.

## Evidence

Base Git SHA:
`c5b7749cec295fddd795827733e2889d6f1f896b`.

Implemented:

- opaque application/plugin owners created only by the composition boundary;
- owner-bound service and contribution registrar views;
- structured duplicate diagnostics containing Rust type, first owner and
  attempted owner;
- metadata-only frozen summaries with deterministic type grouping and global
  contribution installation indices;
- `ComposedApplication::registration_provenance()` and bounded
  `cargo minco inspect --json` output;
- ADR and pre-1.0 compatibility notes.

Review-time registry validation found all 24 lock-step `0.2.0` packages already
published from exact tag `v0.2.0`. The workspace and internal dependency
requirements therefore advance to the unpublished `0.3.0` candidate; placing
these public API changes in a `0.2.x` patch would incorrectly advertise Cargo
compatibility with the immutable `0.2.0` archives.

Focused and workspace Rust gates pass, including 37 `minco-core` tests and 15
`cargo-minco` tests. The first focused strict-Clippy run failed because two
manual registry `Debug` implementations omitted newly added fields; they now
emit counts/next-index metadata only, and the exact Clippy gate passes. Native
ARM64 Orders and SQS-worker builds pass; exact refreshed artifact measurements
are recorded in `verification/adoption-measurements.json`.

The authoritative `./scripts/quality.sh` gate, bounded inspection assertion,
official-plugin validation, generated PostgreSQL and SQLite consumers,
24-package inventory and 24-package publication dry run pass. The publication
driver ran without `--execute`; Cargo verified every package tarball and
aborted every upload because of `--dry-run`. The reverse-apply whitespace
check, source-manifest check and JJ conflict query also pass.

The first publication dry run reached all 24 package archives, then failed
while verifying `minco-http` because the isolated Cargo target exhausted the
local disk. Clearing only generated target outputs and retrying the unchanged
clean JJ source produced a complete pass. No registry, tag, deployment,
database or product repository was modified.

## Reason for separate task

M6-T06 found the current failure mode is safe—duplicates fail—but not
diagnostic enough. Retrofitting provenance touches every plugin registration
site and inspection schema. Keeping it out of the HTTP/worker P0 slice avoids
destabilizing those release gates while making the migration prerequisite and
acceptance boundary explicit.
