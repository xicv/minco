# Test a plugin

Use the public conformance kit inside the plugin crate, not from an application
that merely consumes it.

## 1. Publish the package record

Point Cargo metadata to one package-root record and include it in the archive:

```toml
[package]
include = ["src/**", "Cargo.toml", "minco-plugin.json"]

[package.metadata.minco]
plugin = "minco-plugin.json"
```

Complete the schema described in
[`plugin-distribution.md`](../reference/plugin-distribution.md). Evidence values
are labels only; the CLI never runs those strings.

## 2. Add the test dependency

```toml
[dev-dependencies]
minco-test = "0.6.0"
```

Keep `minco-core` and `minco-test` on the same Minco release.

## 3. Exercise the public boundary

```rust
use minco_test::PluginConformance;
# use minco_core::Plugin;
# fn check<P: Plugin>(plugin: P) {
PluginConformance::for_package(env!("CARGO_MANIFEST_DIR"))
    .with_plugin(plugin)
    .run()
    .assert_passed();
# }
```

For required settings, insert `.with_configuration(serde_json::json!({...}))`.
For plugin dependencies, add each constructor with `with_supporting_plugin`.
The kit composes locally and rejects an injected unknown configuration field;
plugin lifecycle hooks must therefore remain deterministic and free of remote
calls.

## 4. Run local evidence

```bash
cargo test --all-features --locked
cargo minco plugin validate
cargo minco plugin test --all
```

`plugin validate` checks catalog and linked-descriptor drift. `plugin test`
checks every local catalog package. A standalone or registry plugin should run
its direct Rust conformance test from its own workspace.

## 5. Prove provider behavior separately

Run database, emulator, provider sandbox, and bounded live tests under explicit
commands and credentials. Record those results separately. Offline conformance
does not prove application readiness, data durability, provider compatibility,
deployment success, or production readiness.

See [`plugin-conformance.md`](../reference/plugin-conformance.md) for the report
shape and diagnostic contract.
