---
title: Test a Plugin
description: Package a static plugin and run the same offline conformance boundary used by official Minco components.
---

# Test a Plugin

The Minco 1.3.0 candidate publishes one `minco-test` builder for official and
third-party-style plugins. It checks package metadata, distribution intent,
descriptor overlap, and—when supplied—deterministic plugin composition.

## 1. Create the Package Boundary

The current CLI can scaffold an official-style package:

```bash
cargo minco plugin new example
```

For an external package, keep the distribution record inside the crate archive
and point to it from Cargo metadata.

```toml
[package]
name = "minco-plugin-example"
version = "0.1.0"
include = ["src/**", "Cargo.toml", "minco-plugin.json"]

[package.metadata.minco]
plugin = "minco-plugin.json"
```

Use exact `1.2.2` registry dependencies. Use reviewed
path dependencies only while developing coordinated post-release changes from
a Minco source checkout.

## 2. Declare Distribution Intent

Start with the smallest strict schema. Add capabilities, configuration,
operations, database assets, resources, health, data classes, wake sources, and
cost intent only when the plugin really supports them.

```json
{
  "schema": 1,
  "id": "example",
  "kind": "plugin",
  "plugin_version": "0.1.0",
  "core_compatibility": "^1.2.0",
  "stability": "experimental",
  "default_enabled": false,
  "feature": "plugin-example",
  "runtimes": ["native"],
  "retention": "none",
  "failure_policy": {
    "mode": "fail_closed",
    "description": "Example failures remain explicit."
  },
  "documentation": {
    "reference": "https://docs.rs/minco-plugin-example"
  },
  "conformance": {
    "profile": "minco-plugin-v1",
    "evidence": ["cargo test --all-features --locked"]
  }
}
```

Evidence strings are labels. Validation never executes them.

## 3. Exercise the Public Builder

Test from the plugin package root so Cargo inclusion and relative assets match
the archive boundary.

```rust
use minco_test::{ConformanceStatus, PluginConformance};

let report = PluginConformance::for_package(env!("CARGO_MANIFEST_DIR"))
    .with_plugin(ExamplePlugin)
    .run();

report.assert_passed();
assert_eq!(
    report.assurance.plugin_lifecycle,
    ConformanceStatus::Passed
);
assert_eq!(
    report.assurance.provider_live,
    ConformanceStatus::NotRun
);
```

Use `.with_configuration(value)` when the plugin requires settings. Use
`.with_supporting_plugin(plugin)` for declared graph dependencies. A
descriptor-only run can assess package/descriptor overlap but must report the
lifecycle as `not_assessed`.

## 4. Run Both Package and Catalog Checks

```bash
cargo test --all-features --locked
cargo minco plugin validate --json
cargo minco plugin test --all --json
```

`plugin test --all` covers local catalog packages only. It does not download a
registry package or execute an evidence command.

## 5. Interpret the Boundary Correctly

Passing proves an offline plugin contract and, when a concrete plugin is
supplied, deterministic composition. It does not prove application readiness,
database migration behavior, a provider call, an AWS deployment, or production
fitness.

See the [Plugin Conformance reference](../reference/plugin-conformance) and the
[standalone exercised example](../examples/).
