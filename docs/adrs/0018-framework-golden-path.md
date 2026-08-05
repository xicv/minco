# ADR 0018: Define Minco by a five-plane contract-to-cloud golden path

## Status

Accepted.

## Context

Minco 0.3.1 has a strong OpenAPI contract system, modular architecture, static
plugin kernel, SQLx boundaries, AWS/local adapters, SQS worker runtime, Plan IR,
release manifests, JJ workflow, and quality/release evidence. The pieces are
individually mature, but the application lifecycle remains spread across
generated source, scripts, delegated commands, and documentation.

Adding more unrelated framework primitives would not prove that a developer can
move coherently from a contract to a safely operated application. The framework
needs a stable product definition, completion criteria, and sequencing boundary
before its next serialized runtime redesign.

## Decision

Minco's product identity is:

> Minco is the contract-to-cloud framework for building, operating, and
> evolving low-idle-cost Rust web applications through one inspectable
> application graph.

That graph connects five planes:

1. contract;
2. code;
3. capabilities;
4. resources;
5. evidence.

Framework completion is defined by one developer and deployment golden path:

```text
new -> contract -> generate -> dev -> migrate -> seed -> test -> inspect
    -> package -> change set -> migrate target -> deploy -> verify
    -> promote exact artifact -> observe or compatibility-checked rollback
```

The implementation sequence is:

1. framework definition and documentation map;
2. M6-T10 trigger-aware multi-runtime Plan IR;
3. typed configuration;
4. migration lifecycle;
5. safe classified seeders;
6. graph-driven local development;
7. contract-aware generators and compatibility reporting;
8. safe deployment controller;
9. plugin ecosystem and versioned documentation;
10. local AI tooling and 1.0 compatibility freeze.

The detailed product definition, 1.0 criteria, non-goals, and evidence model are
in
[`../vision/minco-framework-definition.md`](../vision/minco-framework-definition.md).

## Consequences

- M6-T10 is the next runtime implementation task after this docs-only decision
  is reviewed.
- Application-lifecycle work is split into bounded JJ tasks and PRs; it does
  not become a mega-change.
- Configuration, migrations, seeds, processes, resources, and evidence acquire
  stable graph identities and provenance before local/deployment controllers
  automate them.
- The default low-idle-cost profile and explicit provider boundaries remain
  unchanged.
- Documentation becomes a versioned product with generated reference rather
  than manually synchronized exhaustive lists.
- Plugin installation tooling may plan deterministic application edits but
  cannot introduce runtime discovery.
- AI MCP/workbench work waits for stable read models and is local/read-only by
  default.
- A 1.0 release is gated on two-application evidence and explicit public API,
  CLI, feature, configuration, and serialized-schema review.

## Alternatives rejected

### Continue adding independent primitives

This would increase surface area without proving the end-to-end lifecycle or
clarifying 1.0 completion.

### Reproduce Laravel's runtime model

Global facades, runtime package discovery, Active Record, and boot-time side
effects conflict with Minco's typed static composition, explicit SQL, and
inspectable deployment model.

### Implement the entire lifecycle in one change

Plan IR, configuration, databases, process supervision, deployment, ecosystem,
and AI tooling have different compatibility and operational risks. One change
would prevent meaningful review and exact evidence.

### Build a hosted Minco control plane

The framework remains self-hostable and AWS-account-owned. Local tooling and
portable evidence are sufficient for the target product.

## Compatibility

This ADR changes documentation and roadmap only. It changes no Rust API,
serialized schema, Cargo feature, dependency graph, generated deployment
artifact, or runtime behavior.

M6-T10 remains a likely `0.4.0` boundary. Later tasks must state their own
public and serialized compatibility impact.

### 1.0 compatibility addendum (2026-08-05)

M12-T04 completed the source-level review required by this ADR. The intended
1.0 boundary includes every rustdoc-visible public item in the 32 publishable
packages, all named Cargo features and defaults, the complete CLI and its
machine-readable outputs, configuration schema 1, Plan schemas 1 and 2,
release manifest schema 3, schema-1 deployment/plugin/project-view records,
diagnostic codes and Rust `1.97.1` as the MSRV.

The complete inventory and post-1.0 change rules are maintained in the
[compatibility policy](../reference/compatibility.md). Catalog maturity does
not weaken SemVer: a beta component remains opt-in and evidence-bounded, but a
published 1.x Rust, feature, CLI or serialized boundary cannot break without
the required version transition. Strict schema evolution uses a new version
plus an explicit old reader or migration path; receipt digest payloads are not
silently extended.

This is a source freeze, not evidence of merge, tag, registry publication,
application adoption, provider execution or production promotion.

## Safety

This decision authorizes no AWS mutation, database mutation, crate upload, tag,
release, or product-repository change.
