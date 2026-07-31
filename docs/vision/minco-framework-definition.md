# Minco Framework Definition

Status: Accepted product direction for the framework-completion program
Decision date: 2026-07-27
Published baseline: `0.4.0`
Current workspace version: `0.5.0`
Workspace release state: `candidate`
Reviewed release source: `65bf94045448bdbeedd37e10b1a004c926513508`

## Product identity

> Minco is the contract-to-cloud framework for building, operating, and
> evolving low-idle-cost Rust web applications through one inspectable
> application graph.

Minco is not a replacement language, ORM, hosted control plane, or attempt to
recreate Laravel in Rust. It is an explicit lifecycle that connects an
application's public contract to ordinary Rust business code, statically linked
capabilities, provider resources, cost, deployment, and verifiable evidence.

The published `0.4.0` classification is:

> Published-source coherent through guarded hosted verification, a bounded
> disposable AWS rehearsal, exact-artifact promotion and cleanup; production
> runtime and later lifecycle/ecosystem programs remain incomplete.

The framework-completion program still prioritises one coherent path from a new
contract to a safely deployed, observable, and upgradable application. Source
and bounded release qualification do not substitute for production proof,
rollback/canary, static-site domains, review-environment cleanup, a
documentation product, multi-application adoption or the 1.0 freeze.

## The five-plane application graph

The graph is one model viewed through five planes, not five independent sources
of truth.

```text
Contract
  OpenAPI operations, schemas, auth, errors, examples, compatibility
        |
        v
Code
  domain, application use cases, ports, adapters, HTTP delivery
        |
        v
Capabilities
  static plugins, typed services, contributions, configuration
        |
        v
Resources
  databases, queues, storage, workers, schedules, IAM, cost, deployment
        |
        v
Evidence
  tests, migration and seed receipts, releases, hosted verification, rollback
```

Every plane must retain stable identities and provenance. A change in one plane
must have deterministic, inspectable consequences in the others:

```text
contract change
  -> implementation structure
  -> capability and resource consequences
  -> local and provider checks
  -> immutable artifact and manifest
  -> guarded deployment
  -> hosted evidence
  -> exact-artifact promotion
```

The graph must never contain secret values. It may contain opaque secret
references, sensitivity classification, ownership, and the evidence required to
prove that a selected provider satisfies an application capability.

## Architectural invariants

The framework-completion program preserves the accepted ADRs and
`AGENTS.md`. In particular:

1. OpenAPI 3.1 remains the canonical external HTTP contract.
2. Domain and application code remain ordinary Rust with inward dependencies.
3. Axum and Tower remain the HTTP runtime; Minco adds conventions, not a second
   router.
4. SQLx and visible SQL remain the persistence foundation; no ORM or generic
   CRUD repository enters core.
5. Plugins remain statically linked, explicitly constructed, and typed.
6. Composition performs no network calls, migrations, or detached background
   work.
7. The provider-neutral core does not depend on Axum, SQLx, Lambda, or cloud
   SDKs.
8. The default AWS profile retains no NAT Gateway, fixed application compute,
   provisioned concurrency, or scheduled wakeup. Zero provisioned application
   compute never hides storage, retained-log, DNS, secret, database, request,
   schedule or fixed dimensions.
9. Production migrations remain explicit release operations.
10. Promotion uses an already verified artifact and manifest; it never rebuilds.
11. Provider correctness, wake sources, connection pressure, cost, and residual
    operational gates remain visible.
12. Product-specific concepts do not enter framework core or official
    infrastructure plugins.

## Current maturity

