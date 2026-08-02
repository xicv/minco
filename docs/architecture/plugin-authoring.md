# Authoring Minco Plugins

Minco plugins are statically linked Rust crates. Static linkage keeps type
safety, reviewable dependency graphs, predictable startup, and deployment
analysis. The plugin API is a Rust source/API contract, not a dynamic-library
ABI and not a runtime package-discovery mechanism.

## Lifecycle

Composition has four deterministic phases:

1. **Registration** reads and validates the immutable `PluginDescriptor`.
2. **Configuration** applies schema defaults, rejects unknown fields, and allows
   the plugin to adjust graph metadata through `configure_descriptor`.
3. **Installation** registers typed single services and ordered multi-provider
   contributions. It must not make network calls, run migrations, or start
   background work.
4. **Finalization** assembles aggregate registries after every plugin has
   installed. It is also side-effect-free with respect to remote systems.

The complete configured graph is validated before installation. Missing or
incompatible capabilities, duplicate operations/routes/migrations/resources,
plugin cycles, and resource cycles therefore fail before concrete clients are
constructed.

## Minimal plugin

```rust
use minco_core::{
    CapabilityProvision, Plugin, PluginContext, PluginDescriptor, PluginError,
    PluginId, PluginStability,
};
use semver::{Version, VersionReq};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ExampleService;

#[derive(Debug, Clone, Default)]
pub struct ExamplePlugin;

impl Plugin for ExamplePlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("example").expect("static plugin ID"),
            Version::new(1, 0, 0),
            "Example capability",
        );
        descriptor.core_compatibility =
            VersionReq::parse("^0.1").expect("static compatibility requirement");
        descriptor.stability = PluginStability::Beta;
        descriptor.provides.push(CapabilityProvision {
            name: "example.use".into(),
            version: Version::new(1, 0, 0),
        });
        descriptor
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        context.services().insert(Arc::new(ExampleService))?;
        Ok(())
    }
}
```

`install` is synchronous by design. It registers runtime objects; those objects
may expose asynchronous methods. This prevents plugin discovery from silently
connecting to infrastructure.

## Single services and multi-contributions

Use `ServiceCollection` for one authoritative implementation of a type:

```rust
context.services().insert(Arc::new(ExampleService))?;
```

Use `ContributionCollection` when independent plugins may provide multiple
values, such as HTTP modules, health checks, notification observers, or event
handlers:

```rust
context.contributions().push(Arc::new(MyHttpModule));
context
    .contributions()
    .push_shared::<dyn MyObserver>(Arc::new(MyObserverImpl));
```

Registration order is deterministic. A plugin that owns an aggregate registry
can read contributions during `finalize` and register the final single service.
Do not emulate multi-binding by replacing a previously registered service.

`PluginContext` binds both registrars to the current effective plugin ID.
Plugins never supply an owner string or token. If two plugins register the same
singleton type, composition fails with a diagnostic naming the first owner,
attempted owner and Rust type. Contributions retain deterministic installation
indices and remain ordered within their Rust type.

Successful composition exposes metadata-only summaries through
`ComposedApplication::registration_provenance()` and `cargo minco inspect
--json`. The summaries contain types, owners and indices only. Do not add
configuration, service values, URLs, credentials or concrete `Debug` output to
inspection.

## Application-provided dependencies

Concrete database pools, AWS clients, clocks, and product-specific adapters
belong in the application composition root. Supply them through
`PluginManager::compose_with` before plugin installation. This keeps provider
selection explicit and avoids a global service locator. Direct
`ServiceCollection::insert` and `ContributionCollection::push` registrations
are identified as `application`; plugin registrations remain distinct.

## Configuration contract

Publish every supported setting in `PluginDescriptor::configuration` with:

- stable key;
- value kind;
- required/optional status;
- secret classification;
- documented default;
- user-facing description.

Minco rejects unknown fields and type mismatches and applies descriptor defaults
before deserializing the plugin's configuration type. Secret values are never
copied into the application graph or deployment plan.

A configuration-dependent capability or resource belongs in
`configure_descriptor`, not `install`. The hook may only modify mutable graph
metadata; plugin ID, version, default selection, namespace, and schema remain
immutable.

## HTTP contribution

HTTP-capable plugins contribute a `minco_http::HttpModule` containing:

- a fully state-bound Axum router fragment;
- the exact OpenAPI operation IDs it implements;
- its maximum request-body requirement.
- exact allowed request, exposed response, and sensitive request header
  additions.

`compose_plugin_http` merges modules, rejects operation drift, and applies the
aggregate body/header policy and standard middleware once. Header names are
normalized and de-duplicated; wildcard headers fail composition. A plugin
should not create a second independent HTTP server or copy middleware policy.

## Create and register

```bash
cargo minco plugin new audit-export --dry-run --json
cargo minco plugin new audit-export
cargo minco plugin validate
cargo minco plugin test audit-export
```

`plugin new` is the compatibility spelling of `make plugin`; both create an
application-owned package and catalog entry. For an existing local package,
use `cargo minco plugin init plugins/minco-plugin-audit-export --dry-run --json`
before applying the catalog edit.

Add an app-owned or third-party crate as a normal Cargo dependency and register
its typed constructor in the composition root:

```rust
let mut manager = minco::default_plugin_manager()?;
manager.register(AuditExportPlugin::new(reviewed_configuration))?;
let plugins = manager.compose(&minco::core::PluginSelection::default())?;
```

`plugin add` automates only known Minco facade features, whose constructors are
already explicitly compiled into the facade. It refuses app-owned packages so
that it never guesses a Rust constructor or configuration expression. The
catalog controls inspectable selection metadata; it does not download, discover,
or execute code at runtime. Run `cargo minco plugin doctor --json` after the
composition edit.

## Official plugin tiers

The bounded default set is:

- `health`;
- `observability`;
- `idempotency`.

Opt-in official plugins cover sessions, identity/permissions, object storage,
events/outbox, notifications, audit, static-site deployment intent, and the
Feedback vertical slice. Memory implementations are deterministic development
references, not a claim of production durability. Provider adapters must state
their persistence, delivery, IAM, cost, retry, and local-emulation behavior.

## Ecosystem compatibility checklist

Third-party plugins should publish:

- supported Minco core requirement;
- plugin and capability semantic versions;
- stability level and documentation URL;
- configuration schema and examples;
- data sensitivity classes;
- exact Cargo feature flags and transitive dependencies;
- operations, migrations, health checks, resources, wake sources, and idle-cost
  classes;
- supported runtimes/databases;
- security, privacy, retention, and failure semantics;
- unit, conformance, and integration evidence.

Minco core must not add product-specific code to accommodate a plugin. New
cross-cutting extension points require an ADR and a minimal interface proven by
at least two implementations.

## Registry and local plugins

Catalog entries without `path` describe crates resolved through Cargo. Local
workspace plugins declare a repository-relative `path`; `cargo minco plugin
validate` verifies the local manifest and package name. Both use the same public
API and explicit composition path.
