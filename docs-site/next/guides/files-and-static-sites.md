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

### Current upload boundary

Current source ships provider-neutral object-storage service ports, but it does
not ship a direct-to-object-store signed-upload HTTP flow. The application owns
the upload use case and HTTP policy, then calls the selected adapter through
the typed service boundary. Treat any future direct-upload contract as shipped
only after it appears in the versioned API, implementation, and tests.

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