| Area | Current `0.5.0` candidate source state | Remaining boundary |
|---|---|---|
| Core architecture | Strong | Preserve and stabilise |
| Static plugin kernel | Strong | Distribution metadata and conformance |
| OpenAPI contract | Strong constrained profile plus structural diff and operation generators | Measured adoption and compatibility freeze |
| HTTP runtime | Strong | Adoption fixtures, not redesign |
| PostgreSQL and SQLite | Status, digest plan, lock, apply, verify and classified seeds | Live application evidence and operational recipes |
| Events and SQS worker | Trigger-aware Plan IR, SAM, local projection and bounded runtime | Live multi-runtime rehearsal |
| Provider adapters | Broad and explicit | Operational recipes and evidence |
| Feedback | Stable vertical slice and Verified Review Loop foundation | Optional review-environment/delivery trace |
| Local infrastructure | Graph-driven `cargo minco dev` with supervised process groups | Broader generated-app adoption |
| Deployment Plan IR | Schema 2 API, workers, queues, DLQs, mappings and schedules | Profile research and compatibility freeze |
| Deployment controller | Exact release, change set, apply, receipts, hosted verify, promote and bounded disposable AWS rehearsal | Rollback/canary and static-site domains |
| Configuration | Unified typed environment and opaque secret-reference graph | Measured application adoption |
| Migrations | Status, plan, drift, lock, apply, verify and receipt | Live target rehearsal |
| Seeders and fixtures | Classified, idempotent, preservation-aware plans | Live application policy evidence |
| Generators | Contract-aware vertical-slice family and app-owned stubs | Stabilisation through generated consumers |
| Documentation | Substantial checked Markdown plus `0.4.0` and candidate `0.5.0` upgrade guides | Versioned Diátaxis product |
| AI support | Stable paths and JSON inspection | Local read-only MCP and optional workbench |
| Compatibility | Pre-1.0 policy | Explicit public API and feature freeze |

## Developer golden path

The target application lifecycle is:

```text
cargo minco new
  -> define or change OpenAPI
  -> generate a vertical-slice structure and failing tests
  -> cargo minco dev
  -> inspect configuration, capabilities, resources, and cost
  -> plan/apply/verify migrations
  -> run an explicit seed profile where permitted
  -> test and package
  -> preview a deployment change set
  -> migrate the target explicitly
  -> apply the exact artifact
  -> run hosted contract/readiness/smoke verification
  -> promote the exact artifact
  -> observe or perform a compatibility-checked rollback
```

The golden path is cohesive but not magical. Every generated file is ordinary
reviewable source. Every mutating command must support a preceding plan or
dry-run where meaningful, apply environment guards, and emit stable evidence.
Local defaults must not contact AWS, run schedules, reset data, or select
undeclared services.

### Application-developer journey

An application developer can:

- generate or incrementally adopt a layered application;
- change canonical OpenAPI and trace an operation through code and tests;
- run only graph-declared local dependencies, API processes, and workers;
- understand effective typed configuration without seeing secret values;
- plan and verify migrations and permitted seed profiles;
- package and hand off an exact artifact with complete local evidence.

### Plugin-author journey

A plugin author can:

- create an ordinary Cargo crate with a real descriptor;
- declare typed services and contributions, configuration, operations,
  migrations, seeds, resources, health, sensitivity, wake sources, and cost;
- run the same conformance kit used by official plugins;
- publish compatibility and evidence metadata without runtime discovery;
- provide explicit installation instructions and deterministic app-owned edits.

### Operator journey

An operator can:

- inspect the target account, region, environment, resource, IAM, cost, and
  migration plans before mutation;
- review a CloudFormation change set and destructive/replacement classification;
- apply one verified release manifest and receive a deployment receipt;
- run hosted verification, promote by alias without rebuilding, and assess
  rollback compatibility;
- distinguish framework evidence from provider, live-AWS, and business gates.

### Contributor journey

A contributor can:

- select one repository task in one JJ workspace;
- inspect accepted decisions, task ownership, and exact prerequisites;
- run focused checks and the repository's authoritative quality path;
- record exact evidence without rewriting historical release results;
- open one coherent review boundary without mixing runtime, deployment, and
  release work.

### AI coding-agent journey

An AI coding agent can:

- inspect stable files, JSON commands, task dependencies, and provenance;
- explain an operation and its capabilities/resources without inferring hidden
  runtime structure;
- read redacted configuration and evidence models;
- use a local read-only MCP only after its underlying schemas stabilise;
- require explicit capability grants for any future write tool.

## Deployment golden path

