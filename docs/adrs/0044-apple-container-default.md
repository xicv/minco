# ADR 0044: Prefer Apple Container for fresh local services

## Status

Accepted

## Context

ADR-0036 established one typed local-service contract with Docker Compose and
Apple Container adapters. Its initial fresh `auto` policy preferred Docker when
both runtimes were ready. That kept the prototype close to its Compose origin,
but it also made a qualified first-party Apple runtime opt-in on supported Macs
and allowed a running Docker daemon to override an intentional Apple Container
migration.

Apple Container `1.2.x` is already a bounded, fail-closed Minco runtime for the
two first-class services Minco owns: PostgreSQL and Rustack. The application
continues to run natively. Docker remains necessary on other platforms and for
application-owned services that exist only in Compose.

Changing a fresh preference must not reinterpret existing data. Lifecycle
receipts and exact owned resources are stronger evidence than a default, and a
PostgreSQL volume cannot be assumed equivalent merely because both runtimes use
the same Minco ownership labels.

## Decision

On a fresh `MINCO_CONTAINER_RUNTIME=auto` start with no exact receipt or owned
resource, Minco prefers a ready, qualified Apple Container `1.2.x` runtime and
falls back to ready Docker Compose. Explicit `apple` and `docker` selections
remain authoritative and fail closed when unavailable.

An existing lifecycle receipt still selects its exact runtime. Without a
receipt, Minco inspects every ready runtime: one exact owned resource is reused,
matching resources in both runtimes remain an ambiguity error, and only the
absence of either resource permits the new Apple-first preference.

Docker and the project-owned Compose file remain supported. Compose-only custom
services are not projected into Apple Container. Minco does not automatically
copy, compare, reset, or delete persistent volumes; migration and deletion need
separate explicit authority and evidence.

## Consequences

- Supported Apple silicon development hosts use Apple Container without an
  environment override once its system is ready.
- Docker remains the portable fallback and explicit customization boundary.
- Existing projects do not change runtimes mid-lifecycle because receipts and
  owned resources outrank the fresh default.
- A deliberate Docker-to-Apple data move must validate PostgreSQL contents
  before the Docker volume is deleted.
- Production deployment, Plan IR, AWS behavior and cloud cost are unchanged.

## Compatibility

This changes only fresh local `auto` selection. Set
`MINCO_CONTAINER_RUNTIME=docker` to retain Docker explicitly or
`MINCO_CONTAINER_RUNTIME=apple` to require Apple Container. The immutable
`1.6.0` manual continues to describe its released Docker-first behavior; the
new preference belongs to the next release line.
