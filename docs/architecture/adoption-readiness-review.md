# Minco adoption-readiness architecture review

Date: 2026-07-26
Candidate: `0.2.0`
Published baseline: `0.1.1`
Verdict: `ready_with_explicit_gaps`

This review is repository-scoped. It does not edit or claim validation of CGSP
or GarmentIQ.

## 1. Does `minco-core` remain provider and transport neutral?

Yes. Its manifest has no direct Axum, SQLx, Lambda or AWS SDK dependency.
Repository dependency-hygiene checks and the all-feature workspace compiler
gate protect that boundary. Domain and application crates retain the stricter
dependency direction described in `AGENTS.md`.

## 2. Can every official plugin be omitted at compile time?

Yes. Each catalog plugin maps to an optional facade dependency and feature.
`official-plugins` is an explicit aggregate; it is not part of `default`.
Static validation checks both catalog-to-facade and facade-to-catalog coverage.

## 3. Can default plugins be disabled safely at runtime?

Yes within declared dependencies. Health, observability and idempotency are
compile-time defaults, while runtime selection remains explicit. Graph
validation rejects missing capabilities and dependency cycles before
composition; it does not silently install a disabled plugin.

## 4. Does graph validation precede provider construction or remote calls?

Yes. The typed descriptor graph is built and validated before the composition
root selects concrete adapters. Plugin graph construction does not build AWS
clients or make network calls. Provider-specific startup remains in runtime or
adapter crates.

## 5. Are install/finalize hooks deterministic and side-effect bounded?

Yes for the current catalog: ordering is deterministic and hooks perform typed
registration/contribution, not migrations, network access or detached work.
Production migrations remain explicit release operations. The residual gap is
diagnostic provenance, owned by M6-T07.

## 6. Are capabilities separate from provider resources and IAM?

Yes. Provider-neutral capabilities describe what a plugin needs; resource
intents and IAM markers describe how a selected provider satisfies it. The
minimal AWS profile continues to reject NAT, fixed compute, schedules and
provisioned concurrency.

## 7. Can an existing application inject adapters without a locator?

Yes. Application-owned, use-case-shaped ports are supplied through typed
composition. There is no string lookup, global service locator, runtime plugin
scan or dynamic library boundary. M6-T07 will improve owner names in duplicate
registration diagnostics without changing that model.

## 8. Are routes, ownership and OpenAPI inventory bijective?

The canonical OpenAPI inventory drives generated bindings, the application
slice and Plan/SAM routes. Contract and static checks reject duplicate
operation IDs and compare generated route inventory. Plugin HTTP modules carry
their operation/header contribution explicitly. Registration owner provenance
is the one remaining inspectability gap and is not falsely claimed complete.

## 9. Is the worker business-neutral and explicitly scheduled?

Yes. `minco-aws-worker` exposes validated strings and attributes, not product
event schemas. It has no AWS SDK client dependency, queue creator, poller,
timer, detached task or hidden event-source mapping. It returns partial-batch
failures, bounds size/concurrency, fails FIFO forward, and redacts payload and
attribute values from `Debug`.

## 10. Do the default facade and minimal Lambda stay small?

Yes against the immutable `0.1.1` baseline: no-default, default and
official-plugin normal dependency counts are unchanged. The candidate Orders
ARM64 ZIP remains below the 10 MiB compressed budget. `aws-worker`, SQLx and
AWS adapters are opt-in. Exact bytes and methodology are recorded in
`verification/adoption-measurements.json`.

## 11. Are status claims current rather than inherited?

Repository truth now cross-checks candidate version, publish inventory,
catalog, facade, descriptors, roadmap contradictions, Plan descriptors and
measured budgets. Compiler/runtime, generated consumer, browser, security,
package and hosted results remain separate claims in `VERIFICATION.md`.
Docker-backed PostgreSQL/Rustack checks are explicitly not converted into a
pass when the local shared Docker daemon is unavailable.

## 12. Is Minco ready for a reversible existing-application pilot?

`ready_with_explicit_gaps`. A pilot may begin only after this candidate's draft
PR and exact-head hosted quality gate are reviewed. The safe pilot is one
contract-first operation behind a compatibility switch, with existing
deployment, migration, authorization and rollback tooling kept authoritative.

Remaining owners:

- M6-T07: plugin registration/contribution provenance and conflict diagnostics.
- M7-T01: the separately authorized two-application validation boundary.
- Pilot owner: provider budgets, data classification, rollback evidence and
  runtime parity.
- Worker Plan follow-up: explicit SQS/DLQ/event-source Plan IR if a pilot
  requires framework-rendered worker infrastructure.

No product migration is part of M6-T06.
