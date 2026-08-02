---
id: M10-T05
title: Complete static-site and custom-domain deployment
milestone: M10
status: complete
priority: high
area: deployment/static-site
depends_on: [M6-T04, M10-T03]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - plugins/minco-plugin-static-site/**
  - extensions/minco-aws-adapters/**
  - crates/minco-cli/**
  - crates/minco-deploy-aws/**
  - crates/minco-plan/**
  - docs/adrs/**
  - docs/DECISIONS.md
  - infra/aws/**
  - scripts/aws/**
  - docs/deployment/**
  - docs/reference/cli.md
  - verification/deep-review.json
  - verification/adoption-measurements.json
  - verification/publish-validation.json
  - verification/source-manifest.json
  - verification/static-validation.json
  - tasks/M10/M10-T05-static-site-domain.md
checks:
  - cargo test -p minco-plugin-static-site -p minco-aws-adapters -p minco-deploy-aws --all-features --locked
  - cargo minco deploy verify --static-site --dry-run
  - sam validate --lint --template-file infra/aws/generated/template.yaml
---

## Goal

Complete private-object publication, CloudFront OAC, optional custom-domain
inputs, certificate/DNS guards, cache policy, invalidation, and hosted
byte/hash verification through the generic deployment receipt. Research the
current CloudFront request/transfer and flat-rate choices before selecting a
profile; flat-rate pricing is eligibility-dependent and must not be encoded as
a timeless default.

## Acceptance

- traversal and content-type/cache behavior remain safe and deterministic;
- uploaded bytes and deployed object hashes match the release;
- certificate region, DNS ownership, distribution, and invalidation are
  explicit guarded stages;
- live CloudFront proof is separately authorised and cost-labelled;
- cost evidence uses explicit classes and dated pricing confidence, including
  account eligibility and Region where relevant;
- removal and rollback behavior is documented.

## Non-goals

- a frontend build system;
- public S3 buckets;
- silently creating domains or certificates.

## Completion evidence

Completed on 2026-08-02 in the isolated `minco-task-m10-t05` JJ workspace
against merged-main parent `0b0506db`.

- A deterministic, strict static-site release manifest binds each exact path,
  byte count, SHA-256 digest, content type and cache policy. Traversal,
  symlinks, non-UTF-8 paths, duplicate/unsorted assets and the reserved
  `.minco` provider-control prefix fail before provider calls.
- The AWS adapter uploads checksum-bound AES-256 S3 objects, re-reads metadata,
  deletes stale objects only after all uploads verify, serializes publication
  with an exclusive S3 lock, and waits for a deterministic CloudFront
  invalidation. A failed publication deliberately retains its lock for
  explicit operator recovery.
- Plan IR and SAM render a retained, encrypted, private S3 origin, CloudFront
  OAC with always-on SigV4, an explicit cache policy, SPA fallback, exact
  bucket policy, optional existing `us-east-1` certificate and guarded public
  Route 53 A/AAAA aliases. Request/transfer and flat-rate billing evidence is
  typed, dated and eligibility-aware; unevaluated eligibility is rejected.
- CLI package/apply/verify paths bind source, release, target, caller, stack,
  object bytes and metadata, distribution/OAC, completed invalidation,
  certificate SAN, public DNS and CDN response evidence. Immutable receipts
  fail closed, and promotion accepts only structurally valid release-bound
  hosted/static verification kinds.
- `cargo test -p minco-plugin-static-site -p minco-aws-adapters
  -p minco-deploy-aws --all-features --locked` passed. Focused strict Clippy,
  static-site manifest/adapter/Plan/SAM/CLI suites and the complete
  `./scripts/quality.sh` repository gate passed with warnings denied.
- `cargo minco deploy verify --static-site --dry-run` reported the expected
  missing hosted-evidence blockers without contacting AWS or a CDN.
  `sam validate --lint --template-file infra/aws/generated/template.yaml`
  reported a valid SAM template.
- Exact ARM64 candidate artifacts were measured: Orders Lambda SHA-256
  `7864a2533e14dbb21abec1d7757e1ace047dc1c2b9c9b4c7e3081ff08288a5f7`
  at 5,102,303 compressed bytes, and SQS worker SHA-256
  `80d7f8bb3c82a4ead305696437dcad88f5c1473b82373e8a606e5d61749b11f8`
  at 574,199 compressed bytes. The official-plugin dependency budget remains
  unchanged and the all-feature package delta is recorded separately.

No live AWS API, hosted endpoint or CDN was contacted. No resource,
deployment, DNS record, certificate, promotion, package, release tag or
registry publication was created or changed; those remain separately
authorised runtime and release operations.
