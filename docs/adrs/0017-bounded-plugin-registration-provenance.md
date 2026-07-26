# ADR 0017: Bounded plugin registration provenance

## Status

Accepted.

## Context

Minco's typed registries correctly rejected duplicate singleton services and
preserved contribution order, but they did not retain who registered a value.
That made a safe composition failure difficult to diagnose in applications
with application-seeded adapters, auto-enabled dependencies, and several
plugins contributing the same trait-object type.

Provenance must not turn the registries into string-key lookup, expose concrete
values, allow a plugin to claim another plugin's identity, or add work to
request paths.

## Decision

1. `ServiceCollection` and `ContributionCollection` remain `TypeId`-keyed.
2. Direct collection registration is application-owned. `PluginContext` and
   `PluginFinalizeContext` return owner-bound registrar views whose opaque
   plugin owner is created internally from the effective descriptor selected
   by `PluginManager`.
3. A duplicate singleton reports the Rust type plus the first and attempted
   owners. It never replaces the first value.
4. Contributions retain a global deterministic installation index. Frozen
   metadata is grouped and sorted by Rust type while registrations within each
   group remain in installation order.
5. Frozen registries expose metadata-only summaries. `ComposedApplication`
   combines those summaries as `RegistrationProvenance`; mutable registries and
   graph-only planning expose no provenance.
6. `cargo minco inspect --json` composes the manifest-selected, statically
   linked plugin set and emits only the bounded summaries. Plugin lifecycle
   hooks remain subject to the existing deterministic, no-network,
   no-migration, no-background-work contract.

`RegistrationOwner` is inspectable and serializable but has no public
constructors. A plugin cannot pass an arbitrary owner string or `PluginId`.
Application registrations are explicitly serialized as `application`; plugin
registrations include the authoritative plugin ID.

## Pre-1.0 API impact

Normal plugin call sites remain source-compatible:

```rust
context.services().insert(service)?;
context.contributions().push(contribution);
```

The return types of `PluginContext::services`,
`PluginContext::contributions`, and `PluginFinalizeContext::services` are now
owner-bound registrar views rather than direct mutable collection references.
Third-party code with an explicit `&mut ServiceCollection` or
`&mut ContributionCollection` type annotation must remove that annotation or
accept the registrar type.

`ServiceError::Duplicate` keeps its variant name but its payload is now
`DuplicateServiceRegistration`, carrying `rust_type`, `first_owner`, and
`attempted_owner`.

## Consequences

- Duplicate diagnostics and inspection are deterministic for an exact binary.
- Trait-object `Shared<T>` registration and `compose_with` application
  injection retain their typed behavior.
- Frozen registries retain small immutable metadata alongside values; request
  handlers do not perform provenance work.
- Rust type names are diagnostic identities, not stable business or plugin
  capability IDs.
- Service values, configuration values, URLs, credentials, concrete `Debug`
  output, and provider diagnostics are never part of registration provenance.
- Failed composition returns no partially frozen application.
