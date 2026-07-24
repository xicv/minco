# Extension and Plugin Model

Minco has a strong provider-neutral core and statically linked plugins. A plugin
is an ordinary Rust crate implementing `minco_core::Plugin`; it returns a
`PluginDescriptor` and installs typed services through `PluginContext`.

Descriptors declare:

- plugin identity/version and plugin dependencies;
- provided and required capability versions;
- OpenAPI operations;
- migration sets and health checks;
- resource intents, dependencies, wake sources and idle-cost class.

The manager selects defaults plus explicit enable/disable configuration, resolves
dependencies in deterministic topological order, injects typed services and then
builds the validated application graph. Startup fails on duplicates, missing or
disabled dependencies, cycles, capability mismatches, route conflicts,
migration conflicts or resource conflicts.

The catalog is configuration, not executable discovery. Plugin code must be a
reviewed Cargo dependency and explicitly registered in the composition root.
Runtime package scanning, global containers, facades and dynamic shared libraries
are intentionally out of scope because they weaken type safety, determinism and
deployment/cost analysis.

Default plugins:

- health;
- structured observability;
- idempotency.

Official extensions initially cover SQLx PostgreSQL, SQLx SQLite and native AWS
Lambda. Future S3, SQS/outbox, SES, OIDC/Cognito and static-site plugins must use
the same descriptor, health, local-emulation, IAM and cost conventions.

See [`plugin-authoring.md`](plugin-authoring.md) for the public author workflow.
