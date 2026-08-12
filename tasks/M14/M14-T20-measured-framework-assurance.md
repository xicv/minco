---
id: M14-T20
title: Establish measured framework assurance and maintainability baselines
milestone: M14
status: active
priority: high
area: quality/performance/maintainability
depends_on: [M14-T19]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - CODEX_HANDOFF.md
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - quality.toml
  - docs/DECISIONS.md
  - docs/adrs/0040-measured-framework-assurance.md
  - docs/development/quality-assurance.md
  - docs/reference/generated/diagnostics.md
  - docs/research/aws-rust-capability-review-2026-08.md
  - crates/minco-cli/src/main.rs
  - crates/minco-cli/src/command.rs
  - crates/minco-cli/src/delivery_evidence.rs
  - crates/minco-plan/Cargo.toml
  - crates/minco-plan/src/**
  - crates/minco-release/Cargo.toml
  - crates/minco-release/src/**
  - scripts/ci/local-assurance.sh
  - scripts/ci/local-release.sh
  - scripts/quality.sh
  - scripts/quality_assurance.py
  - scripts/release/release_identity.py
  - scripts/source_manifest.py
  - scripts/test/candidate_qualification.py
  - scripts/test/hosted_ci_policy.py
  - scripts/test/operational_evidence.py
  - scripts/test/quality_assurance.py
  - scripts/test/release_identity.py
  - scripts/test/repository_truth.py
  - scripts/validate_static.py
  - tasks/M14/M14-T20-measured-framework-assurance.md
  - verification/1.4-candidate-load.json
  - verification/1.4-performance-baseline.json
  - verification/aws-capability-candidates.toml
  - verification/deep-review.json
  - verification/operational-evidence-validation.json
  - verification/publish-validation.json
  - verification/quality-assurance-policy.toml
  - verification/quality-assurance.json
  - verification/release-identity.json
  - verification/provider-evidence.toml
  - verification/repository-truth.toml
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - uv run --locked python scripts/test/quality_assurance.py
  - uv run --locked python scripts/test/release_identity.py
  - uv run --locked python scripts/quality_assurance.py --check-output verification/quality-assurance.json
  - uv run --locked python scripts/release/release_identity.py --check
  - cargo test -p minco-plan --all-features --locked
  - cargo test -p minco-release --all-features --locked
  - cargo test -p cargo-minco --locked
  - cargo clippy -p minco-plan -p minco-release -p cargo-minco --all-targets --all-features --locked -- -D warnings
  - scripts/ci/local-assurance.sh
  - scripts/quality.sh
  - scripts/ci/local-release.sh
---

# M14-T20 - Establish measured framework assurance and maintainability baselines

## Goal

Turn Minco's post-1.4 quality and performance aspirations into reproducible,
exact-source evidence without broadening the runtime framework. Establish
measured coverage, mutation, SemVer, test-runner and gate-duration baselines;
retain current local performance diagnostics without promoting them to hosted
or provider proof; reduce the highest-risk CLI concentration point; and give
release consumers one deterministic identity projection over independently
validated source authorities.

## Acceptance

- exact compatible versions of cargo-nextest, cargo-llvm-cov, cargo-mutants
  and cargo-semver-checks are reviewed and pinned in a source-bound policy;
- the assurance runner is deterministic, confines private logs to
  `target/minco`, emits stable diagnostics, records exact tool versions,
  source identity, command durations and report digests, and fails closed for
  missing, stale, malformed or misleading evidence;
- nextest is accepted only after observable test inventory/outcome parity,
  coverage thresholds derive from a committed measured baseline rather than an
  invented percentage, mutation testing is bounded to pure authority and Plan
  decision code, and SemVer checks compare against immutable tag `v1.4.0`;
- exact-tree local API/worker performance is recorded as machine-specific,
  provider-free and `production_slo = false`; the hosted baseline and current
  provider evidence remain truthfully `NOT RUN` when unavailable;
- one behavior-preserving CLI decomposition removes command schema ownership
  from the dispatch module while preserving every Clap name, help surface,
  JSON contract and diagnostic;
- one deterministic release-identity projection binds workspace, repository
  truth, package, plugin and documentation identities while independent
  validators remain authoritative;
- property-oriented tests strengthen Plan and release digest invariants through
  public interfaces without adding a general repository or cloud abstraction;
- current official AWS and Rust ecosystem conclusions are dated, source-bound
  and leave every capability at its existing support state unless Minco has
  implementation, cost, security, recovery, performance and provider proof;
- the authoritative local quality and release gates pass on the final source;
  and
- source, hosted Linux, live-provider, deployment, publication and production
  evidence remain separate claims.

## Non-goals

- changing a public Rust API, serialized Plan IR, CLI command name, plugin
  compatibility boundary or release version;
- adding an AWS service, generic provider abstraction, runtime scanner,
  scheduler, poller, telemetry service or hosted Minco control plane;
- imposing an arbitrary repository-wide coverage or mutation percentage;
- contacting AWS, Waffo or another live provider, creating resources,
  deploying, tagging, publishing, releasing or merging; or
- treating local macOS performance, nextest speed or a lexical quality check as
  production, hosted-Linux or provider proof.

## Recovery and workspace

The task did not exist when P0 implementation was authorized. Its isolated JJ
workspace was therefore bootstrapped directly from exact merged `main`
`f48ead125b09699f1d7e8ab8bf02deeeb9dc6fb4` at
`/private/tmp/minco-task-m14-t20`. The stale detached primary checkout and
unrelated `task-m12-t09` workspace are not used for task mutation.

## Evidence

Active. Exact base `f48ead125b09699f1d7e8ab8bf02deeeb9dc6fb4` was
verified before mutation. The base had 122 executable tests plus one doctest;
four focused regressions raise the exact current inventory to 126 plus one
doctest. Base coverage is 84.91% lines/80.98% functions and current measured
coverage is 85.65%/82.01%, with 46 bounded
mutants (43 caught, zero missed, zero timeout, three unviable). CLI help remains
byte-identical at SHA-256
`ce7f5203366875eeb62daf3f1584eba5eb7f2b91b7930f8b59b1de0dfdf5d2f7`.
The exact four tool versions are recorded in the policy and the immutable
`v1.4.0` SemVer baseline passed the manual workspace comparison.

The canonical final local result is `verification/quality-assurance.json` and
must verify against current source and every digest-addressed private artifact.
The exact-head security review closed 23/23 source-like rows and reproduced one
Low/P3 gap where substituted absent command, coverage and mutation artifacts
were accepted. Nineteen focused Python tests now cover matching, absent,
substituted and symlinked evidence plus the clean ephemeral release wrapper.
Clean release qualification executes the measured lane under ignored
`target/minco` outputs; canonical frozen validation requires its original
private evidence. Exact-tree hosted Linux performance remains `NOT RUN`; no
current live-provider evidence exists. Those unavailable lanes keep the task
active and cannot be reported as PASS.
