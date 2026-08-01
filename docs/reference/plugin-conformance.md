# Plugin conformance

Minco publishes one offline conformance API in `minco-test`. Official and
third-party plugins use the same `PluginConformance` builder and
`PluginConformanceReport` JSON shape. The kit never scans for runtime plugins,
downloads packages, executes manifest evidence strings, calls a provider, runs
migrations, or starts background work.

## Public API

Add version-matched dependencies to a plugin package:

```toml
[dependencies]
minco-core = "0.6.0"

[dev-dependencies]
minco-test = "0.6.0"
```

Exercise a concrete plugin from its own package workspace:

```rust
use minco_test::{ConformanceStatus, PluginConformance};
# use minco_core::Plugin;
# fn check<P: Plugin>(plugin: P) {
let report = PluginConformance::for_package(env!("CARGO_MANIFEST_DIR"))
    .with_plugin(plugin)
    .run();

report.assert_passed();
assert_eq!(report.assurance.plugin_lifecycle, ConformanceStatus::Passed);
assert_eq!(report.assurance.provider_live, ConformanceStatus::NotRun);
# }
```

Use `with_configuration` for required plugin settings and
`with_supporting_plugin` for declared plugin/capability dependencies. Use
`with_descriptor` when only archive/runtime overlap can be assessed; its report
leaves `plugin_lifecycle` as `not_assessed`.

The standalone
[`examples/plugins/third-party-minimal`](../../examples/plugins/third-party-minimal)
workspace uses versioned dependencies with repository path overrides so the
unreleased source can be tested exactly as a crates.io consumer will use the
published packages:

```bash
cargo test \
  --manifest-path examples/plugins/third-party-minimal/Cargo.toml \
  --all-features --locked
```

## Offline checks

The package contract covers:

- the strict, size-bounded, non-symlink distribution record and Cargo include
  list;
- current Minco core compatibility and the profile implied by component kind;
- unique configuration, capability, operation, migration, seed, health, and
  resource identifiers;
- configuration default types and the prohibition on secret defaults;
- HTTP operation/route ownership and valid security-relevant header names;
- declared database and packaged migration/seed paths;
- resource dependencies, conditional Cargo features, IAM action syntax, wake
  sources, and explicit idle-cost classes;
- HTTPS documentation, inert evidence labels, data classifications, retention,
  and failure policy;
- provider-runtime dependency leakage from provider-neutral plugin crates;
- linked descriptor overlap, graph construction, configuration rejection,
  deterministic installation, and metadata-only registration provenance when
  the caller supplies a concrete plugin.

Unknown JSON fields fail strict deserialization. Diagnostics are sorted by
`code`, `path`, then `message`; callers should automate against `code` and
`path`, not prose.

## Assurance boundary

Every report separates these states:

| Field | Meaning |
|---|---|
| `plugin_contract` | Offline package/distribution/descriptor checks passed or failed. |
| `plugin_lifecycle` | Concrete registration and composition passed, failed, or was not assessed. |
| `application_readiness` | Always `not_assessed`; an application must prove its own graph, contract, data, and runtime. |
| `provider_live` | Always `not_run` in this kit; provider tests are explicit separate commands. |
| `production_readiness` | Always `not_assessed`; conformance is not deployment or production certification. |

`cargo minco plugin test --all --json` runs all local catalog packages through
this boundary. Registry-backed dependencies are not fetched or executed; run
the API in that plugin's package workspace instead.

## Stable diagnostic families

Codes are lower snake case and grouped by boundary:

- `package_*` and `distribution_*` for Cargo/archive safety;
- `core_*`, `conformance_*`, and `documentation_*` for compatibility and
  evidence metadata;
- `configuration_*`, `operation_*`, `migration_*`, `seed_*`,
  `health_check_*`, `resource_*`, and `schedule_*` for distribution behavior;
- `descriptor_*` for archive/runtime drift;
- `plugin_*`, `supporting_plugin_*`, `unknown_configuration_*`, and
  `registration_provenance_*` for concrete lifecycle checks;
- `provider_dependency_leakage` for provider-specific runtime dependencies in a
  provider-neutral plugin package.

Provider integration commands, Rustack tests, database-backed behavior tests,
and bounded real-AWS smoke remain separately labelled evidence. Passing this
kit cannot promote any of them to passed.
