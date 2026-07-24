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
