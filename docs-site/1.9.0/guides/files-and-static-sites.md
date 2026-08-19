---
title: Files and Static Sites
description: Use provider-neutral object storage and publish exact static assets through private S3 and CloudFront.
---

# Files and Static Sites

The object-storage plugin supplies typed upload, download, metadata, and deletion
ports. The static-site plugin adds deployment intent for private assets, CDN
caching, SPA fallback, and an optional custom domain.

## Select only what you need

```bash
cargo minco plugin enable object-storage --dry-run --json
cargo minco plugin enable static-site --dry-run --json
```

For AWS, compile the `aws-adapters` feature and inject the selected S3 adapter
at the composition root. Provider credentials and bucket policy never live in
plugin distribution metadata.

## Uploads and attachments

Application policy owns:

- allowed media classes and content types;
- maximum file and aggregate request size;
- object keys and tenant/owner boundaries;
- encryption, retention, deletion, scanning, and residency;
- whether downloads are streamed, proxied, or served through a signed URL.

Treat filenames, metadata, images, audio, and documents as untrusted input. An
object-store success does not make content safe to parse or execute.

### Direct upload, update and download

Current source includes an opt-in authenticated JSON control plane for direct
single and multipart upload, immutable update, private download, range resume
and conditional cache metadata. Inject one application-owned
`ObjectTransferHttpUseCases` implementation into
`ManagedObjectStoragePlugin::with_http_api`; each handler calls exactly one use
case after bounded transport validation. The application still owns principal
authorization, purpose/tenant quotas, durable sessions, logical object IDs,
conditional pointer updates, retention and content-inspection policy.

Large file bytes travel directly between the browser or native client and the
private provider. The Lambda/API Gateway path carries only bounded JSON. A
multipart plan has no more than 10,000 parts; the completion manifest is capped
at 3 MiB and each provider part `ETag` at 64 bytes. Retry only a failed part,
keep its latest accepted receipt, abort cancelled sessions, and configure
provider lifecycle cleanup for incomplete uploads.

Completed untrusted bytes begin in quarantine. Exact size, content type,
checksums and provider metadata prove integrity, not safe content. Publish a new
immutable revision only after the application-selected scanner or decoder
accepts it and the logical pointer's `If-Match` condition still succeeds.

Private download grants can cover the full object or one byte range bound to a
strong provider validator. Cancelling the client request stops consumption;
resuming requests a fresh range grant from the acknowledged offset. Cache bytes
by stable object ID plus revision and revalidate metadata with `If-None-Match`.
An authorized `304 Not Modified` avoids issuing another signed URL and avoids
another object download. Use `no-store` where local revocation risk is more
important than repeat-transfer savings.

S3 is the production-targeted byte-plane adapter. A filesystem or other
provider is not large-transfer ready merely because it implements the older
buffering `ObjectStore`; it must provide bounded streaming, download signing,
multipart/abort behavior and the same conformance guarantees, or the
application must expose its own protected byte endpoint.

## Configure a static site

Enable the plugin, then point `minco.toml` at application-owned build output.
The static-site plan records source paths, content hashes, cache behavior, SPA
fallback, private S3 origin, CloudFront distribution, and optional domain
inputs.

Package the API and assets once:

```bash
cargo minco package
cargo minco release verify target/minco/release.json
```

Review the exact asset publication before mutation:

```bash
cargo minco deploy static-site plan \
  --manifest target/minco/release.json \
  --json
```

Applying requires the exact reviewed plan, target ownership, and deployment
authority. It uploads the packaged bytes; it does not rebuild the frontend.

## Cache and rollback

Immutable fingerprinted assets should receive long cache lifetimes. The HTML
entry point needs a shorter policy so it can point at a promoted asset set.
Rollback must select a compatible prior release and its exact asset manifest;
deleting current source files is not rollback proof.

## Verify separately

```bash
cargo minco deploy verify --static-site --dry-run --json
```

Dry-run output lists the API and static checks but contacts nothing. A live gate
must verify the expected host, private-origin behavior, representative asset
bytes/content types, SPA fallback, caching, and release identity.

Static storage and CDN/DNS dimensions can cost money while compute is idle. See
[Zero idle, precisely](../explanation/zero-idle).
