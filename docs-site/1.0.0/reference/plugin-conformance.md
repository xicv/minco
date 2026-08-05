---
title: Plugin Conformance
description: Public conformance builder, deterministic report shape, offline checks, diagnostics, and assurance boundaries.
---

# Plugin Conformance

`minco-test` exposes one builder for plugin, adapter, and runtime package
profiles.

## Profiles

| Component kind | Profile |
|---|---|
| Plugin | `minco-plugin-v1` |
| Adapter | `minco-adapter-v1` |
| Runtime | `minco-runtime-v1` |

## Builder

```rust
let report = PluginConformance::for_package(package_root)
    .with_plugin(plugin)
    .with_supporting_plugin(dependency)
    .with_configuration(configuration)
    .run();
```

| Method | Purpose |
|---|---|
| `for_package(root)` | Set the package/archive boundary |
| `with_descriptor(descriptor)` | Compare static distribution and runtime descriptor without installing |
| `with_plugin(plugin)` | Assess descriptor overlap plus concrete lifecycle composition |
| `with_supporting_plugin(plugin)` | Satisfy declared graph dependencies explicitly |
| `with_configuration(json)` | Supply required typed plugin configuration |
| `run()` | Return a deterministic report; never contact a provider |

`PluginConformanceReport::assert_passed()` prints the full JSON report when a
test fails.

## Report Shape

```json
{
  "profile": "minco-plugin-v1",
  "plugin_id": "third-party-example",
  "status": "passed",
  "assurance": {
    "plugin_contract": "passed",
    "plugin_lifecycle": "passed",
    "application_readiness": "not_assessed",
    "provider_live": "not_run",
    "production_readiness": "not_assessed"
  },
  "diagnostics": []
}
```

Automate against status, diagnostic `code`, and diagnostic `path`. Diagnostic
messages are public prose and may become clearer without changing the stable
code.

## Offline Checks

The kit checks:

- one regular, package-root, size-bounded distribution JSON file;
- Cargo inclusion, core compatibility, and component profile;
- strict fields and unique identifiers;
- configuration types and the prohibition on secret defaults;
- capabilities, dependencies, operations, headers, migrations, seeds, health,
  resources, IAM syntax, wake sources, data classes, retention, and failure
  policy;
- HTTPS documentation and inert evidence labels;
- provider dependency leakage, including renamed Cargo dependencies and
  target-specific sections;
- linked descriptor overlap, graph construction, unknown configuration,
  deterministic repeated composition, and registration provenance.

It never scans dynamically, downloads a package, runs an evidence string,
executes a migration, starts background work, or calls a cloud provider.

## Assurance Fields

| Field | Possible current meaning |
|---|---|
| `plugin_contract` | offline package/distribution checks passed or failed |
| `plugin_lifecycle` | concrete composition passed/failed, or descriptor-only `not_assessed` |
| `application_readiness` | always `not_assessed`; the application owns this proof |
| `provider_live` | always `not_run`; provider checks are separate commands |
| `production_readiness` | always `not_assessed`; conformance is not certification |

## Diagnostic Families

Stable lower-snake-case families include `package_*`, `distribution_*`,
`core_*`, `conformance_*`, `configuration_*`, `operation_*`, `migration_*`,
`seed_*`, `health_check_*`, `resource_*`, `schedule_*`, `descriptor_*`,
`plugin_*`, `supporting_plugin_*`, `unknown_configuration_*`,
`registration_provenance_*`, and `provider_dependency_leakage`.

See [Test a Plugin](../guides/plugin-conformance) for the workflow.
