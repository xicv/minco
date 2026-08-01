# Plugin distribution records

Minco keeps plugin composition static while making compatibility inspectable
before code is linked. Each catalog package contains an archive-visible
`minco-plugin.json` selected by Cargo metadata:

```toml
[package]
include = ["src/**", "Cargo.toml", "minco-plugin.json"]

[package.metadata.minco]
plugin = "minco-plugin.json"
```

The pointer must be one package-root JSON filename. The target must be a regular
file no larger than 1 MiB, and the package path must remain inside the project.
The strict parser rejects unknown fields and unsupported schemas.

## Schema 1

The top-level record contains these required distribution coordinates:

| Field | Meaning |
|---|---|
| `schema` | Distribution schema; currently `1`. |
| `id`, `kind` | Stable lower-kebab ID and `plugin`, `adapter`, or `runtime`. |
| `plugin_version` | Version of the plugin/component contract, which may differ from the containing crate release. |
| `core_compatibility` | SemVer requirement for the Minco core API. |
| `stability`, `default_enabled`, `feature` | Catalog maturity and explicit Cargo selection coordinate. |
| `runtimes`, `databases` | Supported execution and persistence profiles; not automatic selections. |
| `requires`, `provides`, `plugin_dependencies` | Typed graph requirements and contributions. |
| `configuration` | Names, types, requirements, descriptions and non-secret defaults. |
| `operations` | Method/path/public/idempotency metadata plus security-relevant header names. |
| `migrations`, `seeds` | Explicit database assets and seed classifications. |
| `resources` | Resource kind, conditional feature, IAM actions, wake sources, dependencies and idle-cost class. |
| `health_checks`, `data_classes` | Operational readiness and sensitivity declarations. |
| `retention`, `failure_policy` | Storage lifetime and fail/degrade behavior. |
| `documentation`, `conformance` | Learning links and inert evidence labels. |

Secret configuration fields may set `"secret": true`, but must omit `default`.
The record contains no secret-value or provider-credential field. Application
configuration owns opaque secret references and values.

The complete official records under `plugins/` and `extensions/` are executable
examples of every field family.

## Commands

```text
# Static inspection only: does not construct plugin code.
cargo minco plugin list --json

# Validate package inclusion, schema, catalog drift, safety rules and linked
# first-party runtime-descriptor overlap.
cargo minco plugin validate --json

# Scaffold a crate, metadata pointer, distribution record and catalog entry.
cargo minco plugin new example
```

`plugin validate` never runs a conformance evidence string. A CI or release
workflow chooses and executes the reviewed command separately.

For registry dependencies, an application-local validation does not fetch or
execute the package. Inspect the downloaded `.crate` archive (or validate the
same package as a path dependency) to verify its embedded record. Local path
dependencies, including Minco's official workspace catalog, are checked
strictly for the pointer, file, package inclusion and contract drift.

## Authority and conditional behavior

Cargo remains authoritative for what is compiled and shipped. The distribution
record is authoritative for the pre-link union of supported behavior. The
runtime descriptor is authoritative for one explicitly constructed and
configured instance. Application configuration owns enablement, provider
selection, retention values and secrets.

Overlapping stable fields must match. Runtime-selected migrations and resources
must appear in the distribution union, so metadata can conservatively describe
all supported profiles without claiming that every resource is provisioned.
Nothing in the record performs runtime discovery, registration, provisioning,
migration or seed execution.

See [ADR 0027](../adrs/0027-static-plugin-distribution-manifest.md) for the
decision and rejected alternatives.
