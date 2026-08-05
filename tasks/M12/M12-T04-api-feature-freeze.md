---
id: M12-T04
title: Audit and freeze public APIs and Cargo features
milestone: M12
status: complete
priority: critical
area: compatibility/release
depends_on: [M11-T06, M12-T03]
operations: []
owned_paths:
  - Cargo.toml
  - crates/**
  - plugins/**
  - extensions/**
  - docs/adrs/**
  - docs/adoption/**
  - docs/reference/**
  - CHANGELOG.md
  - tasks/M12/M12-T04-api-feature-freeze.md
checks:
  - cargo semver-checks --workspace --all-features
  - cargo test --workspace --all-targets --all-features --locked
  - cargo doc --workspace --all-features --no-deps --locked
  - scripts/docs/generate-reference.sh --check
---

## Goal

Review and explicitly freeze the 1.0 Rust API, Cargo feature matrix, CLI
surface, configuration schema, Plan IR, release/deployment receipts, plugin
distribution contract, diagnostics, MSRV, and compatibility policy.

## Acceptance

- public types and features have evidence-backed stability classifications;
- default and opt-in dependency surfaces are measured;
- deprecated pre-1.0 paths have migration guidance;
- unsupported promises are removed before the freeze;
- the compatibility policy states how Rust, CLI, serialized, and behavioral
  changes are versioned after 1.0.

## Non-goals

- preserving accidental undocumented internals;
- raising the MSRV without an explicit compatibility decision;
- freezing known unsafe or unverified behavior merely to meet a date.

## Freeze result

- `docs/reference/compatibility.md` freezes every rustdoc-visible public item
  in the 32 publishable packages, 28 facade features plus the three
  package-specific feature sets, 101 CLI command paths, configuration schema
  1, Plan schemas 1 and 2, release manifest schema 3, schema-1 deployment,
  plugin-distribution and project-view records, diagnostic codes, behavioral
  safety boundaries and Rust `1.97.1` as the MSRV.
- Catalog stability is now explicitly separate from compatibility. A beta
  plugin, adapter or runtime remains opt-in and evidence-bounded, but its
  published 1.x Rust, feature, CLI and serialized boundaries follow SemVer.
- The post-1.0 policy classifies patch, minor and major changes across Rust,
  Cargo features/defaults, CLI, strict/digest-bound schemas, diagnostics,
  behavior and MSRV. Unsupported framework/business/provider promises remain
  explicitly excluded by the supported matrix.
- The measured facade dependency surfaces remain 16/81 normal packages and
  feature-tree lines with no defaults, 105/825 with defaults, 119/1,062 with
  defaults plus official plugins and 298/3,462 with all features. Relative to
  published 0.6.0, the normal-package deltas are 0, 0, +1 and +8.
- There are no Rust `#[deprecated]`, `#[doc(hidden)]`, `#[non_exhaustive]`,
  unstable or nightly markers. The 1.0 migration guide retains and documents
  `plugin new` to `make plugin`, Feedback `--message` to `--body` and bounded
  Plan schema-1 to schema-2 migration paths.
- A forced non-breaking audit exposed the previously undocumented 0.6-to-0.7
  source boundary: 18 new fields across `minco-aws-adapters`, `minco-plan` and
  `minco-deploy-aws`, plus the `StaticSitePublisher::publish` to
  `publish_manifest` change and expanded publication record. The 0.6-to-0.7
  guide now gives an exact migration for every finding.
- The hand-written 29-package candidate narrative was reconciled with the
  generated 32-package authority. No generated reference was edited manually.

This task changes documentation and the task record only. It performs no AWS,
database, registry, tag, deployment, product-repository or application
mutation.

## Qualification evidence

- `cargo-semver-checks 0.50.0` was installed under the ignored task
  `target/tools` directory. The exact required
  `cargo semver-checks --workspace --all-features` returned exit 101 after the
  published-package comparisons because first-publication `minco-mcp` has no
  0.6.0 registry baseline. The other absent baselines are
  `minco-plugin-realtime`, `minco-project-view` and `minco-workbench`; the gap
  is not recorded as a pass.
- The published-package fallback excluded only those four packages and forced
  `--release-type minor`. It returned expected exit 100 with four affected
  packages and the migrations above; all other published packages passed 196
  applicable checks each with 58 inapplicable checks skipped.
- `cargo test --workspace --all-targets --all-features --locked`: PASS. The
  explicitly configured real-AWS, Rustack and Orders PostgreSQL cases remained
  ignored without their provider/test URLs; no provider claim is inferred.
- `cargo doc --workspace --all-features --no-deps --locked`: PASS for the full
  workspace.
- `scripts/docs/generate-reference.sh --check`: PASS, seven generated files
  byte-current.
- `uv run --locked python scripts/validate_static.py`: PASS with zero errors
  and zero warnings.
- Documentation snippets: PASS, 181 fenced blocks. Documentation links: PASS,
  121 internal and 13 external links across 65 canonical pages.
- The separate repository-truth qualification child regenerated the
  deterministic reports and passed the complete `./scripts/quality.sh` matrix
  plus final `scripts/source_manifest.py --check` on the combined task tree.
