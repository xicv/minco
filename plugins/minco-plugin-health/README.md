# minco-plugin-health

Official Minco health plugin.

It installs an asynchronous typed health registry and supports critical and
non-critical dependency checks. The plugin descriptor contributes health
capability and readiness metadata to the Minco application graph.

```rust
use minco_plugin_health::{HealthRegistry, StaticHealthCheck};
use std::sync::Arc;

# async fn check() {
let registry = HealthRegistry::default();
registry.register(Arc::new(StaticHealthCheck::new("database", true, true)));
assert!(registry.ready().await);
# }
```
