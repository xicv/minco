---
title: Configure Applications
description: Compose strict environments and opaque secret references with deterministic, redacted provenance.
---

# Configure Applications

Minco builds one typed `ConfigurationGraph` without contacting a provider or
resolving secrets. Application defaults, environment files, plugin schema,
process-environment overrides, and CLI overrides have a fixed order.

## Declare the schema

Generated applications configure the file roots and application-owned fields
in `minco.toml`:

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

Enabled plugins contribute their own namespaces through typed descriptors.
Unknown fields and invalid types fail before application composition.

## Add an environment

Each environment document uses schema 1:

```toml
schema = 1
environment_class = "staging"

[values.application]
name = "orders"

[values.database]
connection = "ssm:/orders/staging/database-url"

[values.plugins.idempotency]
claim_timeout_seconds = 600
```

`environment_class` is required for named environment files. Local override
files are accepted only for `local` and `development`; test, staging, and
production fail closed if one is present.

## Understand precedence

```text
compiled defaults
< default.toml
< <environment>.toml
< local override
< MINCO_CONFIG__ environment overrides
< --set CLI overrides
```

Environment paths use double underscores:

```bash
export MINCO_CONFIG__APPLICATION__NAME=orders-preview
cargo minco config check --environment preview
```

Use CLI overrides for one reviewed invocation:

```bash
cargo minco config check \
  --environment staging \
  --set application.name=orders-staging \
  --json
```

## Keep secrets opaque

A secret field accepts only an `env:` or `ssm:` reference. Composition,
checking, explaining, diffing, and hashing do not read the referenced value.

```toml
[values.database]
connection = "env:ORDERS_DATABASE_URL"
```

The composition root passes a typed `SecretReference` to the adapter that owns
resolution. Domain and application crates never read the process environment.
Plans and diagnostics can contain secret names only where the contract allows;
they never contain secret values.

## Inspect and compare

```bash
cargo minco config schema
cargo minco config check --environment dev --json
cargo minco config explain database.connection --environment dev --json
cargo minco config diff --from dev --to production --json
```

The effective digest is deterministic. Secret explanations are marked
`redacted` and diffs omit both before and after values.

## Common failure modes

- Putting deployment resource profiles into the configuration tree. Keep them
  as Plan IR inputs selected by `deployment_config`.
- Committing `.local.toml` or a password-bearing URL.
- Reading a second untyped set of environment variables inside a handler.
- Treating `config check` as proof the referenced SSM parameter exists; that is
  a later provider/runtime gate.

See the exact [configuration reference](https://github.com/xicv/minco/blob/main/docs/reference/configuration.md).
