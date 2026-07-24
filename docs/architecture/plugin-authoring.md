# Authoring Minco Plugins

Minco plugins are statically linked Rust crates. Static linkage keeps type safety,
reviewable dependency graphs, predictable startup, and deployment analysis. The
plugin API is stable Rust source/API—not a dynamic-library ABI.

## Plugin contract

A plugin provides:

- a `PluginDescriptor` with ID, semantic version, dependencies, capabilities,
  operations, migrations, health checks, and resource intents;
- an `install` implementation that inserts typed services into
  `ServiceCollection` and contributes graph metadata;
- configuration validation;
- unit and conformance tests;
- cost, wake-source, IAM, local-emulation and health documentation where relevant.

```rust
#[async_trait::async_trait]
impl minco_core::Plugin for ExamplePlugin {
    fn descriptor(&self) -> minco_core::PluginDescriptor { /* explicit data */ }

    async fn install(
        &self,
        context: &mut minco_core::PluginContext<'_>,
    ) -> Result<(), minco_core::PluginError> {
        context.services.insert(std::sync::Arc::new(ExampleService::new()))?;
        Ok(())
    }
}
```

The actual implementation must return real descriptors/services; the snippet
shows the interface shape only.

## Create and register

```bash
cargo minco plugin new audit-export --path plugins/minco-plugin-audit-export
cargo minco plugin validate audit-export
cargo minco plugin enable audit-export
```

Add the plugin as a Cargo dependency and register its constructor in the
application composition root. The catalog controls default selection; it does
not dynamically download or execute code.

## Default plugins

- `health`: critical/non-critical health registry and readiness aggregation.
- `observability`: structured tracing configuration.
- `idempotency`: typed keys, fingerprints, replay/conflict semantics and memory
  reference store.

Applications may disable a default plugin in configuration when its dependencies
allow it. The plugin manager rejects missing dependencies, duplicate IDs,
capability version mismatches and cycles before startup.

## Ecosystem compatibility

Third-party plugins should publish:

- supported Minco API requirement;
- plugin/capability semantic versions;
- exact feature flags and transitive dependencies;
- configuration schema and examples;
- supported runtimes/databases;
- local and AWS resource behavior;
- security policy and test evidence.

Minco core must not add product-specific code to accommodate a plugin. New
cross-cutting extension points require an ADR and a minimal interface proven by
at least two implementations.


## Registry plugins and local plugins

Catalog entries without `path` describe crates resolved through Cargo. Local
workspace plugins declare a repository-relative `path`; `cargo minco plugin
validate` then verifies the local manifest and package name. This distinction
lets application repositories consume published plugins while authoring new
plugins locally without runtime discovery.