The generic controller is built only after multi-runtime planning,
configuration, and database lifecycle models are stable:

```text
cargo minco package
cargo minco deploy plan
cargo minco deploy changeset
cargo minco db plan
cargo minco db migrate
cargo minco deploy apply
cargo minco deploy verify
cargo minco promote
cargo minco rollback
```

Required safeguards:

- expected AWS account, region, environment, and role;
- clean exact source and verified release manifest;
- no secret values in plans, templates, logs, or receipts;
- change-set preview before infrastructure mutation;
- replacement, deletion, drift, wake, cost, and connection visibility;
- database plan and explicit lock before migration;
- exact artifact deployment and hosted contract/readiness/smoke verification;
- a durable deployment receipt;
- promotion without rebuilding;
- optional alias-based canary policy with alarms;
- rollback compatibility checks without promising arbitrary SQL reversal;
- static-site byte, hash, cache, custom-domain, and invalidation evidence.

CloudFormation change sets improve reviewability but do not guarantee that a
runtime update will succeed. Hosted verification and rollback planning remain
separate evidence.

## Database and seed safety model

Migrations and seeders become first-class lifecycle objects, not startup side
effects.

A migration records stable identity, owner, digest, dependency order,
applied/pending/drift state, destructive risk, direct migration connection,
lock, verification, and execution receipt. Minco does not claim that arbitrary
SQL can be reversed automatically.

Seed plans use four classes:

| Class | Permitted intent |
|---|---|
| `reference` | Approved deterministic reference data |
| `demo` | Local and development data only |
| `test` | Disposable test databases only |
| `bootstrap` | Explicit environment allowlist and operator approval |

Backfills remain migrations. A seed declares stable identity/version, owner,
environment allowlist, dependencies, idempotency, mutable-state ownership,
preservation rules, destructive risk, transaction behavior, verification, and
digest. Production demo/test seeding fails closed.

## Plugin ecosystem contract

Plugin code remains an explicit Cargo dependency and composition-root
registration. Distribution tooling may plan deterministic Cargo, catalog,
configuration, and constructor edits, but it never scans or executes packages
at runtime.

A public plugin distribution record must describe:

- Minco core and capability compatibility;
- plugin version and stability;
- Cargo feature and dependency consequences;
- typed configuration schema and opaque secret-reference needs;
- supported databases and runtimes;
- operations and exact HTTP headers;
- migrations and seeds;
- resources, IAM, wake sources, and cost class;
- health behavior;
- data sensitivity, retention, and failure semantics;
- conformance evidence;
- tutorial, how-to, reference, and explanation links.

Static metadata complements the runtime descriptor; drift between them must be
checked deterministically.

## Compatibility and version boundaries

Minco remains a lock-step pre-1.0 crate family while the golden path is built.

- Patch releases preserve the public Rust API and serialized contract of the
  current minor line.
- A breaking public Rust API, serialized Plan IR, CLI contract, feature
  boundary, or application configuration schema advances the left-most
  non-zero version component.
- The work through M10-T03 is the `0.4.0` boundary because it changes public
  serialized planning/configuration/release structures, package inventory and
  the lifecycle/deployment CLI.
- The resource response/concurrency contract and structured Plan cost evidence
  advance the unpublished candidate to `0.5.0`.
- New Cargo features stay opt-in unless a separately reviewed default-surface
  decision proves the dependency and behavior impact.
- Every significant public change includes a compatibility note, migration
  guide, fixture, and exact final-source checks.
- The 1.0 candidate requires an explicit public API, Cargo feature, CLI,
  configuration, Plan IR, and plugin-distribution freeze.

The current Rust MSRV remains the manifest-pinned `1.97.1`. Changing it is an
explicit compatibility decision with its own evidence.

## Complete enough for 1.0

Minco may call itself complete enough for 1.0 only when all of these are true:

1. A generated application reaches a real AWS deployment through the documented
   golden path.
2. API, worker, queue, DLQ, mapping, and explicit schedule topology is modeled.
3. Typed configuration, migrations, and seeders are first-class and safe.
4. `cargo minco dev` runs the graph-defined local environment.
5. Contract-aware generators create structure and failing tests without fake
   business behavior.
