---
id: M14-T06
title: Add a verified direct-object upload lifecycle
milestone: M14
status: complete
priority: high
area: plugins/object-storage
depends_on: [M14-T02]
operations: []
owned_paths:
  - Cargo.lock
  - docs/DECISIONS.md
  - docs/adrs/0034-verified-direct-object-uploads.md
  - docs/how-to/object-uploads.md
  - docs/research/object-storage-review-2026-08.md
  - extensions/minco-aws-adapters/README.md
  - extensions/minco-aws-adapters/src/lib.rs
  - extensions/minco-aws-adapters/src/s3.rs
  - extensions/minco-aws-adapters/src/s3_storage.rs
  - plugins/minco-plugin-object-storage/Cargo.toml
  - plugins/minco-plugin-object-storage/README.md
  - plugins/minco-plugin-object-storage/src/base.rs
  - plugins/minco-plugin-object-storage/src/lib.rs
  - plugins/minco-plugin-object-storage/src/uploads.rs
  - tasks/M14/M14-T06-verified-object-uploads.md
  - verification/source-manifest.json
checks:
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/source_manifest.py --check
  - rustfmt --edition 2024 --check plugins/minco-plugin-object-storage/src/lib.rs plugins/minco-plugin-object-storage/src/uploads.rs extensions/minco-aws-adapters/src/lib.rs extensions/minco-aws-adapters/src/s3.rs extensions/minco-aws-adapters/src/s3_storage.rs
  - cargo check -p minco-plugin-object-storage --all-targets --locked
  - cargo test -p minco-plugin-object-storage --locked
  - cargo clippy -p minco-plugin-object-storage --all-targets --locked -- -D warnings
  - cargo check -p minco-aws-adapters --all-targets --features s3 --locked
  - cargo test -p minco-aws-adapters --features s3 --locked
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

## Compatibility

The old `ObjectStore`, `ObjectStoragePlugin`, `ObjectAccessSigner`, S3 adapter,
and presigned request types remain available through the same crate root. The
new managed plugin, exact upload signer, and metadata/upload capabilities are
opt-in additive APIs.

## Evidence

Clean GitHub-hosted qualification on pinned Rust 1.97.1 is recorded by Actions
run [31147869166](https://github.com/xicv/minco/actions/runs/31147869166). Static
validation reports zero errors and zero warnings; the deterministic source
manifest and targeted rustfmt check pass. `minco-plugin-object-storage`
compiles, passes 9 tests, and passes warning-denying Clippy.
`minco-aws-adapters` with `s3` compiles, passes 11 tests, and passes
warning-denying Clippy. Public rustdoc for both packages passes with
`RUSTDOCFLAGS=-D warnings`.

The run makes no real AWS call or mutation. Final changed-file review contains
no deployment plan, database, release, fixed compute, NAT Gateway, provisioned
concurrency, hidden schedule, or unrelated formatting change.
