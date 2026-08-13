---
id: M14-T31
title: Prefer Apple Container for fresh local services
milestone: M14
status: done
priority: high
area: developer-experience/runtime
depends_on: [M14-T11]
operations: []
owned_paths:
  - crates/minco-cli/src/service_runtime.rs
  - crates/minco-cli/templates/app/README.md.tmpl
  - docs/DECISIONS.md
  - docs/adrs/0036-owned-local-service-runtimes.md
  - docs/adrs/0044-apple-container-default.md
  - docs/development/local-development.md
  - docs-site/next/guides/local-development.md
  - roadmap/tasks.mmd
  - tasks/M14/M14-T31-apple-container-default.md
  - verification/source-manifest.json
  - verification/static-validation.json
  - verification/1.6-performance-baseline.json
  - verification/deep-review.json
  - verification/operational-evidence-validation.json
checks:
  - cargo test -p cargo-minco service_runtime::tests::runtime_selection_is_deterministic_and_versions_fail_closed --locked
  - cargo test -p cargo-minco service_runtime::tests::auto_start_uses_the_ready_runtime_when_the_other_system_is_stopped --locked
  - cargo test -p cargo-minco --all-targets --all-features --locked
  - cargo clippy -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/test/repository_truth.py
  - scripts/docs/build.sh
  - scripts/docs/check-links.sh
  - scripts/docs/check-snippets.sh
  - uv run --locked python scripts/source_manifest.py --check
---

# M14-T31 - Prefer Apple Container for fresh local services

## Goal

Make a ready, qualified Apple Container 1.2.x runtime the default for fresh
`MINCO_CONTAINER_RUNTIME=auto` local PostgreSQL and Rustack services on Apple
silicon macOS. Preserve exact receipt/resource recovery, explicit Docker
selection, Docker fallback when Apple is unavailable, and Docker-only Compose
customization.

## Acceptance

- when Apple Container and Docker are both ready and no owned resource or
  receipt exists, `auto` selects Apple Container;
- an exact existing receipt or owned resource still wins so changing the
  default never strands or silently migrates data;
- explicit `docker` and `apple` selections retain their current fail-closed
  behavior;
- Docker remains the fallback when Apple Container is unavailable or
  unsupported;
- current and next documentation state the platform-aware preference without
  rewriting the immutable `1.6.0` manual;
- Minco-owned local Docker resources are removed only after separate ownership,
  inactivity and PostgreSQL data-migration proof; and
- foreign Docker resources, including active PeoplePlanner CI resources, are
  not changed.

## Non-goals

- removing Docker or Compose support from Minco;
- parsing arbitrary Compose services into Apple Container;
- changing production runtime, Plan IR, deployment or cloud cost behavior;
- automatically copying or deleting persistent volumes; or
- releasing a new crate version.

## Starting evidence

Exact source starts from merged `main`
`f2282d9e774f55d3947e189f19b30f93b5edb167`. Installed Apple Container
`1.2.2`, Docker `29.7.2` and Compose `5.4.0` are all ready. Repository source
and tests currently select Docker when both runtimes are ready. Apple Container
has the pinned PostgreSQL and Rustack images and no running container. Docker
has no running Minco container and retains three Minco-managed PostgreSQL
volumes; their deletion remains blocked on the migration checks above.

## Completion evidence

- The new both-ready regression failed with Docker before the implementation
  and passes with Apple after changing fresh `auto` selection. All 147 CLI unit
  tests and every `cargo-minco` integration target pass; scoped all-feature
  Clippy is warning-free.
- Repository truth passed 53 tests. Documentation passed 350 snippet checks, a
  production VitePress build and 1,666 internal-link checks. Static validation
  reports zero errors and zero warnings across 111 task contracts.
- The source-built `1.6.0` CLI selected Apple Container without an environment
  override for both OneUnity and the Minco Orders example, reused each exact
  Apple volume, answered PostgreSQL queries and stopped cleanly.
- OneUnity's active Apple database retains 69 application tables and 177 audit
  records. The older unmanaged CGSP Docker database contained distinct data, so
  it was restored alongside the active database as
  `cgsp_legacy_docker_20260813`; all 566 logical catalog objects, 66 table
  fingerprints and 6,968 primary-keyed row fingerprints match the Docker
  source. The active database was not overwritten.
- The Minco framework's legacy Docker `minco_orders` database was restored into
  a separate Apple volume. Its canonical SQL dump is byte-identical and its 11
  table fingerprints, keyed-row fingerprints and normalized object catalog all
  match the Docker source.
- Recoverable pre-deletion logical dumps and checksums remain outside the
  repository under `~/Library/Application Support/Minco/migration-backups/`.
  After restore verification, five exact Minco/CGSP PostgreSQL Docker volumes,
  one stopped Minco container and the pinned PostgreSQL Docker image were
  removed. No Minco/CGSP Docker container, volume or image remains; 613 foreign
  Docker volumes and foreign project images were left unchanged.
- The checked performance state remains truthfully `NOT RUN`; its evidence was
  rebound to the exact verified source tree without claiming a measurement or
  provider contact.
