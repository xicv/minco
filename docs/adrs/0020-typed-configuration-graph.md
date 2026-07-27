# ADR 0020: Typed environment and secret-reference graph

## Status

Accepted.

## Context

Application defaults, plugin configuration, process environment values and
deployment profiles currently enter Minco through separate ad hoc paths.
Plugin descriptors describe individual fields, but there is no application-wide
schema, fixed precedence, environment classification, provenance, safe diff or
effective digest. A misspelled field can therefore be ignored by a loose
deserializer, and reading arbitrary process variables in application code makes
the effective runtime state impossible to inspect.

Secret values create a stronger boundary. Configuration inspection must know
which runtime adapter should resolve a secret without loading the secret into a
graph, deployment plan, log or CLI response.

## Decision

Minco uses the publishable, provider-neutral `minco-config` crate for one typed
configuration graph. Application fields and enabled-plugin descriptor fields
form one strict schema. Plugin fields retain their stable
`plugins.<id>.<field>` namespace.

The compiler applies exactly this precedence:

1. schema-declared compiled defaults;
2. `default.toml`;
3. `<environment>.toml`;
4. the ignored local override, only for local and development classes;
5. centrally collected `MINCO_CONFIG__...` environment overrides;
6. explicit CLI `--set KEY=JSON-or-string` overrides.

Call-site ordering cannot change precedence, and duplicate precedence layers
fail. Environment files declare one of `local`, `test`, `development`,
`staging`, or `production`. Missing or mismatched classes fail. Test, staging
and production reject local overrides.

Configuration documents have a strict schema-1 header and a `[values]` tree.
Every leaf must match one application field or one field from an enabled,
statically linked plugin. Required fields, unknown paths, type mismatches,
duplicate schema fields and unsafe environment combinations produce stable
`config.*` diagnostics.

Secret fields accept only opaque references:

- `env:EXPLICIT_VARIABLE_NAME`;
- `ssm:/absolute/parameter/name`.

The graph parses and types the reference but never reads the referenced value.
The reference type redacts its name from `Debug`; explain and diff output omit
both resolved values and reference names. Runtime or deployment adapters resolve
references after composition at an explicit boundary.

The effective digest is SHA-256 over canonical, sorted JSON containing schema
version, selected environment and typed effective values. Provenance is not
part of the digest, so equal effective configurations have equal digests even
when their non-secret source paths differ.

`ConfigurationGraph::deserialize_namespace` is the constructor boundary for
application-owned typed settings. There is no global configuration locator.
The `minco` facade re-exports the configuration crate and common types behind
the explicit `config` feature, preserving the minimal facade dependency budget.

`cargo minco config check`, `explain`, `diff`, and `schema` expose stable JSON.
Check, explain and diff validate only the effective enabled-plugin graph.
Schema inspection includes every statically linked plugin so operators can
discover optional configuration before enabling a capability.

## Consequences

- Application and plugin schema drift is rejected before services or provider
  clients are constructed.
- Process environment access is centralized in the CLI composition boundary
  and restricted to one manifest-declared prefix.
- Secret provider names can participate in a deterministic digest without a
  secret value entering the graph.
- Applications deserialize only the namespaces their constructors own.
- Adding a required field to an enabled plugin is a visible compatibility
  change; a required field on a disabled plugin does not invalidate an
  environment.
- Runtime configuration files remain separate from deployment Plan IR files.
  A runtime environment may select the same deployment profile, but one is not
  inferred from the other.

## Compatibility

The new crate, public Rust types, `minco.toml` configuration section and CLI
surface are a likely Minco `0.4.0` boundary. Existing deployment TOML remains
schema-compatible. Existing plugin values in `minco.toml` continue to feed the
legacy composition path while applications migrate them into environment
profiles; the two sources must not be maintained indefinitely.

The migration procedure and examples are documented in
[`../reference/configuration.md`](../reference/configuration.md).

## Safety

Compilation, validation, explanation and diff are local read-only operations.
This decision authorizes no environment-variable secret read, SSM call, AWS
mutation, database connection, migration, crate publication, tag or release.

## Alternatives rejected

### Deserialize directly into one application struct

That loses plugin schema integration, field provenance, stable diagnostics and
safe partial explanation.

### Store resolved secrets in a redacting wrapper

Accidental serialization, hashing or cloning would still place secret values in
the graph. References make the boundary structural.

### Let each crate read its own environment variables

The resulting precedence and effective state cannot be inspected or reproduced,
and misspelled variables remain invisible.

### Use deployment profiles as runtime configuration

Plan IR describes deployable resources, cost and performance. Application
runtime settings and secret references have different ownership and lifecycle.
