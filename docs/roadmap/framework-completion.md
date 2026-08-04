# Framework Completion Program

Status: Accepted roadmap
Program baseline: Minco `0.3.1`
Planning task: `M9-T01`

## Purpose

This program turns Minco's strong architectural kernel into one coherent
contract-to-cloud application lifecycle. The product definition and completion
criteria live in
[`../vision/minco-framework-definition.md`](../vision/minco-framework-definition.md).
The repository roadmap and task records remain the execution source of truth.

The program is Minco-only. CGSP, GarmentIQ, and other products may provide
read-only adoption evidence through separately authorised work, but this
program does not modify their repositories.

## Sequencing

```text
M9-T01 framework definition and documentation map
    |
    +--> M6-T10 trigger-aware multi-runtime Plan IR
             |
             v
M9 application lifecycle and developer experience
    |
    v
M10 safe deployment controller
    |
    v
M11 plugin ecosystem and documentation
    |
    +--> M7 bounded two-application evidence
    |
    v
M12 AI workbench and 1.0 preparation
```

M6-T10 is the next runtime task after this RFC is reviewed. It remains its own
minor-version implementation PR. No Phase 1 runtime work belongs in M9-T01.

## PR boundaries

Every task uses one JJ workspace and one coherent PR. In particular:

- framework definition and roadmap changes do not contain Plan IR code;
- Plan IR does not contain typed configuration or database lifecycle work;
- migrations and seeders are separate design/implementation boundaries;
- the local supervisor does not absorb the deployment controller;
- plugin distribution, conformance, and installation workflows stay reviewable;
- documentation generation does not become runtime discovery;
- project views render existing authoritative read models rather than owning
  another progress state machine;
- MCP/workbench work starts only after the JSON models it exposes stabilise;
- publication, AWS mutation, and product adoption remain separate approvals.

## M9 — Application lifecycle and developer experience

Outcome: a developer can configure, migrate, seed, run, generate, and assess
compatibility through one graph-driven control plane.

| Task | Deliverable | Exit signal |
|---|---|---|
| M9-T01 | Framework definition, ADR, documentation map, and roadmap | Docs-only draft reviewed |
| M9-T02 | Typed configuration/environment/secret-reference graph | Strict redacted effective config with provenance |
| M9-T03 | Database status/plan/migrate/verify | Locked, drift-aware migration receipts |
| M9-T04 | Classified seeders and deterministic fixtures | Production demo/test seeds fail closed |
| M9-T05 | Graph-driven `cargo minco dev` | Declared processes start/stop together without AWS |
| M9-T06 | Contract-aware generators and app-owned stubs | Deterministic dry-run edits plus failing tests |
| M9-T07 | OpenAPI compatibility diff and upgrade report | Breaking/non-breaking report with explicit limits |

M9 is complete when a generated PostgreSQL and SQLite application can follow
the local golden path using documented commands, without hidden provider work
or fake business implementations.

## M10 — Safe deployment controller

Outcome: an operator can preview, apply, verify, promote, and assess rollback of
one exact release without rebuilding.

| Task | Deliverable | Exit signal |
|---|---|---|
| M10-T01 | Package and deployment receipts | Source/artifact/config/migration digests bind one release |
| M10-T02 | CloudFormation change sets and environment guards | No apply before account/region/change review |
| M10-T03 | Hosted verification and exact-artifact promotion | Promotion changes traffic, not source |
| M10-T04 | Rollback compatibility and optional canary aliases | Explicit compatible/incompatible rollback result |
| M10-T05 | Static-site and custom-domain completion | Byte/hash/cache/domain/invalidation evidence |
| M10-T06 | Preview TTL, cost, and cleanup | Expiry and cleanup are guarded and receipted |
| M10-T07 | Zero-idle service and cost research | Dated profiles keep correctness, wake and pricing limits explicit |
| M10-T08 | Bounded real-AWS controller rehearsal | Exact apply, hosted verification, promotion, rollback and cleanup evidence |

M10 closure is supported by a bounded real-AWS rehearsal of the documented
controller path. Local/SAM validation alone is not deployment proof.

## Current transition after the real-AWS closure

The 2026-08-03 repository-truth audit closes M9: all nine tasks are complete,
its M4 prerequisite is complete, and its PostgreSQL/SQLite golden paths,
strict lifecycle controls, generators, resource contract and local-first CI
exit signals are recorded in the owning tasks.

