---
id: M5-T01
title: Build deploy and verify the real AWS development stack
milestone: M5
status: complete
priority: critical
area: deployment/aws
depends_on: [M1-T02, M3-T01, M4-T01]
operations: [getLive, getReady, placeOrder, getOrder]
owned_paths:
  - infra/aws/**
  - scripts/aws/**
  - crates/minco-release/src/lib.rs
  - crates/minco-cli/src/main.rs
  - crates/minco-plan/src/sam.rs
  - crates/minco-plan/src/lib.rs
  - extensions/minco-aws-lambda/src/lib.rs
  - docs/deployment/aws-minimal.md
  - docs/deployment/release.md
  - docs/deployment/real-aws-smoke.md
  - tasks/M5/M5-T01-real-aws-dev.md
checks:
  - scripts/aws/build-lambda.sh
  - scripts/aws/validate.sh
  - scripts/aws/deploy.sh
  - scripts/aws/run-bounded-root-bootstrap.sh
---

## Goal

Build one native ARM64 ZIP, deploy it behind API Gateway HTTP API, use an
existing pooled PostgreSQL URL from SSM or a bounded disposable RDS PostgreSQL
instance, and retain hosted verification and cleanup evidence.

## Safety

This task is intentionally not marked complete until it runs in a reviewed AWS account with no secrets in committed output.

## Current evidence

Implemented and locally verified:

- release schema 2 stores repository-relative digests for the native ZIP, Plan
  IR, rendered SAM template, contract, lockfile and migrations; verification
  rejects unsupported schemas, paths outside the repository and digest changes;
- SAM `CodeUri` is relative to the rendered template location, including a
  regression test for the nested `infra/aws/generated` output;
- the Lambda role has exact-parameter `ssm:GetParameter` and no wildcard KMS
  grant; a customer-managed key adds exact-key, SSM-service and parameter
  encryption-context restrictions;
- deployment verifies the release, rejects placeholder JWT issuers and
  pre-existing targets, uses a private encrypted one-day fallback artifact
  bucket, retains an unexecuted create-only change set, restricts it to the six
  expected SAM-transformed resource types and only then executes;
- the bounded smoke uses a temporary Cognito Lite identity with immutable
  permissions, verifies live/ready/auth/place/get/replay and the deployed
  Lambda digest, then deletes synthetic rows and all temporary AWS resources;
- every top-level AWS, SAM, API Gateway HTTP and external PostgreSQL touch is
  journaled under ignored `target/minco/aws/<run-id>/`;
- the approved root bootstrap path validates a run-scoped role policy and
  exact-role assumption policy, creates a temporary bootstrap user restricted
  to that role, uses an isolated one-hour role session, creates a run-owned SSM
  `SecureString`, and proves removal of the key, user, role, profiles and local
  credentials. API Gateway and general VPC mutations are usable only through
  CloudFormation forward-access calls; direct Cognito and security-group
  mutations require all three exact run ownership tags;
- scoped Rust tests and Clippy pass, ShellCheck passes, static validation has
  zero findings, SAM lint passes, the native ARM64 ZIP builds, and Rustack
  S3/SQS/SSM/STS plus the real Minco SSM SDK adapter pass locally.

Real-cloud touch record for 2026-07-24:

- one `sam local invoke` credential-resolution probe failed because the cached
  default login had expired; no authenticated service API or resource operation
  succeeded;
- two `aws login` browser authorizations expired without callbacks; no account
  API or resource operation succeeded;
- one journaled `sts:GetCallerIdentity` succeeded through the existing
  `garmentiq-demo-deploy` profile. It identified a project-specific IAM user,
  so it was deliberately not used for Minco;
- no SSM value was read and no IAM, KMS, Cognito, S3, CloudFormation, Lambda,
  API Gateway, CloudWatch Logs or database mutation was performed.

The project-specific deploy profile was not reused.

Real-cloud touch record for 2026-07-25:

- the approved root identity gate succeeded and the existing standard RDS
  service-linked role was verified without modification;
- root could not assume the temporary role directly, so that role was deleted
  and proved absent. The permanent bootstrap now creates a minimal user that
  can assume only the exact run-owned role;
- IAM rejected application permissions on the bootstrap user at its aggregate
  2,048-character inline-policy quota. That user was deleted and the permanent
  design keeps only the small assumption policy on the user and the bounded
  application policy on the role;
- bounded IAM propagation retries were needed for the trust principal, new
  access key and role policy. Every attempt is journaled, and every failed
  bootstrap proved the key, user, role and local credential files absent;
- the disposable RDS path exercised and permanently fixed three fail-closed
  boundaries: URI use through `PGDATABASE`, the generic RDS waiter returning
  before public-access modification propagated, and stale prior-run cleanup
  markers. Each failed database/VPC attempt was deleted and independently
  proved absent before the next attempt;
- CloudTrail showed that tagged API Gateway stage creation required the
  runtime-only `TagResource` authorization name even though Access Analyzer
  rejects that explicit action. The final policy uses an Analyzer-clean
  `apigateway:*` statement restricted to the stage-collection ARN and all three
  Minco ownership request tags; IAM simulation and the real deployment passed.
  Deployment failures now retain CloudFormation stack events before automatic
  cleanup, avoiding a second diagnostic cloud pass;
- the final bounded run created an encrypted single-AZ 20 GiB RDS PostgreSQL
  instance, migrated and verified it with TLS `verify-full`, revoked the
  operator `/32`, made the instance private, created the SSM interface endpoint
  without NAT, and deployed the exact native ARM64 release;
- the hosted smoke passed exact digest/runtime verification, live, ready,
  unauthenticated rejection, authenticated place/get and idempotent replay;
- CloudFormation deleted the explicit log group before the VPC Lambda finished
  and the runtime recreated it. The verifier caught the orphan. Cleanup now
  proves the function absent, deletes only that exact log group if present and
  polls for absence;
- focused review closed response-loss cleanup windows: deterministic, exactly
  tagged IAM users, IAM roles and Cognito pools are rediscovered before
  teardown even when a successful create response was not saved. Every access
  key found on the exact run-owned bootstrap user is deleted before the user;
- focused review also made schema-2 release paths strictly normalized and
  repository-relative, added multi-Region `mrk-` KMS key ARN support, paired
  VPC parameters in the template itself, allowlisted the six transformed app
  resource types and retained create-failure stack events before cleanup;
- destructive recovery is now ownership-bound: app and RDS stacks, Cognito
  pools and SSM parameters require the exact managed, purpose and run-ID tags.
  Focused review found that a separate S3 tagging request left a response-loss
  cleanup gap. The final path uses the current general-purpose `CreateBucket`
  tag support so all three ownership tags are atomic and mandatory in IAM;
  cleanup has no untagged-bucket exception. Recorded good tags pass and
  mismatched tags fail the local deletion-gate regression;
- the hardened bootstrap policy was revalidated through a journaled, read-only
  Access Analyzer call after cleanup with zero errors, warnings or suggestions.
  Journaled IAM simulation independently returned `implicitDeny` for a direct
  API Gateway mutation and `allowed` only with the CloudFormation
  `aws:CalledVia` context;
- focused review then replaced the separate S3 create/tag calls with atomic
  tagged `CreateBucket`. A local validation attempt under Zsh failed before any
  cloud call because the journal helper requires Bash. The Bash retry made one
  journaled root identity check, then stopped locally on the retained IAM
  response wrapper without calling Analyzer. The corrected journaled,
  read-only Analyzer call reported zero errors, warnings or suggestions for
  `s3:CreateBucket` plus `s3:TagResource` with the three exact request tags;
- the new offline regional/`us-east-1` atomic-tag regression first failed on
  jq boolean precedence in the assertion itself. Explicit grouping fixed the
  test, which then passed both payload shapes without a cloud call;
- focused IAM review found that the initial RDS-managed-secret wildcard let the
  temporary role retrieve any RDS-managed secret in the account and Region.
  The final policy permits secret creation/tagging only through an RDS
  forward-access session and permits retrieval, description and orphan cleanup
  only when the secret's owning-service tag is `rds` and its
  `aws:rds:primaryDBInstanceArn` tag equals the exact disposable DB ARN.
  Analyzer accepted both tag-key spellings, but IAM simulation rejected a
  file-encoded policy input and then did not match the service-prefixed
  resource-tag context. The corrected compact input and global
  `aws:ResourceTag/...` form produced zero Analyzer findings, `allowed` for the
  exact DB tag, `implicitDeny` for an unrelated DB tag and direct secret
  creation, and `allowed` for RDS-forwarded creation. All attempts are
  journaled and were read-only;
- recovery cleanup then proved the app stack, HTTP API, Lambda, execution role,
  log group, Cognito pool, artifact bucket, SSM parameter, RDS instance,
  managed secret, VPC, bootstrap key/user/role and all local credential files
  absent. `final-cleanup.json` contains only `true` values.

The ignored evidence directory is
`target/minco/aws/20260725t001746z-rds/`. Its append-only
`cloud-touches.jsonl` records every real AWS, SAM, HTTP and PostgreSQL touch,
including failed attempts and cleanup. Committed evidence intentionally omits
account IDs, ARNs, resource IDs, IP addresses, URLs, tokens and secret values.

Final local gates after the permanent fixes:

- modified Rust files pass `rustfmt --check`;
- targeted `cargo test` and all-feature `cargo clippy -D warnings` pass for
  `cargo-minco`, `minco-plan`, `minco-release` and `minco-aws-lambda`;
- ShellCheck passes for every changed AWS script;
- `scripts/aws/validate.sh` reports zero static findings and a valid SAM
  template, including the secure PostgreSQL URL-to-conninfo regression check;
- post-review Access Analyzer validation reports zero findings for the
  forward-access and exact-tag IAM boundaries;
- the disposable RDS CloudFormation template passes SAM/cfn lint;
- Rustack passes S3, SQS, SSM, STS and the real Minco SSM SDK adapter.
- the repository-wide local quality suite passes static/publish validation,
  Feedback contract checks, formatting checks, the feature matrix, strict
  workspace Clippy, all workspace tests, rustdoc and documentation generation.
  Its two deep-review unwrap-count advisories are pre-existing heuristic
  warnings rather than compiler, test or security failures;
- an ad-hoc exact-script lint wrapper used Bash 4-only `mapfile` on macOS Bash
  3 and, without `errexit`, incorrectly reported that zero files passed. That
  output is not accepted as evidence. A portable read-loop wrapper with
  `set -euo pipefail` then ran Bash syntax and ShellCheck successfully on all
  13 modified AWS scripts;
- the native cargo-lambda build passes and produces the expected ARM64 ZIP.
  Rust 1.97 also surfaces one upstream Zig linker diagnostic:
  `ignoring deprecated linker optimization setting '1'`. Minco supplies only
  `-C target-cpu=neoverse-n1`; the deprecated `-Wl,-O1` originates in the
  cargo-lambda/cargo-zigbuild/Zig toolchain. It is recorded rather than
  suppressed so unrelated linker diagnostics remain visible. Removal is
  blocked on an upstream cargo-zigbuild/cargo-lambda update and is not a
  product-build failure.

The focused post-implementation review is complete with no unresolved finding.
