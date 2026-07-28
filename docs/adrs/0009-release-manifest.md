# ADR-0009: Build once and promote immutable releases

- Status: Accepted
- Date: 2026-07-23

## Context

Minco needs a small, explicit and durable foundation that can be reasoned about by humans, AI agents, local tooling and deployment planners without duplicating sources of truth.

## Decision

A deterministic, schema-versioned release manifest binds the exact source
revision, every function artifact, contract, redacted configuration digest,
migration and seed catalog digests, lockfile, Plan IR, rendered template,
toolchain identity and optional offline attestations.

`cargo minco package` may execute only the project-declared package commands. It
then renders Plan IR and the deployment template beneath ignored `target/`,
proves that the VCS source revision did not change, seals the manifest, and
verifies every bound file. Packaging does not contact or mutate a deployment
environment.

Each deployment attempt has a separate digest-sealed receipt. It binds one
verified release manifest, exact migration/seed plan files, environment
identity, configuration digest, optional attestations, and verification
evidence. `started` may transition once to `failed` or `succeeded`; both terminal
states are immutable, and success requires verification evidence.

## Consequences

Rebuilding or replanning during promotion destroys provenance. Explicit
package, migration, deploy, verify and terminal receipt stages reduce
environment and data mistakes. Offline signatures can be attached without
making a hosted attestation service part of the trust boundary.

Changes that invalidate this decision require a superseding ADR and migration/compatibility plan.
