# minco

The ergonomic facade crate for the Minco web framework.

Minco is a contract-first, AI-native, AWS-native Rust framework for modular API
backends. The facade always includes the provider-neutral plugin/application
kernel and exposes the rest of the framework through explicit Cargo features.

## Add Minco

```bash
cargo add minco
```

The default feature set provides:

- OpenAPI contract loading and validation;
- Axum/Tower HTTP conventions;
- health, observability, and idempotency plugins.

Database and deployment support are opt-in:

```bash
cargo add minco --features sqlx-postgres,aws-lambda,plan,release
```

For a local SQLite application:

```bash
cargo add minco --features sqlx-sqlite,test
```

Enable typed environment composition explicitly:

```bash
cargo add minco --features config
```

## Compose plugins

```rust
use minco::prelude::*;

let manager = minco::default_plugin_manager()?;
let application = manager.compose(&PluginSelection::default())?;

println!("{} plugins installed", application.graph.plugins.len());
# Ok::<(), minco::core::PluginError>(())
```

Disable an official default without changing the compiled dependency set:

```rust
use minco::prelude::*;

let mut selection = PluginSelection::default();
selection.disabled.insert(PluginId::new("idempotency")?);
let application = minco::compose_defaults(&selection)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

For the repository layout, CLI, contract workflow, database profiles, and AWS
deployment model, see the project documentation in the Minco repository.

## CLI

Install the Cargo subcommand separately:

```bash
cargo install cargo-minco --locked
cargo minco doctor
```

## License

Dual-licensed under MIT or Apache-2.0, at your option.
