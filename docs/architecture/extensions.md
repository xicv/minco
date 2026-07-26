# Extension and Plugin Model

Minco has a provider-neutral core and statically linked plugins. A plugin is an
ordinary Rust crate implementing `minco_core::Plugin`; it publishes a validated
`PluginDescriptor`, installs typed services and ordered contributions, and may
finalize aggregate registries after all plugins have installed.

Descriptors declare:

- plugin identity, semantic version, core compatibility, stability and docs;
- plugin dependencies and provided/required capability versions;
- data-sensitivity classes and a strict configuration schema;
- OpenAPI operations;
- migration sets and health checks;
- resource intents, dependencies, wake sources and idle-cost class.

The manager selects defaults plus explicit enable/disable configuration, applies
configuration defaults, resolves dependencies in deterministic topological
order, and validates the complete graph **before** constructing services. It then
runs deterministic install and finalize phases and freezes the service and
contribution registries.

Startup fails on unknown/contradictory configuration, duplicates, missing or
disabled dependencies, core/capability incompatibility, cycles, route conflicts,
migration conflicts, or resource conflicts.

## Binding model

- `ServiceCollection`: one authoritative implementation of a type.
- `ContributionCollection`: ordered zero-to-many implementations of a type.
- `compose_with`: application composition-root injection for database pools,
  AWS clients, clocks and product-specific adapters.
- `HttpModule`: a fully state-bound Axum router fragment plus exact operation
  inventory and request-body budget.

There is no global service locator. Runtime code receives concrete services or
narrow application ports through constructors.

After successful composition, frozen registries retain bounded provenance:
singleton summaries contain the Rust type and authoritative application/plugin
owner; contribution summaries are grouped by Rust type and retain owner plus a
global deterministic installation index. `RegistrationOwner` has no public
constructor, so a plugin cannot claim another plugin's identity. These
summaries are inspection metadata only: values, configuration, URLs,
credentials and provider diagnostics are never serialized.

## Selection model

Cargo features decide what code is present in the binary. `PluginSelection`
then enables or disables registered plugins and supplies validated runtime
configuration. Required plugin dependencies are auto-enabled unless explicitly
disabled, in which case composition fails closed.

The catalog is metadata, not executable discovery. Plugin code must be a
reviewed Cargo dependency and explicitly registered. Runtime package scanning,
global facades and dynamic shared libraries are intentionally out of scope
because they weaken type safety, determinism, security review and deployment
analysis.

## Official tiers

The bounded default plugins are:

- health;
- structured observability;
- idempotency.

Opt-in official plugins cover:

- sessions and CSRF primitives;
- verified identity, scopes and permissions;
- object storage and direct-access signing ports;
- events and explicit outbox leases;
- notifications;
- append-only audit;
- static-site deployment intent;
- the Feedback client-review vertical slice.

Official SQLx PostgreSQL/SQLite and AWS Lambda crates are provider/runtime
extensions. Concrete S3, SQS, SES, Cognito and CloudFront renderers are tracked
separately; memory implementations are reference adapters and must not be
represented as production durability.

See [`plugin-authoring.md`](plugin-authoring.md) for the public author workflow
and [`capability-audit.md`](capability-audit.md) for coverage against GarmentIQ
and CGSP.
