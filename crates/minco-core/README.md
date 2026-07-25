# minco-core

Provider-neutral kernel for the Minco framework.

This crate contains the stable composition primitives that application and
third-party plugin crates depend on:

- statically linked `Plugin` implementations;
- typed service registration and lookup;
- required/provided capabilities;
- operation, migration, health, resource, and wake-source descriptors;
- deterministic plugin dependency ordering and graph validation.

It intentionally has no Axum, SQLx, Lambda, or AWS SDK dependency.

```rust
use minco_core::{PluginManager, PluginSelection};

let manager = PluginManager::default();
let graph = manager.build_graph(&PluginSelection::default())?;
assert!(graph.plugins.is_empty());
# Ok::<(), minco_core::PluginError>(())
```

`build_graph` resolves configuration and validates the same graph as runtime
composition without installing services or invoking lifecycle hooks. Deployment
planning can therefore remain read-only.

Most application authors should depend on the `minco` facade crate. Plugin
authors may depend directly on `minco-core` to keep their dependency surface
small.
