# Typed configuration reference

Minco composes application defaults, environment files, enabled-plugin schema,
explicit environment overrides and CLI overrides into one
`ConfigurationGraph`. Graph construction is provider-neutral and performs no
network or secret-value access.

## Manifest

`minco.toml` selects the profile directory and the only process-environment
prefix the CLI reads:

```toml
[configuration]
root = "config/environments"
default_file = "default.toml"
local_override = ".local.toml"
environment_prefix = "MINCO_CONFIG__"

[[configuration.fields]]
key = "application.name"
kind = "string"
required = true
secret = false
description = "Stable application service name"
default = "orders"

[[configuration.fields]]
key = "database.connection"
kind = "string"
required = true
secret = true
description = "Opaque database connection secret reference"
```

Ignore the local override in version control. Minco accepts it only for
`local` and `development`; `test`, `staging`, and `production` fail closed when
it is present. The application owns `configuration.fields`; enabled plugins
contribute their fields from descriptors. `root` must be a project-relative
path containing only normal components; `default_file` and `local_override`
must each be one filename. Existing symlinks are resolved and rejected if they
leave the project root.

## Files and precedence

Every document uses schema 1 and puts application values below `[values]`:

```toml
schema = 1
environment_class = "staging"

[values.application]
name = "orders"

[values.database]
url = "ssm:/orders/staging/database-url"

[values.plugins.idempotency]
claim_timeout_seconds = 600
```

`environment_class` is required in `<environment>.toml` and forbidden in other
layers. `default.toml` usually omits it.

Precedence is fixed, independent of API call order:

```text
compiled defaults
< default.toml
< <environment>.toml
< local override
< MINCO_CONFIG__ environment overrides
< --set CLI overrides
```

Environment override paths use double underscores:

```text
MINCO_CONFIG__APPLICATION__NAME=orders-preview
MINCO_CONFIG__PLUGINS__IDEMPOTENCY__CLAIM_TIMEOUT_SECONDS=120
```

Values that parse as JSON keep their JSON type; other values are strings. The
CLI does not read variables outside the configured prefix. The prefix must be
an uppercase identifier ending in `__`.

## Secret references

A field marked `secret` accepts only:

```text
env:ORDERS_DATABASE_URL
ssm:/orders/production/database-url
```

These strings identify a later resolver. Minco does not read the named
environment variable or SSM parameter while composing, checking, explaining,
diffing or hashing the graph. Explain and diff omit reference names as well as
resolved values. Typed-constructor failures for a namespace containing secret
fields also omit deserializer-provided error detail, because a custom
deserializer can otherwise echo its input. SSM references fail early unless
they obey the documented
[Parameter Store name constraints](https://docs.aws.amazon.com/systems-manager/latest/userguide/sysman-parameter-name-constraints.html),
including allowed characters, reserved prefixes, length and hierarchy depth.

Application composition receives the typed reference through a constructor:

Enable the facade surface with `minco = { version = "...", features =
["config"] }`, or depend directly on `minco-config`.

```rust
use minco::config::{ConfigurationGraph, SecretReference};
use serde::Deserialize;

#[derive(Deserialize)]
struct DatabaseSettings {
    url: SecretReference,
}

fn build_database(
    graph: &ConfigurationGraph,
) -> Result<DatabaseSettings, minco::config::ConfigurationError> {
    graph.deserialize_namespace("database")
}
```

The composition root passes `DatabaseSettings` to the adapter that knows how to
resolve it. Domain and application crates do not read the process environment
or use a global locator.

## CLI

```text
cargo minco config check [--environment dev] [--set KEY=VALUE]
cargo minco config explain <path> [--environment dev] [--set KEY=VALUE]
cargo minco config diff --from dev --to production
cargo minco config schema
```

Use `--json` for automation. Diagnostics have stable `config.*` codes. A valid
check prints the environment class and deterministic effective digest. Secret
explanations set `redacted: true` and omit `value`; secret diff entries omit
both `before` and `after`.

## Migrating existing profiles

1. Add `[configuration]` to `minco.toml`.
2. Create `default.toml` for non-secret cross-environment defaults.
3. Create explicit local, test, development, staging and production files.
4. Move application and enabled-plugin values to their typed `[values]`
   namespaces.
5. Replace committed secret values with `env:` or `ssm:` references.
6. Replace application-wide environment reads with one graph constructor call
   per owned namespace.
7. Run `config check` for every environment, review `config diff`, then remove
   the old plugin/runtime value source.

Do not move deployment resource profiles such as `minco.dev.toml` into this
tree. They remain Plan IR inputs selected by `deployment_config`.
