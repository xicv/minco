# Minimal-cost framework review: post-1.0 direction

Reviewed: 2026-08-07 (Australia/Adelaide)
Published baseline: `1.1.0`
Reviewed release source: `4d81543f7c5adb773655f23278abfe084de9f3e0`

## Executive conclusion

Minco should not compete with Laravel by becoming a second broad, general-purpose
web framework. Its defensible position is narrower and deeper:

> Minco is an AWS-native Rust application delivery system whose default path has
> zero provisioned compute at idle, explicit residual cost, contract-derived
> behavior, fast reversible deployment, and evidence strong enough for humans and
> coding agents to change the application safely.

That statement is more precise than “zero cost”. Lambda, API Gateway, DynamoDB,
S3, CloudFront, logs, retained database storage, DNS and secrets can still incur
request, storage or fixed control-plane charges. The invariant Minco can enforce
is **no continuously provisioned application compute in the default profile**,
with every remaining cost and wake source exposed before deployment.

The current project is unusually strong for its maturity. It already has a small
core, contract ownership, deterministic Plan IR, cost and performance policies,
AWS rendering, guarded deployment receipts, candidate verification and
promotion, plugins, migrations and seeds, preview lifecycle, feedback capture,
release qualification, a project view, local MCP/workbench surfaces, and
version-matched agent skills. The next phase should therefore be about
**assurance, adoption and feedback throughput**, not a large expansion of the
core API.

The most important concrete gap found in this review is a support-truth gap:
`IngressPlan` declares `LambdaFunctionUrl`, while the SAM renderer explicitly
accepts only `ApiGatewayHttpApi`, and the runtime cost estimator currently names
API Gateway pricing independently of ingress. A public enum variant must not be
mistaken for a supported deployment profile. This review introduces a
machine-checked deployment assurance ledger so future support promotions require
contract, code, cost, security, performance, recovery and provider evidence.
It deliberately records Lambda Function URLs as **declared, not supported**.

## Core product doctrine

### 1. Optimise for feedback latency, not only deployment latency

The commercial advantage is not merely that a stack can be deployed cheaply.
It is that a client can see a real, versioned application early, report evidence
against that exact version, and receive a verified correction quickly.

The ideal Minco loop is:

1. capture an outcome and acceptance rule;
2. bind it to an OpenAPI operation, task or explicit infrastructure capability;
3. generate only the safe skeletons that preserve those contracts;
4. run deterministic local gates;
5. create a short-lived, zero-provisioned-compute review environment;
6. bind feedback to release digest, route, user context and non-secret telemetry;
7. turn accepted feedback into a task with reproducible evidence;
8. verify the candidate, promote one immutable artifact, and retain rollback
   evidence;
9. capture what the team learned as a test, diagnostic, skill or decision.

This is the product flywheel. Deployment is one stage of it, not the product by
itself.

### 2. Make the cheap path the paved path

The default must remain opinionated:

- native ARM64 Lambda zip artifacts;
- API Gateway HTTP API for the generic authenticated web API;
- no NAT Gateway;
- no provisioned concurrency;
- no always-on worker or queue supervisor;
- no scheduled polling disguised as “serverless”;
- bounded database connections and concurrency;
- request-driven or explicit event-driven work;
- S3 and CloudFront for static assets;
- DynamoDB on-demand or a scale-to-zero relational option where the domain
  actually requires it;
- finite log retention and explicit retained resources.

More expensive profiles may exist, but they should be opt-in capabilities with
an exact reason, cost class, migration path and recovery plan. Minco should not
silently “help” an application into fixed monthly infrastructure.

### 3. Treat every feature claim as an assurance case

A feature is not complete because a type, CLI flag or CloudFormation fragment
exists. For an AWS-facing feature, Minco should require seven dimensions:

- **contract** — user-visible behavior and compatibility boundary;
- **code** — implementation and architecture ownership;
- **cost** — residual idle classes, wake sources, regional rate confidence;
- **security** — identity, least privilege, secret and data boundaries;
- **performance** — measurable budget and representative load evidence;
- **recovery** — retry, rollback, deletion and partial-failure behavior;
- **provider** — rendered-resource and, where necessary, hosted AWS evidence.

