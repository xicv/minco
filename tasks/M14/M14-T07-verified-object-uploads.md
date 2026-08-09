---
id: M14-T07
title: Add a verified direct-object upload lifecycle
milestone: M14
status: active
priority: high
area: plugins/object-storage
depends_on: [M14-T02]
operations: []
owned_paths:
  - Cargo.lock
  - docs/DECISIONS.md
  - docs/adrs/0035-verified-direct-object-uploads.md
  - docs/how-to/object-uploads.md
  - docs/research/object-storage-review-2026-08.md
  - extensions/minco-aws-adapters/Cargo.toml
  - extensions/minco-aws-adapters/README.md
  - extensions/minco-aws-adapters/src/lib.rs
  - extensions/minco-aws-adapters/src/s3.rs
  - extensions/minco-aws-adapters/src/s3_storage.rs
  - extensions/minco-aws-adapters/tests/real_aws.rs
  - extensions/minco-aws-adapters/tests/real_aws_s3.rs
  - extensions/minco-aws-adapters/tests/rustack.rs
  - plugins/minco-plugin-object-storage/Cargo.toml
  - plugins/minco-plugin-object-storage/README.md
  - plugins/minco-plugin-object-storage/src/base.rs
  - plugins/minco-plugin-object-storage/src/lib.rs
  - plugins/minco-plugin-object-storage/src/uploads.rs
  - scripts/aws/run-adapter-smoke.sh
  - tasks/M14/M14-T07-verified-object-uploads.md
  - tasks/M14/M14-T08-object-upload-profiles.md
  - verification/source-manifest.json
  - verification/deep-review.json
  - verification/static-validation.json
checks:
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/source_manifest.py --check
  - rustfmt --edition 2024 --check plugins/minco-plugin-object-storage/src/lib.rs plugins/minco-plugin-object-storage/src/uploads.rs extensions/minco-aws-adapters/src/lib.rs extensions/minco-aws-adapters/src/s3.rs extensions/minco-aws-adapters/src/s3_storage.rs extensions/minco-aws-adapters/tests/rustack.rs extensions/minco-aws-adapters/tests/real_aws_s3.rs
  - cargo check -p minco-plugin-object-storage --all-targets --locked
  - cargo test -p minco-plugin-object-storage --locked
  - cargo clippy -p minco-plugin-object-storage --all-targets --locked -- -D warnings
  - cargo check -p minco-aws-adapters --all-targets --features s3 --locked
  - cargo test -p minco-aws-adapters --features s3 --locked
  - cargo test -p minco-aws-adapters --features s3 --test real_aws_s3 --locked --no-run
  - cargo clippy -p minco-aws-adapters --all-targets --features s3 --locked -- -D warnings
  - cargo doc -p minco-plugin-object-storage -p minco-aws-adapters --features minco-aws-adapters/s3 --no-deps --locked
---

## Goal

Turn the existing object storage and S3 signing primitives into one safe,
low-idle-cost everyday upload workflow without breaking the published 1.x API,
proxying browser bytes through Lambda, or adding hidden compute/schedules.

## Acceptance

- research compares the current code with Laravel 13, OWASP, current S3, Lambda,
  checksum, conditional-write, CORS, ownership, and multipart guidance before
  implementation;
- application policy owns an exact prefix, media allowlist, maximum bytes, and
  short expiry;
- upload issuance generates an extensionless non-user-controlled key and signed
  upload identity;
- the client bearer grant and trusted pending state are distinct types, and the
  pending record contains no URL, policy, signature, or temporary token;
- managed upload signing binds exact content type, exact byte count, SHA-256,
  expiry, encryption, and attributes; S3 rejects single POST above 5 GiB;
- completion verifies provider metadata and checksum without downloading the
  object;
- memory and S3 implement the metadata port, and S3 composes store, private
  download signer, upload signer, and `HeadObject` reader from one exact
  configuration;
- signed values remain redacted and the documentation distinguishes transfer
  integrity from content inspection;
- no fixed compute, NAT Gateway, provisioned concurrency, hidden schedule,
  database mutation, AWS mutation, deployment, or release is introduced; and
- formatting checks name only modified Rust files; package-scoped compiler,
  test, and Clippy gates do not rewrite source files.
- one managed service exposes one exact purpose-specific policy; statically
  composed multiple product profiles remain the dependent M14-T08 task rather
  than widening this task's authorization and configuration surface.

## Compatibility

The old `ObjectStore`, `ObjectStoragePlugin`, `ObjectAccessSigner`, S3 adapter,
and presigned request types remain available through the same crate root. The
new managed plugin, exact upload signer, and metadata/upload capabilities are
opt-in additive APIs.

## Evidence

The senior hardening pass started on 2026-08-09 in isolated Git worktree
`minco-pr130-storage-hardening` from exact PR head
`0b3df64cf60183c801d4e0a97801dc4069d7ead6`. The earlier hosted run
[31147869166](https://github.com/xicv/minco/actions/runs/31147869166) qualifies
only that pre-hardening head and is not final evidence.

Current local evidence on pinned Rust 1.97.1:

- static validation reports zero errors and zero warnings across 86 tasks and
  176 Rust files;
- targeted rustfmt checks only the seven modified Rust files;
- `minco-plugin-object-storage` compiles, passes 10 tests, and passes
  warning-denying Clippy;
- `minco-aws-adapters` with `s3` compiles, passes 15 tests with the ignored
  real-S3 test separately compiled, and passes warning-denying Clippy;
- public rustdoc for both packages passes with `RUSTDOCFLAGS=-D warnings`;
- Rustack passes S3/SQS/SSM/STS transport plus managed issue/POST and proves the
  emulator checksum gap fails closed; and
- the bounded real-S3 test was not executed because the required AWS
  environment was absent. It creates no bucket and uses only run-owned keys in
  a pre-existing bucket when explicitly invoked.

The repository-wide quality script passed static, repository, release,
database, browser (40 tests), snippet (252 blocks), and preceding checks before
stopping at the unchanged docs-site lockfile's current `nanoid <3.3.17`
`GHSA-2v37-7h3g-55p8` audit. The exact PR base contains the same package and
lock manifests; the storage task does not silently change that unrelated
baseline dependency.

Final cross-PR integration, exact-head hosted qualification, and source
manifest evidence are appended before this task moves from `active` to
`complete`. No real AWS call, deployment, database mutation, release, fixed
compute, NAT Gateway, provisioned concurrency, or hidden schedule is authorized
by this task.