The separately approved M10-T08 rehearsal then applied and promoted an exact
prior release, an exact current release and the exact prior rollback target.
Each phase passed fresh hosted verification. Rollback compatibility was
explicitly `compatible`, no source or artifact was rebuilt, and independent
cleanup verification proved every bounded AWS resource class absent. The
repository record is redacted and retains no account, role, endpoint,
credential or resource identifier.

M10 is therefore `complete`. M7 and M11 also become `complete` because their
task sets, direct exit signals and M10 prerequisite are complete. M12 becomes
`active`, with M12-T01 as the single dependency-ready source task. Readiness
does not merge a pull request, authorize a provider, deploy an application or
publish a crate. M12-T03 continues to depend on M7-T02, the actual GarmentIQ
contract-only evidence, rather than the earlier M7-T01 gap audit.

## M11 — Plugin ecosystem and documentation

Outcome: application developers, plugin authors, operators, contributors, and
agents can discover and verify the framework without manually synchronised
inventories.

| Task | Deliverable | Exit signal |
|---|---|---|
| M11-T01 | Versioned Diátaxis documentation site | Searchable versioned site with checked links/snippets |
| M11-T02 | Plugin distribution manifest | Static/runtime metadata drift fails deterministically |
| M11-T03 | Shared plugin conformance kit | Official and third-party-style fixtures use one kit |
| M11-T04 | Plugin add/init/explain/test workflow | Planned deterministic edits, explicit registration |
| M11-T05 | Examples and recipes matrix | Supported lifecycle/provider combinations are exercised |
| M11-T06 | Generated feature/plugin/diagnostic reference | README/docs inventories derive from metadata |
| M11-T07 | Deepened documentation site | Versioned content and browser journeys remain complete |
| M11-T08 | Minco 0.6.0 release | Exact source, package and registry evidence remain separate |
| M11-T09 | Expanded documentation catalog | Current framework workflows are discoverable and checked |
| M11-T10 | Repository-native project-view design | One bounded read model keeps status and evidence authority explicit |

M11 does not create a hosted plugin registry or runtime plugin loader.
M11-T10 defines the M12 read-model and presentation contracts; it does not
itself implement the MCP or workbench.

## M12 — AI workbench and 1.0 preparation

Outcome: stable read models support local AI tooling, adoption evidence closes
the compatibility loop, and the release candidate passes an explicit freeze.

| Task | Deliverable | Exit signal |
|---|---|---|
| M12-T01 | Project read models and local read-only MCP server | Bounded versioned tools expose no secrets or arbitrary shell |
| M12-T02 | Optional local developer workbench and project views | One read model powers diagrams, evidence-aware progress and accessible narration |
| M12-T03 | Second-application adoption completion | Two bounded real slices produce framework evidence |
| M12-T04 | Public API and Cargo-feature audit | Rust/CLI/config/Plan/plugin surfaces are frozen |
| M12-T05 | Security, recovery, load, and docs gates | Exact RC source passes every mandatory gate |
| M12-T06 | Minco 1.0 release candidate | Exact tag candidate and external consumer proof |

The MCP and workbench are local-only. Write operations remain disabled unless a
future ADR defines explicit local grants.

## Deferred capabilities

The golden path is not blocked on ORM, templates, Redis, cache, distributed
rate limiting, feature-flag services, realtime transports, localization,
search, GraphQL, multi-cloud, Kubernetes, ECS/Fargate, Step Functions, generic
workflow engines, admin UI, or product business modules.

M6-T01 remains an explicit DynamoDB access-pattern task, but it is not a
prerequisite for M9–M12. It must not emulate relational ports.

## Evidence and review gates

Each implementation task records:

- starting `main` SHA, JJ change, workspace, bookmark, and PR;
- changed public API, serialized schema, features, dependencies, and version
  impact;
- exact PASS/FAIL/BLOCKED/NOT RUN status for every required check;
- dependency, build, artifact, connection, schedule, and cost measurements
  where relevant;
- framework, provider, live-AWS, documentation, application-adoption, and
  business gaps separately;
- confirmation that no product, registry, tag, AWS, database, or secret
  boundary was crossed without explicit authority.

No milestone becomes complete merely because source was written. Its task
checks, reviewed prerequisites, and stated external evidence must be satisfied.
