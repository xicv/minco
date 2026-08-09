---
id: M14-T11
title: Harden owned local service runtimes
milestone: M14
status: in_progress
priority: critical
area: developer-experience/runtime
depends_on: [M4-T02, M9-T05]
operations: []
owned_paths:
  - .env.example
  - Cargo.toml
  - Cargo.lock
  - .github/workflows/local-dev-runtime-validation.yml
  - crates/minco-dev/**
  - crates/minco-cli/**
  - infra/local/**
  - docs/DECISIONS.md
  - docs/adrs/0036-owned-local-service-runtimes.md
  - docs/development/local-development.md
  - verification/**
  - tasks/M14/M14-T11-owned-local-service-runtimes.md
checks:
  - rustfmt --edition 2024 --check <modified-rust-files>
  - cargo test -p minco-dev --all-targets --all-features --locked
  - cargo test -p cargo-minco --all-targets --all-features --locked
  - cargo clippy -p minco-dev --all-targets --all-features --locked -- -D warnings
  - cargo clippy -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - scripts/test/generated_apps.sh
---

# M14-T11 — Harden owned local service runtimes

## Goal

Complete the prototype Docker Compose and Apple Container service boundary used
by `cargo minco dev`: preserve the native application process, derive only
declared PostgreSQL and Rustack dependencies, prove resource ownership before
reuse or lifecycle mutation, preserve the selected runtime through shutdown,
and work from an installed Cargo package with no ambient helper lookup.

## Non-goals

- containerizing the Minco application;
- implementing a general container orchestrator or Compose parser;
- arbitrary Sail-style command forwarding;
- managing Docker Desktop or Apple's global Container system lifecycle;
- adding production infrastructure, fixed compute, schedules or cloud cost;
- contacting real AWS or deleting unproved local data;
- supporting arbitrary Compose-only services through Apple Container.

## Starting state

- Remote prototype: `665abc146cfff94b297ea849469f78edf9fd321e`.
- Verified starting main: `d4cfe76736c26f414b94aa39481943e083e3d336`.
- Current integrated main: `80c0bc71dc21252e853e960745ed984a1a4fe9f5`.
- Clean two-parent JJ reconciliation: prototype plus current main; no source
  changes were required during reconciliation.
- No existing PR and no owning open task existed.
- Baseline `cargo test -p minco-dev -p cargo-minco --all-features --locked`
  passed before task changes: 81 CLI unit, 6 helper, 9 plan, 9 supervisor and
  all CLI integration tests passed with zero failures.

## Research gate

Completed before production-code changes; ADR-0036 records the accepted
architecture and rejected alternatives.

### Exact sources and local tools

| Subject | Verified source/version | Evidence and conclusion |
|---|---|---|
| Laravel Sail | Official Laravel 13 Sail guide, read 2026-08-09 | Adopt one obvious command, project-owned services, predictable lifecycle, persistent data, diagnostics and explicit customization. Reject the application container, PHP command proxy, broad catalogue and Docker-only architecture. |
| Apple Container releases | Official `apple/container` releases and tagged command reference | `1.2.0` was published 2026-07-29; current stable re-verification found `1.2.2`, published 2026-08-08. The installed and qualified CLI is `1.2.0`; support is bounded to the qualified 1.2.x line. |
| Local Mac | macOS 26.5.2 build 25F84, `arm64` | Apple Container was installed but its global service was stopped before this task. The task started it only for isolated research/qualification and must restore stopped state. |
| Apple CLI | `container CLI version 1.2.0 (build: release, commit: unspeci)` | Local help verified run/create/start/stop/delete/inspect/logs/image/volume/system surfaces. `--label`, inherited `--env KEY`, literal `--env KEY=value`, loopback publish, named volumes, `--rm` and `--platform` are supported. |
| Rustack source | `xicv/rustack` master and `tyrchen/rustack` `v0.9.1` | Both resolve exactly to `ab8bc61a3e45058c7d42de8443f9d215cc110b18`; upstream is the publishing authority and the fork is identical. MIT license permits use/redistribution with notice. |
| Rustack image | `ghcr.io/tyrchen/rustack:0.9.1@sha256:18cd91395e17453e2c34b299e45f4679dc2427473dc1db6541bbe212fd70a104` | OCI index contains native amd64 `ca2f8c…` and arm64 `ec5a7f…` manifests plus per-platform BuildKit SLSA provenance attestations. Image labels bind source/revision/version. The Git tag is unsigned and no OCI signature was established: attested, not signed. |
| Rustack contract | Exact `v0.9.1` source and health tests | 18 identifiers match Minco's allowlist. Health is JSON `services` mapping identifiers to `running`, so requested capabilities can be checked individually. |
| Cargo | Cargo 1.97.1 (`c980f4866`, 2026-06-30) plus clean install/package experiment | `cargo build --bins` passed. Default path install produced both binaries; `--bin cargo-minco` produced only the CLI. `cargo package` produced 117 files and verified. A bare helper is therefore unsafe. |
| Docker | Docker client 29.7.1, daemon 29.6.2, Compose 5.3.1 on `aarch64` | Daemon was ready. Compose structured config preserved loopback ports. Explicit projects add project/service/config labels. `stop` retained the container and volume; `down` removed container/network but retained the named volume. |
| Documentation lookup | Context CLI installed with no packages; Context Hub 0.1.4 had no authoritative Sail, Apple, Rustack, Compose or SQLx entries | Official tagged repositories, local pinned crate source and actual CLI help were used instead of unrelated registry results. |

### Measured Apple lifecycle behavior

An isolated `minco-research-019fe3fe` container and volume carried explicit
research ownership labels and were removed only after inspect proved those
labels. Inspect JSON places labels, image, init environment, mounts, platform
and published ports under `configuration`, and lifecycle state under `status`.
Inherited and literal environment forms produced the expected values;
`127.0.0.1:58081:8080` remained loopback-only; the selected Alpine variant was
native `linux/arm64`. Stopping a stopped container exited 0. Deleting a running
container without force exited 1 and left it running. Normal stop then delete
exited 0. `--rm` removed a completed container. Its named labeled volume
survived container deletion and was then explicitly removed as task-created
disposable data.

### Measured Compose and legacy behavior

Without `--project-name`, Compose derived the project `local` from
`infra/local/compose.yaml`. The new explicit project therefore can strand a
legacy `local_minco-postgres` volume. An isolated explicit project proved
Compose project/service/config-file/config-hash labels and named-volume
project/volume labels. The implementation must diagnose compatible legacy
resources but cannot mutate or delete them. Canonical workspace identity is
used for new resources so relative, absolute and symlink-alias invocation do
not split one workspace, while separate or moved workspaces stay isolated.

### Packaging decision

Remove the second binary. Use a hidden `cargo-minco` local-service subcommand
and inject `current_exe()` at the CLI/supervisor boundary. This is deterministic
for source execution, default or single-binary Cargo installation, restricted
`PATH`, package-manager layouts and atomic upgrades. The exact path is an
execution detail and never enters DevPlan JSON.

## Required design invariants

The accepted decision must preserve a native application, one typed first-class
service specification, loopback-only ports, authenticated/structural readiness,
verified Minco ownership, deterministic runtime selection and recovery,
project-local locking, atomic non-secret receipts, persistent ordinary-stop
data, explicit destructive-reset authority, no public-AWS fallback, deterministic
side-effect-free dry runs, bounded cleanup of only attempt-created resources,
installed-package operation, actionable runtime version gates, and unchanged
production deployment/cost plans.

## Evidence

Evidence is intentionally incomplete until research, red-green implementation,
local Docker/Apple integration, packaging, generated-project and hosted checks
have run against the final exact SHA. Unavailable tools and intentionally
unrun live/cloud boundaries remain explicit failures or skips, never passes.
