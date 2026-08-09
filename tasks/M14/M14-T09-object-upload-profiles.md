---
id: M14-T09
title: Add statically composed object-upload profiles
milestone: M14
status: planned
priority: medium
area: plugins/object-storage
depends_on: [M14-T08]
operations: []
owned_paths:
  - docs/adrs/**
  - docs/how-to/object-uploads.md
  - docs/research/object-storage-review-2026-08.md
  - extensions/minco-aws-adapters/src/s3_storage.rs
  - extensions/minco-aws-adapters/tests/real_aws_s3.rs
  - plugins/minco-plugin-object-storage/README.md
  - plugins/minco-plugin-object-storage/src/uploads.rs
  - tasks/M14/M14-T09-object-upload-profiles.md
  - verification/source-manifest.json
checks:
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/source_manifest.py --check
  - cargo check -p minco-plugin-object-storage --all-targets --locked
  - cargo test -p minco-plugin-object-storage --locked
  - cargo clippy -p minco-plugin-object-storage --all-targets --locked -- -D warnings
  - cargo check -p minco-aws-adapters --all-targets --features s3 --locked
  - cargo test -p minco-aws-adapters --features s3 --locked
  - cargo clippy -p minco-aws-adapters --all-targets --features s3 --locked -- -D warnings
---

## Goal

Let one statically composed application expose multiple named direct-upload
policies, such as avatars, images, documents, and attachments, without a broad
union policy, global service locator, runtime plugin scanning, or hidden
infrastructure.

## Acceptance

- a validated typed profile identifier selects an immutable policy installed at
  the composition root, never a provider or unrestricted policy from request
  data;
- each profile owns an exact generated-key prefix, content-type allowlist,
  maximum byte count, capability lifetime, and documented cost/resource intent;
- profile selection is explicit in the application use case after business
  authorization; the framework does not infer tenant, owner, quota, or purpose;
- the issued trusted pending record preserves the selected profile identity so
  completion cannot verify under a weaker or different policy;
- plugin descriptors and `minco inspect` expose deterministic profile intent
  without advertising one capability per tenant or discovering services at
  runtime;
- at least two materially different profiles prove the extension point through
  memory, S3 policy, composition, and fail-before-signing tests;
- duplicate profile IDs, overlapping or invalid prefixes, empty allowlists,
  provider-invalid size limits, and unknown profiles fail during composition or
  before signing;
- public API compatibility is additive and documented, including migration from
  the one-policy `ObjectUploadService`; and
- bounded real-S3 conformance proves profile-specific key, media, size,
  checksum, metadata, and cleanup behavior without creating a bucket or using
  `GetObject`.

## Non-goals

- moving application authorization, tenant ownership, quota, or content-safety
  decisions into Minco;
- selecting arbitrary buckets, credentials, endpoints, or adapters from a
  request profile;
- multipart upload, streaming transfer, scanning workers, transforms, or CDN
  publication; or
- adding fixed compute, NAT Gateway, schedules, provisioned concurrency, or
  implicit AWS mutations.

## Evidence

Created during the M14-T08 senior hardening review because Minco's typed service
collection permits one `ObjectUploadService` instance and the current managed
plugin therefore exposes one exact policy. Widening that policy across unrelated
product purposes would weaken prefix, media, and size boundaries. The follow-up
retains the limitation explicitly until a static multi-profile contract is
designed and qualified.
