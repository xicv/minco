# Test a plugin

Use the public conformance kit inside the plugin crate, not from an application
that merely consumes it.

## Features

The standalone example depends only on the public `minco-core` plugin contract
and the `minco-test` conformance kit. It does not enable an application facade,
AWS adapter, HTTP runtime, or database adapter.

## Provider assumptions

The recipe is local and offline. It constructs the plugin through public Minco
APIs and reads only its package metadata and distribution record.

## Cost and wake behavior

The test has `zero_compute` idle cost and no wake source because it creates no
long-running service, schedule, queue, database, or cloud resource.

## Verification

Run the direct standalone Cargo test through
`scripts/test/examples/all.sh`. The matrix binds this page to the
`third-party-plugin` check so documentation and executable evidence cannot
silently diverge.

## Unsupported gates

Offline conformance does not prove provider behavior, application composition,
deployment readiness, data durability, or production safety. Those require
separate integration, provider, deployment, and operational evidence.

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
PLUGIN_ID=your-plugin-id
cargo minco plugin test "$PLUGIN_ID"
cargo minco plugin test --all
```

`plugin validate` checks catalog and linked-descriptor drift. `plugin test`
checks one selected local catalog package; `--all` checks each local entry. A
standalone or registry plugin should run its direct Rust conformance test from
its own workspace.

## 5. Prove provider behavior separately

Run database, emulator, provider sandbox, and bounded live tests under explicit
commands and credentials. Record those results separately. Offline conformance
does not prove application readiness, data durability, provider compatibility,
deployment success, or production readiness.

See [`plugin-conformance.md`](../reference/plugin-conformance.md) for the report
shape and diagnostic contract.
