---
id: M14-T34
title: Add resumable HTTP and mobile object transfers
milestone: M14
status: complete
priority: high
area: plugins/object-storage
depends_on: [M14-T33]
operations:
  - initiateObjectUpload
  - issueObjectUploadPart
  - completeObjectUpload
  - abortObjectUpload
  - getObjectTransferMetadata
  - issueObjectDownload
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0045-resumable-direct-object-transfers.md
  - docs/how-to/object-transfers.md
  - docs/research/object-transfers-2026-08.md
  - docs/reference/generated/plugins.md
  - extensions/minco-aws-adapters/Cargo.toml
  - extensions/minco-aws-adapters/minco-plugin.json
  - extensions/minco-aws-adapters/src/iam.rs
  - extensions/minco-aws-adapters/src/s3.rs
  - extensions/minco-aws-adapters/src/s3_storage.rs
  - extensions/minco-aws-adapters/tests/real_aws_s3.rs
  - plugins/minco-plugin-object-storage/Cargo.toml
  - plugins/minco-plugin-object-storage/README.md
  - plugins/minco-plugin-object-storage/minco-plugin.json
  - plugins/minco-plugin-object-storage/openapi/**
  - plugins/minco-plugin-object-storage/src/**
  - plugins/minco-plugin-object-storage/tests/**
  - tasks/M14/M14-T34-http-object-transfers.md
  - verification/1.7-performance-baseline.json
  - verification/operational-evidence-validation.json
  - verification/release-identity.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/source_manifest.py --check
  - cargo check -p minco-plugin-object-storage --all-targets --all-features --locked
  - cargo test -p minco-plugin-object-storage --all-features --locked
  - cargo clippy -p minco-plugin-object-storage --all-targets --all-features --locked -- -D warnings
  - cargo check -p minco-aws-adapters --all-targets --features s3 --locked
  - cargo test -p minco-aws-adapters --features s3 --locked
  - cargo clippy -p minco-aws-adapters --all-targets --features s3 --locked -- -D warnings
---

# M14-T34 - Add resumable HTTP and mobile object transfers

## Goal

Make the object-storage plugin a production-shaped HTTP control plane for
authorized upload, update and download lifecycles while keeping large bytes on
direct private provider paths. Add provider-neutral range streaming,
checksummed multipart transfer, explicit validation/quarantine state and
structural cost evidence, with S3 as the qualified AWS implementation.

## Acceptance

- HTTP handlers call one injected application use case, require an authenticated
  principal, preserve request IDs and never accept a raw provider key as business
  authorization;
- ordinary bounded uploads retain the verified single-request path while large
  uploads use durable multipart sessions with exact part sizes, checksums,
  idempotent part replacement, ordered completion and explicit abort;
- incomplete multipart bytes have a documented retention/lifecycle fallback and
  remain a visible request/storage cost;
- downloads support metadata-only HEAD semantics, strong validators, one exact
  byte range, short-lived direct grants, safe filenames and private cache policy;
- the provider-neutral read port streams chunks without collecting a whole
  object, and dropping it is the in-process cancellation boundary;
- mobile clients can stop and resume by retaining the object validator and
  requesting a fresh range grant; grant expiry and reconnect behavior are
  explicit;
- updates publish a new immutable generated object key and conditionally replace
  an application-owned reference; the storage plugin does not overwrite a
  cache-visible key or invent generic CRUD persistence;
- MIME/size/checksum verification remains distinct from content safety, and an
  untrusted upload cannot become downloadable until the application records an
  accepted inspection verdict;
- a structural cost model exposes storage, request, incomplete-part, egress,
  optional acceleration and optional edge-cache dimensions without embedding
  moving AWS prices or claiming a production bill;
- the minimal AWS profile relays no file bodies through Lambda or API Gateway and
  adds no NAT Gateway, fixed compute, schedule or provisioned concurrency;
- memory and S3 adapters pass contract, stream/range, multipart, redaction,
  failure and cost tests; and
- bounded ignored real-S3 conformance remains opt-in, uses a pre-existing bucket,
  cleans exact test keys/uploads and is reported separately from local proof.

## Non-goals

- moving tenant ownership, quotas, retention decisions or business
  authorization into the framework;
- claiming antivirus, safe image decoding, content disarm/reconstruction or
  another inspection implementation merely from MIME, filename or checksum;
- making CloudFront, Transfer Acceleration, S3 Express, cross-Region replication
  or an always-on scanner part of the minimal profile;
- proxying tus/IETF resumable-upload byte PATCH requests through the default
  Lambda HTTP API;
- changing M14-T09's planned static multi-profile selection contract; or
- contacting AWS, creating a bucket, deploying an application or mutating
  production during local qualification.

## Evidence

The repository began this task at `main@origin`
`43c263b54f880cbf64ecb9c2f299c7e788c479c7` in the dedicated JJ workspace
`/Users/xicao/Projects/minco-task-m14-t34`. Existing single-request managed
upload tests passed. The S3-only adapter test lane failed to compile because its
ignored real-AWS test imports `aws_config` without the `s3` feature enabling that
dependency; the all-features lane passed locally and made no AWS calls. This
task retains that failure as a regression to close rather than treating ignored
provider tests as executed evidence.

The task closed that feature-boundary regression and added six authenticated
control-plane operations, provider-neutral streaming/range/multipart contracts,
an in-memory conformance implementation and an AWS SDK for Rust S3 adapter. The
application remains responsible for business authorization, quotas, durable
session persistence, logical-reference conditional updates and content
inspection decisions. Upload bytes remain quarantined until the application
records an accepted verdict; provider keys and opaque multipart identifiers are
never accepted as business authorization.

On 2026-08-14 the following exact local source gates passed:

- `cargo test -p minco-plugin-object-storage --all-features --locked`: 25 tests
  passed across unit, HTTP, transfer-contract and fake-port suites;
- `cargo clippy -p minco-plugin-object-storage --all-targets --all-features
  --locked -- -D warnings`;
- `cargo test -p minco-aws-adapters --features s3 --locked`: the S3 unit lane
  passed and the bounded real-S3 test remained ignored;
- `cargo clippy -p minco-aws-adapters --all-targets --features s3 --locked --
  -D warnings`;
- `uv run --locked python scripts/validate_static.py`;
- `cargo minco plugin validate`, which returned no findings; and
- `./scripts/quality.sh`, including static, contract, generated-reference,
  repository-truth, documentation, browser, workspace format, Clippy, test,
  generated-application, Rustdoc, dependency-policy, advisory, secret-scan and
  source-manifest gates.

The exact-tree performance record remains truthfully `NOT RUN`, and operational
evidence reports `PERF-BASELINE-007` and `EVIDENCE-PROVIDER-021` warnings. No AWS
account was contacted, no bucket or multipart upload was created, no hosted
Linux performance baseline was measured, no application was deployed and no
production price or SLO is claimed. The ignored real-S3 conformance test now
covers create, exact checksummed part upload, ordered completion, ranged read,
abort and cleanup against an explicitly supplied disposable bucket.
