# ADR-0013: Keep local quality and update workflows authoritative

- Status: Accepted
- Date: 2026-07-23
- Last reviewed: 2026-08-03

## Context

Minco needs a small, explicit and durable foundation that can be reasoned about
by humans, AI agents, local tooling and deployment planners without duplicating
sources of truth.

The complete manual hosted workflow had become a routine duplicate of the local
gate. The twenty most recent jobs measured on 2026-07-31 consumed 320.5 runner
wall-minutes with a 16.2-minute median. The repository is public, so
[standard GitHub-hosted runner minutes are currently unbilled][billing], but
runner time, cache and artifact storage are still finite resources. In
particular, a workspace-target Rust cache was about 4.5 GiB. The same design
would also become directly billable if the repository became private.

## Decision

Unit, feature, E2E and complete quality commands run locally.
`./scripts/quality.sh` is authoritative and contains the complete static,
compiler, test, browser, documentation, dependency, security and source-identity
matrix.

GitHub Actions remains optional, read-only and
[`workflow_dispatch`-only][workflow-dispatch]. Its default `essential` profile
runs a bounded clean-Linux-runner gate:

1. static repository truth;
2. byte-for-byte generated package, feature, plugin, CLI, configuration, Plan
   and diagnostic reference freshness, using the exact-ref `cargo-minco` binary;
3. cross-source repository truth and the hosted-CI policy regression;
4. Rust formatting;
5. an all-workspace, all-target, all-feature locked compiler check;
6. exact source-manifest verification.

The reference freshness step builds only the locked `cargo-minco` control-plane
binary and runs read-only help, schema and plan inspection against the checkout.
It performs no provider contact. The default profile does not install browsers,
JJ, ripgrep, security tooling, Cargo Lambda or Zig; it does not run the full
tests, publish dry-run, native Lambda artifact build, Rustack or E2E; and it does
not upload browser artifacts.
Workspace target caching is disabled. Registry caching remains available, with
no cache save on failure.

An explicitly selected `release` profile retains the complete local quality
matrix, packaging dry-run, native ARM64 Lambda build, Plan/SAM validation and
optional Rustack/E2E reproduction on a clean Linux runner. This is
qualification evidence, not permission to publish, deploy or promote.

Same-workflow, same-ref runs use
[concurrency cancellation][concurrency] so stale work does not continue after a
replacement dispatch. `minco update` checks and applies reviewed
toolchain/dependency changes only from a clean workspace.

## Consequences

The project remains usable without hosted CI and does not perform unsigned
self-replacement. Ordinary changes pay the local quality cost once and may add
one bounded clean-runner proof. Release candidates deliberately pay for the
broader hosted reproduction only when it is useful.

Local, essential hosted, release hosted, real-AWS and production evidence remain
separate claims. Evidence, not automation venue, determines release readiness.

Changes that invalidate this decision require a superseding ADR and migration/compatibility plan.

[billing]: https://docs.github.com/en/actions/how-tos/monitor-workflows/view-job-execution-time
[concurrency]: https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency
[workflow-dispatch]: https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#onworkflow_dispatch
