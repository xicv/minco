# ADR 0028: Exact static-site release, publication and verification

## Status

Accepted

## Context

The static-site plugin already described private S3 storage, CloudFront OAC,
cache behavior and optional domain intent. A resource graph and a generated
template did not yet prove which frontend bytes were released, whether S3
stored those bytes, whether CloudFront served them, or whether a certificate
and Route 53 alias belonged to the reviewed account and hostname.

Publication also has a destructive edge: deleting stale objects before every
new object is durably verified can break the currently served site. Two
publishers targeting one prefix can race even when each publisher is locally
correct.

Provider behavior was rechecked on 2026-08-02 against AWS documentation for
[private S3 origins and OAC](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/private-content-restricting-access-to-s3.html),
[CloudFront cache policies](https://docs.aws.amazon.com/AWSCloudFormation/latest/TemplateReference/aws-properties-cloudfront-distribution-cachebehavior.html),
[alternate-domain certificates](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/cnames-and-https-requirements.html),
[S3 SHA-256 checksums](https://docs.aws.amazon.com/AmazonS3/latest/userguide/checking-object-integrity-upload.html),
[invalidation status](https://docs.aws.amazon.com/cloudfront/latest/APIReference/API_GetInvalidation.html),
and
[eligibility-dependent flat-rate plans](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/flat-rate-pricing-plan.html).
These sources describe current provider contracts, not future prices or a
particular account's eligibility.

## Decision

An enabled `static-site` plugin contributes one typed static-site deployment
intent to Plan IR. The SAM renderer emits a private encrypted S3 bucket,
CloudFront OAC with SigV4 `always` signing, an explicit cache policy, optional
pre-existing `us-east-1` certificate input, optional pre-existing Route 53
hosted-zone input, and stack outputs used by publication and verification.
Minco never creates a certificate or domain silently.

`cargo minco package` walks the configured source without following symlinks,
rejects traversal and the reserved `.minco/` provider-control prefix, and
creates a deterministic manifest containing each normalized path, byte count,
SHA-256, media type and cache policy. The release manifest binds that file as
an attestation. Rebuilding unchanged input produces the same semantic digest.

Publication is a separate guarded stage:

1. `deploy static-site plan` is local and non-contacting;
2. `deploy static-site apply` requires the exact release digest, source,
   started deployment receipt, reviewed target, caller and stable stack;
3. a conditional S3 object at `.minco/deployment-lock` serializes publishers
   across machines and prefixes;
4. every release asset is uploaded with its SHA-256 checksum and exact metadata;
5. `HeadObject` must return the same checksum, size, media type and cache policy
   for every asset before any stale object is deleted;
6. stale deletion remains inside the dedicated bucket or selected prefix;
7. one deterministic CloudFront invalidation is created and must reach
   `Completed` before an immutable publication receipt is written;
8. the control lock is removed only after the complete publication succeeds.
   Any failure leaves the lock in place because provider state may be partial;
   recovery is a separate, reviewed operator action.

`deploy verify --static-site` keeps API hosted verification mandatory and adds
current provider observations. It verifies the exact S3 object set, CloudFront
bytes and response metadata, distribution, OAC, completed invalidation,
`ISSUED` certificate and SAN,
public hosted-zone ownership, A/AAAA aliases, selected price class, dated
official pricing source and explicit account flat-rate eligibility. Only then
does the generic deployment receipt become `succeeded`. Promotion accepts one
hosted API report and, when present, one release-matching static-site report;
unknown evidence kinds fail closed.

## Consequences

- A frontend build system remains application-owned; Minco binds its output.
- S3 storage, Route 53 and request/transfer charges can remain while compute is
  idle. “Zero idle” still means zero provisioned application compute.
- Request/transfer pricing is the ordinary profile. Flat-rate selection is
  valid only with reviewed `eligible_selected` account evidence; Minco never
  changes commercial plans.
- The retained bucket prevents implicit data loss when the static-site plugin
  is removed. Distribution, DNS and cache-policy removal remains visible in a
  CloudFormation change set; retained objects and their cost need a separate
  cleanup decision.
- The first failed upload leaves the previous entrypoint and stale objects in
  place. New content-addressed assets may remain harmlessly and are reconciled
  by the next successful exact publication.

## Rollback

Rollback is forward publication of a previously verified release manifest from
its exact source revision. It uses the same lock, checksum, stale-deletion and
invalidation gates and writes a new publication receipt. It does not mutate an
old receipt or infer bytes from a deployed bucket.

If a process dies after acquiring `.minco/deployment-lock`, operators first
prove that no publisher is running, inspect the exact lock object and reviewed
stack output, then explicitly remove only that object. Minco does not expire or
steal an ambiguous lock automatically.

## Safety

Receipts contain paths, digests, provider identifiers and public hostnames, not
credentials or object bodies. HTTPS verification disables redirects and asks
for identity encoding before hashing. Certificate lookup is fixed to
`us-east-1`; S3 and stack calls use the reviewed deployment Region. Dry-run
never calls AWS or HTTP, writes receipts, uploads, invalidates or deletes.
