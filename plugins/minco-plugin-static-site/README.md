# minco-plugin-static-site

Official provider-neutral static-site deployment plugin for Minco.

The plugin describes a private object bucket, CDN distribution, immutable asset
caching, SPA fallback, optional custom domain, certificate and DNS alias. It
does not run Node or assume React, Vue, Vite, or another frontend toolchain.
Deployment renderers consume the resulting `StaticSitePlan` and resource graph.

```rust
use minco_plugin_static_site::StaticSitePlugin;

manager.register(StaticSitePlugin)?;
```

Configuration lives under `plugins.static-site` in the normal Minco plugin
selection. The source directory must already contain a built static artifact.

`StaticSiteReleaseManifest::build` produces the provider-neutral, deterministic
release boundary: normalized path, byte count, SHA-256, content type and cache
metadata for every file. It rejects symlinks, traversal and the reserved
`.minco/` provider-control prefix. `StaticSitePublisherService` re-verifies the
manifest immediately before invoking an injected provider.

The Minco CLI automatically attaches this manifest during `cargo minco package`
and exposes guarded `deploy static-site plan`, `deploy static-site apply`, and
`deploy verify --static-site` stages. The plugin itself makes no AWS call and
does not create a certificate, domain or hosted zone.
