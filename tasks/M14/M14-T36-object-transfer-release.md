---
id: M14-T36
title: Harden object transfers and release Minco 1.8.0
milestone: M14
status: completed
priority: critical
area: plugins/object-storage/release
depends_on: [M14-T35]
operations:
  - initiateObjectUpload
  - issueObjectUploadPart
  - completeObjectUpload
  - abortObjectUpload
  - getObjectTransferMetadata
  - issueObjectDownload
owned_paths:
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - Cargo.lock
  - Cargo.toml
  - PUBLISHING.md
  - README.md
  - REVIEW_STATUS.md
  - SECURITY.md
  - VERIFICATION.md
  - crates/minco-cli/assets/agent/**
  - crates/minco-cli/src/agent_cmd.rs
  - crates/minco-cli/src/delivery_evidence.rs
  - crates/minco-cli/src/handover_cmd.rs
  - docs-site/.vitepress/config.mts
  - docs-site/1.8.0/**
  - docs-site/next/**
  - docs-site/release.json
  - docs-site/versions.md
  - docs/adoption/1.7.0-to-1.8.0.md
  - docs/adoption/incremental-adoption.md
  - docs/adrs/0035-verified-direct-object-uploads.md
  - docs/adrs/0045-resumable-direct-object-transfers.md
  - docs/development/publishing.md
  - docs/development/using-minco-crate.md
  - docs/how-to/object-transfers.md
  - docs/reference/compatibility.md
  - docs/reference/generated/**
  - docs/reference/supported-matrix.md
  - docs/vision/minco-framework-definition.md
  - examples/orders/api/src/generated.rs
  - examples/plugins/third-party-minimal/Cargo.lock
  - extensions/**/minco-plugin.json
  - infra/aws/generated/**
  - plugins/**/minco-plugin.json
  - plugins/minco-plugin-object-storage/**
  - plugins/minco-plugin-payments-waffo/agent/**
  - plugins/minco-plugin-payments-waffo/README.md
  - proofs/realtime-appsync/aws-handler/Cargo.lock
  - quality.toml
  - roadmap/tasks.mmd
  - scripts/source_manifest.py
  - scripts/test/candidate_qualification.py
  - scripts/test/operational_evidence.py
  - scripts/test/quality_assurance.py
  - scripts/test/repository_truth.py
  - scripts/validate_operational_evidence.py
  - tasks/M14/M14-T36-object-transfer-release.md
  - verification/1.8-candidate-load.json
  - verification/1.8-candidate-recovery.json
  - verification/1.8-performance-baseline.json
  - verification/agent-workflows.json
  - verification/deep-review.json
  - verification/deployment-assurance.toml
  - verification/operational-evidence-validation.json
  - verification/performance-policy.toml
  - verification/provider-evidence.toml
  - verification/publish-validation.json
  - verification/quality-assurance-policy.toml
  - verification/quality-assurance.json
  - verification/release-identity.json
  - verification/repository-truth.toml
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p minco-plugin-object-storage --all-features --locked
  - cargo clippy -p minco-plugin-object-storage --all-targets --all-features --locked -- -D warnings
  - cargo test -p minco-aws-adapters --features s3 --locked
  - cargo clippy -p minco-aws-adapters --all-targets --features s3 --locked -- -D warnings
  - cargo generate-lockfile
  - cargo test -p cargo-minco --test agent_skills --locked
  - uv run --locked python scripts/test/agent_workflows.py --check-output verification/agent-workflows.json
  - uv run --locked python scripts/test/repository_truth.py
  - uv run --locked python scripts/validate_static.py
  - scripts/docs/generate-reference.sh --check
  - scripts/docs/build.sh
  - scripts/docs/check-links.sh
  - scripts/docs/check-snippets.sh
  - scripts/docs/test-browser.sh
  - scripts/quality.sh
  - scripts/ci/local-release.sh
  - uv run --locked python scripts/source_manifest.py --check
---

# M14-T36 - Harden object transfers and release Minco 1.8.0

## Goal

Close the release-blocking HTTP and trusted-state gaps in the new object
transfer control plane, then qualify and publish the complete lock-step Minco
`1.8.0` family from one exact reviewed source tree.

## Acceptance

- the maximum valid 10,000-part S3 completion manifest fits the HTTP control
  plane and Lambda synchronous event boundary without relaxing provider receipt
  validation;
- provider multipart entity tags are bounded to the reviewed HTTP contract;
- authorized metadata implements RFC weak `If-None-Match` comparison for lists
  and `*`, while invalid application entity tags fail closed;
- persisted single-upload state is revalidated against its configured prefix,
  content policy, size, checksum, generated upload identity and attributes
  before provider contact;
- direct browser/mobile byte transfer, range resume, private caching,
  quarantine, immutable update and structural cost boundaries remain explicit;
- all 34 publishable packages and official descriptors advance in lock-step to
  additive `1.8.0`, with frozen documentation and adoption guidance;
- exact local quality, local release, immutable security review and clean-Linux
  compatibility pass before merge and tagging; and
- registry, GitHub release, Pages, docs.rs, provider, deployment and production
  evidence remain separate truthful states.

## Non-goals

- proxying large file bodies through Lambda or API Gateway;
- moving application authorization, quotas, durable session persistence,
  logical pointer updates, retention or content inspection into the plugin;
- enabling CloudFront, Transfer Acceleration, a scanner, scheduler, fixed
  compute, NAT Gateway or provisioned concurrency by default; or
- claiming current AWS prices, live-provider conformance, deployment, SLOs or
  content safety without separate evidence.

## Starting evidence

The task starts from merged `main`
`9e4e4c2b5b8e35457d4d45f94b4114236a775069`. PR #167's source tree was
security-reviewed with zero findings and is byte-identical to its branch head.
The PR was merged before its branch compatibility run completed, so exact
merged-main clean-Ubuntu run `31766443382` was added and passed before this task
started. No AWS request, deployment or production mutation was performed.

## Evidence

The exact sealed source passes `scripts/quality.sh` and the authoritative
`MINCO_QUALITY_TOOL_ROOT=/Users/xicao/.cargo scripts/ci/local-release.sh` from
an empty JJ child. Pinned assurance records 127 nextest tests plus one doctest,
85.78% line and 81.97% function coverage, 43 caught viable mutants with zero
misses/timeouts, and additive compatibility for all 34 packages against
immutable `v1.7.0`.

Candidate load passes 80/80 loopback API requests and 1,000/1,000 synthetic
worker messages. Candidate recovery passes repeatable migration, backup,
restore, application-read and rollback-contract checks. The clean release
matrix also passes AppSync local proof, all 34 package archive dry-runs,
selected unpacked-archive consumers, Plan/SAM validation, native Lambda and
worker builds, owned PostgreSQL and Rustack runtime qualification, and Orders
E2E.

The focused object-storage slice passes 31 plugin tests plus the added UUIDv7
pending-state case, HTTP contract tests for the 10,000-part maximum and cache
validators, and 20 S3 adapter tests; one real-AWS test remains explicitly
ignored because no disposable provider target was configured. Targeted Clippy
passes with warnings denied.

These are bounded provider-free/local results, not hosted Linux, AWS,
deployment, production or SLO proof. No tag, GitHub release, crates.io upload,
live provider contact, deployment or production mutation occurred while
preparing the candidate.
