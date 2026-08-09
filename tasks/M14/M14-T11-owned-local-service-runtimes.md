---
id: M14-T11
title: Harden owned local service runtimes
milestone: M14
status: complete
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
  - docs/reference/generated/**
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

### Implementation and red-green review

The final implementation has one hidden, version-coupled
`cargo-minco __local-service` boundary rather than a separately installable
helper. `LocalServiceSpec` is shared by Docker and Apple adapters; the CLI
composition root injects `current_exe()` only when turning the deterministic
DevPlan into processes. Application and workspace scoped locks and atomic,
secret-free receipts preserve the selected runtime. A receipt never authorizes
mutation without fresh structured runtime inspection and exact Minco label,
image, port, mount and configuration verification.

The TDD review reproduced and closed these specific failures before the final
green runs:

- clean packaged generated applications could exceed the original 30-second
  application-readiness budget while compiling; a missing-constant compiler
  failure preceded the bounded five-minute development budget;
- receiptless `auto` startup incorrectly reused shutdown's strict all-runtime
  discovery and failed when the non-selected installed runtime was stopped;
  startup now checks every ready runtime for ambiguity while preserving Docker
  preference and Apple fallback, and receiptless shutdown remains strict;
- Apple inspect reports the selected platform manifest digest rather than the
  configured OCI index digest; exact canonical image identity now accepts that
  verified relation without accepting a mutable tag;
- Docker `stop` uses `--timeout`, while Apple `stop` uses `--time`;
- application receipts previously shared a workspace-only path and could
  collide; their path is now workspace, application and service scoped;
- long normalized names could discard workspace identity; they now retain the
  workspace fingerprint, with symlink aliases stable and distinct JJ
  workspaces distinct;
- stale changed-configuration receipts, malformed/missing labels, image or
  digest mismatches, unexpected Docker mounts, stopped-container ports,
  missing volumes and occupied foreign ports all have fail-closed regression
  tests; and
- image overrides are rejected unless they are full immutable `@sha256:`
  references with a 64-hex digest; and
- the first hosted unit/package job exposed its missing JJ prerequisite when
  three existing compatibility tests selected the default VCS profile; the
  workflow now installs the repository-canonical pinned `jj-cli` 0.43.0, and
  the three-test boundary plus `actionlint` pass locally; and
- the corrected hosted unit/package job then reached the generated-app checks
  and exposed their missing `rg` prerequisite after package verification and
  PostgreSQL generation had passed; the workflow now also installs the
  repository-canonical pinned ripgrep 15.2.0.
- the final merge-gate review reproduced a fail-open edge where an unsuccessful
  runtime inspection with empty output was treated as resource absence; the
  container and volume paths now accept only explicit missing-resource
  diagnostics, with a regression test covering the empty failure response.

### Local test and validation matrix

All commands below were executed in the isolated `minco-task-m14-t11`
workspace on Rust 1.97.1 unless a different boundary is stated.

| Boundary | Command or operation | Observed result |
|---|---|---|
| Targeted format | `rustfmt --edition 2024 --check` on the five changed Rust files | Passed; no formatter ran in write mode. The required task-finish gate later invoked non-mutating `cargo fmt --all -- --check`; it passed and rewrote nothing. |
| Affected unit/integration | `cargo test -p minco-dev --all-targets --all-features --locked` | Passed: 9 plan and 9 supervisor tests. |
| Affected CLI | `cargo test -p cargo-minco --all-targets --all-features --locked` | Passed after the merge-gate correction: 116 unit tests and every CLI integration target. One earlier parallel validation attempt transiently failed the immediate lock-release assertion; its exact isolated rerun and the complete isolated package rerun passed. |
| Affected lint | package-scoped `cargo clippy` for `minco-dev` and `cargo-minco`, all targets/features, locked, `-D warnings` | Both passed after task-owned findings were fixed. |
| Build | `cargo build --locked -p cargo-minco --bins` | Passed with only `cargo-minco`; the old helper binary is absent. |
| Generated applications | `scripts/test/generated_apps.sh` | Passed PostgreSQL and SQLite generation, compilation and tests; expected generated TODO tests failed only where the harness requires them to. |
| Package | `cargo package --locked -p cargo-minco --allow-dirty` | Passed against the final local source: 117 files, 1.1 MiB unpacked and 222.5 KiB compressed; registry-dependency verification passed. |
| Install | `cargo install --locked --force --path target/package/cargo-minco-1.1.0 --root /tmp/minco-m14-t11-package-artifact-final.uh9j9i` | Passed from the final packaged source; exactly one executable, `bin/cargo-minco`, was installed. |
| Restricted path | installed `cargo minco dev --dry-run --json` with only the temporary Cargo root plus system paths | Passed; plans retain symbolic `cargo-minco __local-service`, not a host-specific path. Runtime execution additionally requires the runtime CLI path. |
| Workflow/YAML | `actionlint`; Ruby YAML parses of the workflow, root Compose and generated Compose template; `docker compose ... config --format json` | Passed; Compose exposes exactly `postgres` and `rustack`. |
| Safe quality subset | every safe `scripts/quality.sh` stage run individually | Static validation had 0 errors/warnings; generated reference, repository truth (41), hosted policy (4), recipes (11 plus matrix), publish, deep-review, AWS portability, SQLx isolation, feedback browser (40), snippets (321), link (457 internal, 14 external, 141 canonical) and docs browser (34, 2 intended skips) checks passed. The required task-finish gate later ran non-mutating workspace format and Clippy checks; both passed without rewrites or source fixes. |
| Compiler matrix | `cargo check` for `minco` no-default/default/official-plugins/aws-worker/all-features, the all-target/all-feature workspace, and `cargo-minco` | Passed. |
| Workspace regression | `cargo test --workspace --all-targets --all-features --locked` | Passed all runnable tests. Nine explicitly environment-gated tests remained ignored: 4 configured PostgreSQL, 1 DynamoDB Rustack, 2 bounded real-AWS/S3, and 2 Rustack adapter/Lambda tests. |
| Documentation | `npm --prefix docs-site run build`; docs link/browser checks | Manual site build and checks passed. `scripts/docs/build.sh` did not pass because the unchanged lockfile reports `nanoid <3.3.17`, GHSA-2v37-7h3g-55p8, one high advisory. |
| Secrets/integrity | final-diff Gitleaks, source-manifest generation/check, current-commit conflict query, changed-file whitespace and diff inspection | Passed at the final local source boundary; the source manifest covers 1084 files and the changed-file scan found no trailing whitespace. The JJ-only task workspace has no `.git`, so the Git-only `git diff --check` transport check remains literally unavailable there. |

`scripts/quality.sh` itself was not run end-to-end; its safe stages were
reproduced individually. The required task-finish workflow later invoked
non-mutating `cargo fmt --all -- --check` and workspace-wide Clippy through
`cargo minco check --with-cargo`; both passed, rewrote nothing and required no
source fix. This is recorded as an exact workflow deviation from the narrower
requested formatter/lint boundary. The task does not change or waive the
unrelated docs-site advisory.

### Docker integration

Docker client 29.7.1, daemon 29.6.2 and Compose 5.3.1 on arm64 qualified exact
owned services with unique application identities:

- Rustack on loopback port 54566 passed structured capability health, Rust SDK
  STS and AWS CLI STS against local account `000000000000`;
- PostgreSQL on loopback port 55439 authenticated as the expected user/database,
  executed SQL and preserved sentinel `m14-t11-docker` across ordinary
  stop/restart;
- `auto` with both runtimes ready selected Docker and shutdown touched only the
  selected Docker resources;
- with Apple Container restored to its pre-task stopped state, the final
  installed package selected Docker under `auto`, started Rustack on 54575,
  passed readiness, stopped it exactly, and left Apple stopped;
- an installed generated application served its native API on loopback 33091
  with Rustack 54570 and PostgreSQL 55432, and Ctrl-C stopped the API and exact
  owned containers while retaining the database volume;
- a foreign process on 54571 and a same-named container without the full Minco
  ownership contract were refused and left untouched; and
- the legacy `local_minco-postgres` volume was diagnosed but never adopted,
  stopped or deleted.

The daemon-stopped case was not exercised against the real daemon because an
unrelated user-owned CGSP PostgreSQL container was running. The fake-runner
test covers Docker-unavailable Apple fallback; preserving the unrelated
container took precedence over a disruptive live daemon test. After exact
label inspection, every task-created Docker container, volume and Compose
network was removed; the unrelated container was not touched.

### Apple Container integration

The local machine is macOS 26.5.2 build 25F84 on arm64 with Apple Container
1.2.0. Its global service was stopped before the task, started only for this
qualification, and restored to stopped state afterward.

- Rustack on loopback 54567 was native arm64, retained the expected labels and
  immutable image reference, passed structured health, Rust SDK STS and AWS CLI
  STS against local account `000000000000`;
- PostgreSQL on loopback 55440 authenticated, executed SQL and preserved
  sentinel `m14-t11-apple` across stop/restart in its named ext4 volume;
- a same-named unlabeled container was rejected without lifecycle mutation;
- an installed generated application served its native API on 33092 with
  Rustack 54573 and PostgreSQL 55432;
- killing the supervisor after readiness left secret-free receipts and owned
  containers, while the orphan native API process was identified by its
  disposable project working directory and terminated normally;
- the next invocation reused the exact resources, became ready, and Ctrl-C
  stopped only those resources; repeated explicit stops were idempotent; and
- task-created Apple containers and volumes were removed only after exact label
  inspection.

Apple evidence is local-machine evidence, not hosted Apple CI. Installed 1.2.0
and current stable 1.2.2 are within the deliberately bounded 1.2.x support
line.

### Security, provenance and remaining boundaries

The Rustack image remains the upstream-published immutable OCI index
`ghcr.io/tyrchen/rustack:0.9.1@sha256:18cd91395e17453e2c34b299e45f4679dc2427473dc1db6541bbe212fd70a104`,
built from exact MIT-licensed commit
`ab8bc61a3e45058c7d42de8443f9d215cc110b18`. Native arm64 manifest
`ec5a7ffee62c29bebd4862c826c34335928fd017977ed78c551d2dba5e94f5fb`
and amd64 are present. BuildKit SLSA provenance attestations exist, but no Git
tag or OCI signature was established: attested, not signed. The exact 18-service
allowlist and health schema are tied to v0.9.1.

Every published port is loopback-only. Local SDK configuration has an explicit
Rustack endpoint, region and local credentials with EC2 metadata disabled; no
real AWS call was made. Plans, argv, logs, errors, receipts and generated files
were inspected for secret values. This task adds no deployment resource, NAT,
fixed compute, schedule or provisioned concurrency and changes no production
cost model.

There is intentionally no destructive public reset command in this slice.
Ordinary stop preserves data; manual volume deletion remains outside Minco and
requires an explicit operator data-loss decision after ownership inspection.
Hosted Linux unit/package/generated and Docker qualification is supplied by
`.github/workflows/local-dev-runtime-validation.yml`; there is no hosted Apple
runner. Real AWS tests remain intentionally unrun. Final draft-PR and exact-SHA
hosted results are external evidence recorded at handoff, not inferred from
local success. Draft PR [#134](https://github.com/xicv/minco/pull/134) exists.
On implementation SHA `93aa70f6698fe6b151875ddb11a05ccfcc51115a`, hosted
run [31298039515](https://github.com/xicv/minco/actions/runs/31298039515)
passed the real Docker job and failed the unit/package job only because JJ was
absent; the pinned-JJ workflow correction is locally green and the final task
head is requalified separately before handoff.