The deployment assurance ledger added with this review applies that model to
runtime/ingress profiles. It is intentionally small. It does not duplicate the
roadmap or task graph; it guards what the product is allowed to call supported.

## Current project assessment

| Area | Assessment | Important remaining boundary |
|---|---|---|
| Core architecture | Strong. The domain/application/provider separation and static plugin composition protect binary size, startup behavior and agent comprehension. | Keep public surface growth evidence-driven; resist runtime reflection and service-location patterns. |
| Contract ownership | Strong. OpenAPI drives operation identity and generated bindings, with compatibility checks and idempotency/auth policy. | Add requirement-to-operation acceptance trace so client outcomes are as inspectable as transport contracts. |
| AWS deployment | Strong default profile. Plan IR, SAM rendering, packaging, change sets, receipts, verification, promotion and rollback boundaries are unusually explicit. | Close support-truth gaps before adding another ingress; repeat hosted proof across more real applications and regions. |
| Cost control | Strong doctrine and static policy: fixed compute, NAT, provisioned concurrency and scheduled wakeups can be rejected. | Add portfolio-level budget history and alert on cost-model regressions, not only per-plan estimates. |
| Performance | Good static budgets and candidate load tooling. Artifact size, concurrency and connection pressure are visible. | Establish versioned workload baselines, p50/p95/p99 and cold/warm dimensions per golden application. |
| Reliability | Strong receipts, exact artifact identity, candidate verification and recovery qualification. | Add more provider fault injection and prove idempotent recovery for every new AWS mutation surface. |
| Security | Strong fail-closed configuration, secret redaction, least-privilege intent and supply-chain gates. | Keep cloud-facing agent tools schema-defined and audited; do not expose raw shell or broad AWS credentials as an “AI feature”. |
| Plugins | Strong static package model, catalog, conformance and lifecycle boundaries. | Add capability-specific assurance examples and semver compatibility evidence for third-party plugin authors. |
| Feedback | Strategically important and already represented as a plugin and review loop. | Bind every submission to immutable release/deployment identity and provide deterministic feedback-to-task receipts. |
| AI-first workflow | Strong 1.1 foundation: agent projections, bounded context/eval, ProjectView, MCP and local workbench. | Measure task outcomes and manual review, add application-specific evals, and turn recurring failures into maintained skills. |
| Documentation and progress | Rich ADRs, tasks, roadmap, release and verification records. | Prevent current-state headings from drifting while retaining historical evidence; reduce duplicated narrative state. |

## AWS service choices

### Keep API Gateway HTTP API as the generic web default

Lambda Function URLs have no separate endpoint charge, so they are attractive
for very small internal endpoints. They are not a drop-in replacement for the
current Minco default. Function URL authentication is `AWS_IAM` or `NONE`, while
Minco's generic web profile relies on API Gateway JWT authorization, explicit
CORS, candidate-stage routing and guarded alias promotion. CloudFront Origin
Access Control can protect a Function URL, but signed `POST` and `PUT` requests
require the client to provide a SHA-256 payload hash; new Function URLs also need
both `lambda:InvokeFunctionUrl` and `lambda:InvokeFunction` permissions. Those
constraints are acceptable for a deliberately signed machine-client profile,
not as invisible behavior for arbitrary browsers and mobile applications.

Decision:

- keep `ApiGatewayHttpApi + LambdaZipArm64` stable and default;
- retain `LambdaFunctionUrl` as declared only;
- promote it later only as a complete profile with renderer, auth model, cost
  projection, candidate verification, promotion/rollback and hosted tests;
- do not select ingress solely by its endpoint line item.

Primary AWS references:

