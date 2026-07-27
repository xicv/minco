# minco-config

Provider-neutral typed configuration for Minco applications.

`minco-config` provides:

- one strict application and enabled-plugin schema;
- fixed layer precedence and fail-closed environment classes;
- typed, opaque environment-variable and SSM secret references;
- secret-safe provenance, explanation and environment diff;
- deterministic effective SHA-256 digests;
- constructor-time deserialization of application-owned namespaces.

The graph never resolves a secret or connects to a provider.

```rust
use minco_config::{
    ConfigLayer, ConfigSourceKind, ConfigurationField, ConfigurationGraph,
    ConfigurationSchema, ConfigurationValueKind, Environment, EnvironmentClass,
};
use serde_json::json;

let schema = ConfigurationSchema::try_from_fields([ConfigurationField {
    key: "application.name".into(),
    kind: ConfigurationValueKind::String,
    required: true,
    secret: false,
    description: "Application name".into(),
    default: Some(json!("orders")),
}])?;
let graph = ConfigurationGraph::compile(
    &schema,
    Environment::new("test", EnvironmentClass::Test),
    [ConfigLayer::from_toml(
        ConfigSourceKind::EnvironmentFile,
        "test.toml",
        "schema = 1\nenvironment_class = \"test\"",
    )?],
)?;

assert_eq!(graph.digest().len(), 64);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Most applications use the `minco` facade with its explicit `config` feature,
which exposes this crate as `minco::config`.

## License

Dual-licensed under MIT or Apache-2.0, at your option.