6. Deployment uses change sets, exact artifacts, hosted verification, and
   receipts.
7. At least one third-party-style plugin passes the public conformance kit.
8. CGSP and GarmentIQ each provide evidence of a bounded real Minco slice
   without product concepts entering the framework.
9. Tutorials, how-to guides, reference, and explanations are versioned,
   searchable, linked, and checked.
10. Public Rust APIs, Cargo features, CLI and serialized schemas receive an
    explicit compatibility freeze.
11. Security, recovery, load, documentation, package, and release gates pass.
12. No product-specific concept, hidden provider action, or validation bypass
    has entered core.

## Explicit non-goals

The golden path is not blocked on:

- ORM or Active Record;
- server-rendered templates or a core frontend framework;
- Redis, distributed cache, or distributed rate limiting;
- a feature-flag service;
- WebSockets or SSE;
- localization or search abstraction;
- GraphQL;
- multi-cloud or multi-region abstraction;
- ECS/Fargate, Kubernetes, Kinesis, Kafka, or Step Functions;
- a generic workflow engine;
- admin UI, billing, or product-specific RBAC;
- a hosted Minco control plane.

Later opt-in candidates require measured demand and at least two independent
applications or implementations. Likely first candidates are operation
throttling, a small cache contract, feature gates, outbound HTTP conventions,
and realtime transport support.

## External lessons adopted selectively

The framework borrows coherence, not APIs:

- Laravel demonstrates one predictable structure and CLI, service-provider and
  package lifecycles, migration/seed workflows, and task-oriented
  documentation. Minco retains static typed composition, explicit SQL, and no
  runtime discovery.
- Loco demonstrates discoverable Rust generators, tasks, workers, storage, and
  application conventions. Minco does not adopt its ORM, Redis assumptions, or
  frontend scope.
- Encore demonstrates one application graph driving local infrastructure,
  tracing, an API explorer, and a service catalogue. Minco keeps local services
  explicit and does not run schedules automatically.
- Pavex demonstrates validating routes, constructors, and dependency lifecycles
  before runtime. Minco continues graph-before-install validation without
  introducing a compiler/transpiler absent measured need.
- Shuttle demonstrates the value of an understandable initialise-run-deploy
  workflow. Minco remains self-hostable, AWS-account-owned, and exact-artifact
  oriented.
- Diátaxis separates tutorials, how-to guides, reference, and explanation so
  each document serves one reader need.

## Sources consulted

All external sources were checked on 2026-07-27. They are reference material,
not authority over Minco's repository decisions.

- [Laravel 13 service providers](https://laravel.com/docs/13.x/providers)
- [Laravel 13 Artisan console](https://laravel.com/docs/13.x/artisan)
- [Laravel 13 package development](https://laravel.com/docs/13.x/packages)
- [Laravel 13 migrations](https://laravel.com/docs/13.x/migrations)
- [Laravel 13 seeding](https://laravel.com/docs/13.x/seeding)
- [Loco documentation](https://loco.rs/docs/)
- [Encore local development](https://encore.dev/features/local-development)
- [Pavex constructors and lifecycles](https://pavex.dev/docs/latest/guide/dependency_injection/constructors/)
- [Shuttle command surface](https://www.shuttle.dev/)
- [AWS CloudFormation change sets](https://docs.aws.amazon.com/AWSCloudFormation/latest/UserGuide/using-cfn-updating-stacks-changesets.html)
- [AWS SAM deployment preferences](https://docs.aws.amazon.com/serverless-application-model/latest/developerguide/sam-property-function-deploymentpreference.html)
- [AWS Lambda weighted aliases](https://docs.aws.amazon.com/lambda/latest/dg/configuring-alias-routing.html)
- [Diátaxis in five minutes](https://diataxis.fr/start-here/)
- [Cargo SemVer compatibility](https://doc.rust-lang.org/cargo/reference/semver.html)
- [Cargo Rust-version support](https://doc.rust-lang.org/stable/cargo/reference/rust-version.html)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/checklist.html)