- [Lambda Function URL pricing and comparison](https://docs.aws.amazon.com/lambda/latest/dg/urls-configuration.html)
- [CloudFront OAC with Lambda Function URLs](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/private-content-restricting-access-to-lambda.html)
- [Function URL access control](https://docs.aws.amazon.com/lambda/latest/dg/urls-auth.html)

### Database profiles should remain explicit domain choices

There is no single “serverless database” abstraction that preserves semantics,
cost and operability.

**DynamoDB on-demand** is the best AWS-native default when access patterns are
known and fit key-value/document modeling. It has no provisioned instance, can
scale on demand, and integrates cleanly with Lambda and least-privilege IAM. It
must remain an explicit adapter; Minco should not imitate a relational ORM over
DynamoDB.

**Scale-to-zero PostgreSQL**, including Neon, remains practical for conventional
relational applications and migrations. It is not fully AWS-native, so provider
ownership, network path and external service failure remain explicit.

**Aurora Serverless v2 with minimum capacity 0 ACU** is the most relevant
AWS-native relational option. Paused capacity has no instance-capacity charge,
but storage and other retained resources remain billable. Open connections can
prevent automatic pause, and RDS Proxy keeps connections open and therefore
prevents pause. This means Minco's existing connection-pressure doctrine is not
only a performance concern; it is a cost invariant. Aurora v2 should be an
opt-in profile with a hosted pause/wake proof rather than a new default.

**RDS Data API** can remove persistent client connections for bounded operations,
but result and row limits and transaction duration make it unsuitable as an
invisible universal SQL transport. Use it only behind a specific adapter and
measured workload contract.

**Aurora DSQL** is worth tracking, especially for highly available distributed
applications. It speaks the PostgreSQL wire protocol but implements a subset of
PostgreSQL behavior and has a different transaction and pricing model. Treat it
as a separate database capability, not “another Postgres URL”.

Primary AWS references:

- [Aurora Serverless v2 scaling to zero](https://docs.aws.amazon.com/AmazonRDS/latest/AuroraUserGuide/aurora-serverless-v2-auto-pause.html)
- [Aurora Serverless v2 capacity](https://docs.aws.amazon.com/AmazonRDS/latest/AuroraUserGuide/aurora-serverless-v2.setting-capacity.html)
- [RDS Data API limitations](https://docs.aws.amazon.com/AmazonRDS/latest/AuroraUserGuide/data-api.limitations.html)
- [Aurora DSQL PostgreSQL compatibility](https://docs.aws.amazon.com/aurora-dsql/latest/userguide/working-with-postgresql-compatibility.html)
- [DynamoDB on-demand capacity](https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/on-demand-capacity-mode.html)

### Event-driven work should expose every wake source

SQS-triggered Lambda is aligned with Minco when batch size, partial batch
failure, dead-letter behavior, reserved concurrency and database connection
pressure are all in Plan IR. EventBridge Scheduler is appropriate for explicit
one-time or recurring work; one-time schedules should use
`ActionAfterCompletion=DELETE` where the provider path supports it, and recurring
schedules need an explicit end or retention decision. A schedule is never a
free background loop: it is a declared wake source and cost class.

Primary AWS references:

- [Lambda with SQS](https://docs.aws.amazon.com/lambda/latest/dg/with-sqs.html)
- [EventBridge Scheduler automatic deletion](https://docs.aws.amazon.com/scheduler/latest/UserGuide/managing-schedule-delete.html)

### Configuration should stay cheap by default

SSM Parameter Store remains a good deployment-time secret/reference boundary.
AWS AppConfig is useful for dynamic configuration, validation and controlled
rollouts; its Lambda extension caches configuration locally, but it still adds
polling, request and operational behavior. AppConfig should therefore be an
optional feature-management plugin, not a mandatory core dependency. A feature
flag must declare owner, expiry, fallback, evaluation evidence and cleanup task;
otherwise flags become a permanent second configuration system.

Primary AWS references:

- [AWS AppConfig Lambda extension](https://docs.aws.amazon.com/appconfig/latest/userguide/appconfig-integration-lambda-extensions.html)
- [AWS AppConfig pricing](https://aws.amazon.com/systems-manager/pricing/)

### Observability must not require an always-on collector

Structured CloudWatch-compatible JSON logs, correlation IDs, bounded retention
and explicit metrics are correct defaults. The next improvement should be
application-level service-level indicators and release-bound feedback telemetry,
not a mandatory always-running observability stack. Embedded Metric Format or
bounded OpenTelemetry export can be optional; a zero-idle application must not
silently acquire an always-on telemetry bill.

## What to learn from Laravel

Laravel's strength is not any single technical mechanism. It offers a coherent
paved road: naming, documentation, generators, validation, testing, migrations,
queues, events, packages and operational tools fit together. Minco should adopt
that coherence without copying Laravel's runtime architecture.

### Adopt

**Task-oriented CLI and documentation.** A developer should be able to find one
canonical path for “add an operation”, “add a migration”, “queue work”, “deploy”,
“verify”, “roll back” and “hand over”. Minco already has much of this; the next
step is reducing duplicated state and making diagnostics directly actionable.

**Request/operation policy objects.** Laravel Form Requests combine validation
and authorization close to a use case. Minco's equivalent should remain
contract-first: an operation policy can project OpenAPI schema, auth permissions,
idempotency, input size and application handler ownership into one inspectable
view, without coupling the domain to HTTP.

**Feature tests as the confidence centre.** Laravel explicitly gives feature
tests the most confidence. Minco should keep unit tests fast but optimise the
paved road around operation-level feature tests, generated application tests,
hosted smoke tests and recovery tests.

**First-class fakes.** Laravel's event, queue, mail, notification and storage
fakes make side effects observable. Minco should standardise small trait-backed
recording adapters for its official plugins and generated tests. They should
record typed intents, not reproduce AWS SDK clients.

**Factories, migrations and seed classes.** Minco already has migration and seed
boundaries. The missing Laravel-like advantage is ergonomic, deterministic test
data factories that produce valid domain data while keeping demo/test data out
of production.

**Package lifecycle discipline.** Laravel service providers separate registration
from bootstrapping. Minco should preserve its safer compile-time plugin graph but
can keep explicit phases: descriptor/registration, configuration validation,
application composition and runtime start. The phases must be inspectable and
must not become a dynamic service locator.

**Upgrade guides and error quality.** Laravel's ecosystem succeeds because common
work has examples and failures are understandable. Every stable Minco diagnostic
should state what invariant failed, which evidence is missing, and the smallest
safe next command.

Primary Laravel references:

- [Service providers](https://laravel.com/docs/13.x/providers)
- [Package development](https://laravel.com/docs/13.x/packages)
- [HTTP tests](https://laravel.com/docs/13.x/http-tests)
- [Mocking and fakes](https://laravel.com/docs/13.x/mocking)
- [Validation and Form Requests](https://laravel.com/docs/13.x/validation#form-request-validation)
- [Database testing and factories](https://laravel.com/docs/13.x/database-testing)

### Do not copy into the default core

- runtime reflection or an unbounded service container;
- Active Record as the domain model;
- implicit global state and provider discovery;
- Redis-required queue dashboards or always-on supervisors;
- a broad facade for every AWS service;
- convenience that hides IAM, retention, wake sources or cost;
- framework magic that an agent or reviewer cannot project into a bounded graph.

Laravel Horizon and Pulse are valuable in their intended environments, but a
Redis-backed always-on control plane conflicts with Minco's default economics.
The lesson is their operational visibility, not their infrastructure shape.

## Harness engineering and AI-first development

The Qoder Better Harness model describes a coding-agent environment as
feed-forward guidance plus feedback sensors across task understanding,
controlled execution, validation, delivery and learning capture. That framing
matches Minco well. The project already has strong deterministic sensors; the
next improvement is to bind them to feature claims and outcomes rather than
adding more disconnected checks.

The Thoughtworks *Future of Software Engineering 2026* report argues that
verification, not generation, becomes the bottleneck as AI produces more code.
It recommends cheap, fast and human-legible verification; characterization and
constraint tests; mutation testing; production back-testing; deterministic and
non-deterministic evaluation; narrow audited cloud interfaces; and measuring
manual review effort. Minco should treat “harness engineering” as a maintained
product capability.

### The Minco trust stack

1. **Static constraints:** formatting check, lint, dependency and architecture
   policy, generated-source and repository-truth checks.
2. **Contract tests:** OpenAPI compatibility, operation ownership, schemas,
   idempotency and auth policy.
3. **Behavior tests:** domain/unit tests, operation feature tests and side-effect
   recording fakes.
4. **Artifact tests:** reproducible build, size, SBOM/supply chain and exact hash.
5. **Provider tests:** rendered-resource assertions and bounded hosted smoke.
6. **Resilience tests:** retries, partial failures, migration recovery, rollback
   and cleanup.
7. **Outcome tests:** client acceptance, feedback closure and application-specific
   agent evals.

Missing evidence must remain explicit. A local deterministic pass is not hosted
AWS proof; a hosted smoke is not a production SLO; a clean structural contract
diff is not semantic compatibility.

### Risk-tiered autonomy

- **Tier 0 — inspect:** read-only project view, explanations and deterministic
  plans may run automatically.
- **Tier 1 — local writes:** generated, project-contained changes require an
  exact plan digest and local verification.
- **Tier 2 — preview:** ephemeral AWS mutation requires account/region guards,
  TTL/cleanup, budget and receipt.
- **Tier 3 — production candidate:** create but do not execute a reviewed change
  set; bind migrations and artifacts.
- **Tier 4 — production mutation:** require explicit digest approval, hosted
  verification and recoverable evidence.

Agents should receive narrow schema-defined tools at each tier. Raw AWS CLI or a
broad shell is not an adequate product interface simply because the model can
use it.

Primary harness references:

- [QoderAI Better Harness](https://github.com/QoderAI/better-harness)
- [Thoughtworks Future of Software Engineering 2026](https://www.thoughtworks.com/content/dam/thoughtworks/documents/report/tw_future_of_software_engineering_europe_2026.pdf)

## High-value capabilities enabled by the core doctrine

### 1. Release-bound client feedback

Every feedback record should include a non-secret immutable release digest,
deployment receipt identifier, environment, route/operation, UI build identity,
timestamp and optional screenshot/audio provenance. The CLI should be able to
produce a deterministic `feedback -> task` plan and receipt. This turns rapid
deployment into auditable requirement discovery rather than an unstructured
comment stream.

### 2. Acceptance packets for handover

A handover command can assemble, without secrets:

- application and release identity;
- contract and supported operations;
- architecture/cost projection;
- hosted verification and rollback status;
- migrations and seed classes;
- open client feedback and accepted requirements;
- operating commands, ownership and known residual resources.

The packet should link to evidence rather than duplicate it. This is a direct
commercial benefit of the existing graph and receipt model.

### 3. Zero-provisioned-compute certification

`cargo minco inspect` or a future `assure` command can emit a signed or
hash-bound certification result:

- no fixed application compute;
- no NAT Gateway;
- no provisioned concurrency;
- no undeclared schedule/poller;
- explicit retained storage and fixed control-plane resources;
- complete regional pricing confidence or an explicit unknown;
- bounded concurrency and database connection pressure.

The label should never say “free”. It should state the exact assurance level and
unknowns.

### 4. Golden application matrix

Use a small set of real generated/adopted applications instead of adding more
framework micro-fixtures:

- public read-mostly site;
- authenticated CRUD application with relational data;
- DynamoDB access-pattern application;
- SQS worker and dead-letter recovery;
- realtime application;
- static frontend plus API;
- preview/feedback/handover lifecycle.

Each app should carry contract, cost, artifact-size, cold/warm performance,
recovery and hosted evidence. This gives new features a meaningful regression
surface and prevents tests from overfitting the framework repository.

### 5. Application-specific agent evals

The packaged skills prove format and boundary behavior, not whether an agent
successfully changes a real application. Add versioned scenarios derived from
actual defects and feedback. Measure completion, escaped defects, review edits,
commands/time spent and evidence quality. A recurring failure should produce a
maintained diagnostic, test, example or skill—not merely a longer prompt.

## Quality and performance control for every new feature

A new stable AWS feature should not merge until its task answers all of the
following:

1. What user outcome does it enable, and why does it belong in Minco rather than
   an application plugin?
2. Which public contract, serialized schema or CLI behavior changes?
3. What is its support status and compatibility policy?
4. What resources, IAM actions, wake sources and residual cost classes exist?
5. Can it introduce fixed compute, NAT, polling or unbounded concurrency?
6. What are the artifact, cold-start, warm latency, throughput, memory, payload
   and connection budgets?
7. How are retries, duplicates, partial failure, rollback, deletion and retained
   data handled?
8. Which local deterministic tests prove behavior?
9. Which provider assertions or hosted AWS tests are necessary, and how fresh is
   that evidence?
10. What migration, adoption, documentation and handover material is required?
11. How does client feedback identify the exact deployed version?
12. What will force a future support-status review if implementation changes?

The assurance ledger introduced here answers the last question for deployment
profiles: enum coverage, support status, evidence dimensions, provider renderer
markers and default-cost invariants are checked together.

## Prioritised roadmap

### P0 — implemented by this review

- add a machine-readable deployment profile assurance ledger;
- require every `RuntimePlan` and `IngressPlan` variant to have an explicit
  support claim;
- require stable AWS profiles to carry contract, code, cost, security,
  performance, recovery and provider evidence;
- prevent Lambda Function URL support from being claimed before renderer support,
  and force review if implementation appears while status remains declared;
- strengthen current release truth with an exact published commit and guarded
  maturity/status markers;
- record this whole-project review and its decisions.

### P1 — next safe implementation slices

1. **Ingress truth in Plan IR and cost reporting.** Reject unsupported
   runtime/ingress combinations during plan validation, not only SAM rendering;
   make cost evidence depend on ingress. Add a schema/compatibility assessment
   before changing public diagnostics.
2. **Feedback-to-task receipts.** Bind feedback to release/deployment identity,
   create a deterministic task plan, and prove deduplication and redaction.
3. **Performance baselines.** Store immutable baseline/candidate workload
   measurements for golden applications, including cold/warm and p95/p99 budgets.
4. **Provider evidence freshness.** Give hosted proof an explicit scope, Region,
   exact source and freshness policy; never silently promote stale evidence.

### P2 — controlled pilots

- add `cargo-semver-checks` to release qualification after measuring false
  positives and public API policy;
- add `cargo-llvm-cov` thresholds for selected crates and behavior-critical
  branches, not a vanity workspace percentage;
- pilot mutation testing on cost, policy, auth and deployment decision code;
- provide official typed fakes for queues, events, storage, feedback and mail-like
  side effects;
- add application-specific agent evals and report human review effort.

Primary Rust tool references:

- [cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks)
- [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov)
- [cargo-nextest](https://nexte.st/)
- [cargo-mutants](https://github.com/sourcefrog/cargo-mutants)

### P3 — only with a proven application demand

- a bounded Lambda Function URL profile for signed machine clients;
- Aurora Serverless v2 zero-ACU hosted pause/wake and migration proof;
- an optional AppConfig feature-management plugin;
- an Aurora DSQL adapter research spike with explicit PostgreSQL incompatibility
  tests;
- richer CloudWatch/EMF or OpenTelemetry export without an always-on default.

## Non-goals

This review does not recommend:

- implementing every AWS service;
- making Lambda Function URLs the new default;
- adding a dynamic dependency-injection container;
- adding an ORM to the core;
- requiring Redis, Kubernetes, ECS or a hosted Minco control plane;
- enabling automatic production mutation by coding agents;
- treating generated code volume, test count or nominal coverage as product
  quality;
- claiming that zero provisioned compute means a zero AWS bill.

## Decision summary

Minco's next competitive step is not breadth. It is to make the narrow AWS path
more trustworthy and commercially complete: outcome traceability, rapid preview,
release-bound feedback, deterministic task conversion, measured performance,
provider evidence, safe promotion and a concise handover packet. The framework
already contains most of the technical pieces. The work now is to make their
relationships executable, prevent unsupported claims, and prove the loop on
real applications.
