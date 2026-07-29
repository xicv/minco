---
id: M8-T07
title: Prepare the Minco 0.4.0 source and package boundary
milestone: M8
status: complete
priority: critical
area: release/crates-io
depends_on: [M9-T07, M10-T03]
operations: []
owned_paths:
  - .github/workflows/minco-manual.yml
  - Cargo.toml
  - Cargo.lock
  - CHANGELOG.md
  - README.md
  - CODEX_HANDOFF.md
  - PUBLISHING.md
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - crates/minco-cli/src/main.rs
  - crates/minco-deploy-aws/**
  - crates/minco-dev/tests/supervisor.rs
  - docs/**
  - extensions/minco-aws-adapters/README.md
  - roadmap/**
  - scripts/validate_static.py
  - scripts/aws/build-lambda.sh
  - scripts/aws/build-worker-lambda.sh
  - scripts/aws/cleanup.sh
  - scripts/aws/lib/common.sh
  - scripts/aws/run-bounded-root-bootstrap.sh
  - scripts/quality.sh
  - scripts/release/publish.py
  - scripts/test/aws_shell_portability.sh
  - scripts/test/lambda_artifact_reproducibility.py
  - scripts/test/publish_validation.py
  - scripts/test/repository_truth.py
  - tasks/M8/M8-T07-minco-0.4.0-release.md
  - tasks/M10/**
  - verification/**
checks:
  - uv run --locked python scripts/validate_publish.py
  - cargo rustdoc -p cargo-minco --lib --all-features --locked
  - scripts/quality.sh
  - npm run --prefix plugins/minco-plugin-feedback test:browser
  - scripts/test/e2e.sh
  - scripts/dev/rustack-smoke.sh
  - scripts/aws/plan.sh
  - scripts/aws/validate.sh
  - scripts/aws/build-lambda.sh
  - scripts/aws/build-worker-lambda.sh
  - scripts/release/package-list.sh
  - scripts/release/publish.sh --skip-quality
---

## Goal

Prepare one exact `0.4.0` source and package candidate covering the accepted
framework work through M10-T03, with the four new crates, truthful zero-idle
doctrine, an upgrade guide and independently inspectable evidence.

## Release phases

1. Source reconciliation: version, package inventory, task graph, decisions,
   documentation and regression diagnostics.
2. Local qualification: compiler, tests, generated applications, browser,
   Rustack, Plan/SAM, Lambda and archive gates.
3. Hosted qualification: exact pull-request head, including the same material
   release gates.
4. Live AWS rehearsal: separately authorised account/Region mutation and
   runtime/rollback evidence.
5. Publication: separately authorised exact merged-main tag and crates.io
   upload, followed by registry, external-consumer and docs.rs verification.

Passing an earlier phase never implies a later phase. This task and its pull
request stop before merge, AWS mutation, tag creation and registry upload unless
those actions receive separate explicit approval.

## Release boundary

- Published baseline: `0.3.1`, 24 packages.
- Candidate: `0.4.0`, 28 packages.
- First-publish crates: `minco-config`, `minco-db`, `minco-dev` and
  `minco-deploy-aws`.
- Included program boundary: accepted work through M10-T03.
- Deferred: M10-T04, M10-T05, M10-T06, M10-T07, M11 and M12.
- MSRV: Rust `1.97.1`.

## Acceptance

- all 28 public crates use lock-step `0.4.0` dependencies and Cargo.lock is
  reviewed;
- repository truth, README, CLI reference, handoff, framework maturity,
  publishing guide and verification state agree;
- all four first-publish crates receive unpacked-archive tests;
- a `0.3.1` to `0.4.0` guide covers schemas, crates, commands, features,
  generators and deferred operational gates;
- exact source, package, generated-consumer, browser, Rustack and AWS-static
  gates are recorded as pass/fail/blocked/not run without converting missing
  tools or unauthorised live checks into passes;
- the manual hosted workflow builds both native ARM64 Lambda ZIPs and validates
  the generated Plan/SAM boundary without AWS credentials or provider contact;
- the manual hosted workflow pins JJ and verifies its version before running
  compatibility fixtures that require a real `@-` baseline;
- the manual hosted workflow pins ripgrep and verifies its version before
  generated-application checks use `rg`;
- the manual hosted workflow installs Zig `0.14.0` through an immutable action
  commit before Cargo Lambda performs native ARM64 cross-compilation;
- consecutive byte-identical Lambda builds produce the same normalized ZIP
  digest, permissions and entry inventory;
- no credentials, customer data, secret values or live provider mutations enter
  source or evidence.

## Non-goals

- merging the release pull request;
- creating or pushing `v0.4.0`;
- publishing to crates.io;
- creating, modifying or deleting AWS resources;
- implementing rollback/canary, static-site domains, preview lifecycle, a
  pricing engine, hosted Minco control plane, documentation platform or AI
  workbench.

## Evidence

Focused source, feature, new-crate, generated-application, browser, Orders E2E,
Rustack, Plan/SAM, Lambda and coordinated 28-archive dry-run gates pass locally.
The native Lambda helpers normalize volatile Cargo Lambda ZIP metadata and a
dedicated regression proves stable digests plus fail-closed entry validation.
The archive gate tests all five configured packages, compiles no-default,
default, full and four-new-crate consumers from unpacked archives, and installs
the unpacked `cargo-minco` binary. Exact commands, artifact hashes,
measurements, diagnostics and limitations are recorded in `VERIFICATION.md`.

The clean source passed the authoritative local suite. Corrected pull-request
head `46be92f0b68e6759a897ef5e99c010d77c2bf32b` passed manual hosted run
`30410242657`, including authoritative quality, browser, coordinated package
dry-run, Plan/SAM/native Lambda, Rustack and E2E stages. A later evidence
revision exposed a race in the packaged `minco-dev` shutdown fixture during
run `30411179583`; the fixture now waits for a complete numeric descendant PID
before requesting shutdown and passed 600 repeated full-suite runs locally.
Corrected exact head `b211b5083b43a0c9a0de9cd28ca4f748dfbbeb51`
then passed every stage of hosted run `30412849538`. The source/package task is
complete and ready for an exact-head guarded merge. Live AWS rehearsal, tag
and registry publication remain separately authorised and evidenced phases.

The first authorised live-AWS invocation on 2026-07-29 stopped before caller
discovery or resource creation because macOS Bash rejected the controller's
own hyphenated default SSM parameter name. The portable shared predicate and
`scripts/test/aws_shell_portability.sh` regression retain every fail-closed
normalization check while accepting the generated default. Exact-main
qualification and the live rehearsal must be repeated after this correction;
no tag or registry upload occurred.

That correction merged as
`d34c0e49d881a5ababdc1e9576c046c867f45ab3`, which passed the complete local
suite and exact-main hosted run
[`30422838559`](https://github.com/xicv/minco/actions/runs/30422838559).
The next authorised live rehearsal migrated and verified its disposable
private PostgreSQL database and built the native ARM64 Lambda, then failed
tagged Cognito user-pool creation because the bounded deployment role lacked
`cognito-idp:TagResource`. The run subsequently produced all-true application,
database/VPC/secret and bootstrap-IAM cleanup receipts. The candidate
least-privilege correction permits that action only for the current
Region/account user-pool namespace and the exact three run-ownership tags; the
local regression renders and compares the complete generated statement. AWS
IAM simulation returned `allowed` for the exact tag set and `implicitDeny` when
an additional tag key was supplied. No tag or registry upload occurred.

That correction passed PR-head hosted run
[`30425328469`](https://github.com/xicv/minco/actions/runs/30425328469),
merged as exact `main`
`cd5b0049cd55f3ba7093a202eff9b668c825ed0b`, and passed the full local suite,
AWS/SAM validation and exact-main hosted run
[`30426089277`](https://github.com/xicv/minco/actions/runs/30426089277).
Authorised replacement run `20260729t060221z-approved` migrated and verified
its disposable private PostgreSQL database and sealed the native ARM64
artifact, then stopped before application change-set creation because AWS CLI
shorthand parsed comma-delimited Lambda subnet IDs as a list where
CloudFormation requires a string `ParameterValue`. All application,
database/VPC/secret and bootstrap-IAM cleanup receipts are true. The candidate
fix serializes deployment and promotion parameters as one JSON list; its
focused regression and the AWS CLI `2.36.10` non-contacting output-skeleton
validator preserve comma-delimited values as strings. The live replacement,
tag and registry publication remain blocked pending merge and exact-main
requalification.

The JSON-parameter correction passed PR-head hosted run
[`30428780397`](https://github.com/xicv/minco/actions/runs/30428780397), merged
as exact `main` `100ffa276163a2c02149321b2b7ffcc542edb4c5`, and passed the
full local suite, AWS/SAM validation and exact-main hosted run
[`30429829246`](https://github.com/xicv/minco/actions/runs/30429829246).
Authorised replacement run `20260729t071107z-approved` migrated and verified
its disposable private PostgreSQL database, built the 5,038,349 byte native
ARM64 artifact and created the application change set, then stopped
fail-closed because the real `describe-change-set` response omitted
`ChangeSetType`. That field is guarded create input and is not a documented
`DescribeChangeSet` response element. Exact inspection also found an empty,
untagged `REVIEW_IN_PROGRESS` shell that the tag-only cleanup guard correctly
refused. The shell contained one unexecuted change set and zero resources; both
were deleted, the RDS-managed secret reached `ResourceNotFound`, and the exact
repository cleanup verifiers produced all-true application,
database/VPC/secret and bootstrap-IAM receipts.

The candidate parser requires the already-guarded expected type, uses it in the
redacted immutable review and rejects an optional contradictory provider
value. Cleanup may delete an untagged review shell only when the exact stack
was proven absent before the run, remains in `REVIEW_IN_PROGRESS` and has zero
resources. Focused red/green tests cover the real provider shape and every
cleanup refusal boundary. A replacement live rehearsal, tag and registry
publication remain blocked pending merge and exact-main requalification.
