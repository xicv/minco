# Changelog

All notable changes to Minco will be documented here. The project follows
Semantic Versioning once public releases begin.

## [Unreleased]

### Added

- Added compatibility-checked rollback assessment across exact historical
  release, deployment, environment and data boundaries, plus optional
  metric-alarm-guarded Lambda alias canaries with deterministic cleanup and
  durable receipts.
- Added checked-in generated package, facade-feature, plugin, CLI,
  configuration, Plan and diagnostic reference derived from authoritative
  Cargo metadata, validated plugin manifests, Clap help and typed read models.

### Changed

- Local and bounded hosted quality now fail when generated reference bytes
  drift from their authorities. Secret values and secret-reference names remain
  excluded, and reference generation performs no provider contact.

## [0.6.0] - 2026-08-01

### Added

- Added strict archive-visible `minco-plugin.json` distribution records for
  official plugins, adapters and runtimes. Records expose compatibility,
  capabilities, configuration, operations, database assets, resources, IAM,
  wake sources, idle-cost intent, health, sensitivity and inert conformance
  evidence without loading plugin code.
- Added `cargo minco plugin list --json` and strengthened `plugin validate` so
  package inclusion, catalog/schema safety and overlapping statically linked
  descriptor fields fail deterministically before release.
- Added the public `minco-test` plugin conformance builder and `cargo minco
  plugin test --all --json`. Official and third-party-style packages now share
  the same bounded offline contract and stable diagnostic shapes.
- Added a standalone external-style plugin fixture using versioned public
  dependencies, archive metadata and deterministic composition tests.

### Changed

- Plugin, adapter and runtime archives now carry an explicit Minco `0.6.0`
  compatibility requirement. Metadata remains descriptive: Cargo dependency
  selection and typed constructor registration are still required.
- Plugin assurance now reports package/descriptor checks, concrete lifecycle,
  application readiness, provider/live evidence and production readiness as
  separate states instead of collapsing them into one pass.

### Documentation

- Promoted the detailed framework, resource API, plugin, testing, AWS,
  zero-idle and example documentation into a versioned `0.6.0` site while
  retaining immutable `0.5.0` pages.
- Added stable installation and tutorials, a complete plugin distribution
  reference, local search, responsive browser journeys and a candidate/stable
  switch that cannot claim registry publication early.
- Added the `0.5.0` to `0.6.0` upgrade guide with exact dependency,
  distribution-record and conformance migration steps.

### Compatibility boundary

This is a pre-1.0 minor release. Plugin distribution schema 1, the
public conformance report/builder types and new CLI output are public API
additions. Existing `0.5.0` application/resource behavior remains supported,
but plugin packages should update their exact Minco dependencies and
`core_compatibility` together. Source qualification, hosted checks, merge,
tag, crates.io publication, docs.rs and live AWS proof remain separate states.
The exact 28-package family is published from immutable tag `v0.6.0`; no AWS
resource mutation was part of the publication.

## [0.5.0] - 2026-07-31

### Added

- Added the opt-in OpenAPI-first resource convention for complete create, list,
  read, update and delete families. Resource operations carry validated
  identity/action metadata; success responses use predictable data/page
  envelopes; errors remain RFC 9457 Problem Details.
- Added bounded opaque cursor pagination, deterministic sort/filter allowlists,
  strong entity tags, required `If-Match` conditional updates/deletes and
  immutable idempotent-create replay semantics.
- Added `cargo minco make resource <name>` to select an already reviewed
  five-action contract family and atomically plan failing application/HTTP
  specifications, documentation and operation traces without generating
  domain policy, persistence or fake success.
- Completed the Orders reference resource through application-owned ports,
  memory, PostgreSQL and SQLite adapters, Axum contract tests and real-service
  HTTP lifecycle coverage.
- Added typed Plan cost classes and pricing-confidence evidence for
  zero-provisioned-compute profiles, including request/storage dimensions,
  incomplete allowances and explicit one-time cleanup policy.

### Changed

- The Orders external contract now exposes all five resource actions. Create,
  read and update return `{ "data": ... }`; list returns `data` plus bounded
  cursor-page metadata; delete returns `204 No Content`.
- Orders update and delete now require `If-Match`; missing and stale
  preconditions return `428` and `412` before persistence. Create responses
  return a stable replay snapshot even after later update or deletion.
- Optional hosted CI now defaults to a small `essential` clean-Linux
  qualification, while the authoritative full matrix remains local and a
  separately dispatched `release` profile retains browser, package,
  Plan/SAM, native ARM64, Rustack and E2E evidence.

### Documentation

- Added the `0.4.0` to `0.5.0` migration guide with the resource API wire
  format, generator boundary, application-layer responsibilities, Plan/cost
  additions and local/hosted verification profiles.
- Documented the standardized resource contract, tests and generator workflow
  without introducing a generic repository, ORM, Active Record layer or hidden
  SQL.

### Compatibility boundary

This is a pre-1.0 minor release. The standardized resource response
shape, conditional-write behavior, operation family and serialized Plan cost
surface are public compatibility changes. Existing applications opt in by
declaring complete resource metadata and migrating their contract and client
handling deliberately. The 28-package family is published at `0.5.0` from
immutable tag `v0.5.0`; registry publication does not prove live AWS deployment
or production promotion.

## [0.4.0] - 2026-07-30

### Added

- Added four publishable crates: provider-neutral `minco-config`, database
  lifecycle `minco-db`, local supervisor `minco-dev`, and guarded provider
  controller `minco-deploy-aws`. The lock-step family grows from 24 to 28
  packages.
- Added strict
  application/enabled-plugin schemas, fixed environment precedence, opaque
  `env:`/`ssm:` secret references, redacted provenance and diff, deterministic
  effective digests, typed constructor deserialization, and `cargo minco
  config check|explain|diff|schema`.
- Added Plan IR schema 2 for one HTTP API function plus explicit worker
  functions, SQS queues, DLQs, event-source mappings, partial-batch behavior
  and reviewed schedules, with deterministic local-service, IAM, cost,
  performance and SAM projections.
- Added generic API-only, standard/FIFO worker, redrive, schedule and DynamoDB
  fixtures with stable validation and migration-rejection coverage.
- Added digest-bound database migration status/plan/lock/apply/verify receipts
  and classified, preservation-aware seed plans for `reference`, `demo`, `test`
  and explicitly authorised `bootstrap` data.
- Added graph-derived `cargo minco dev` dry-run and supervision, selected
  PostgreSQL/Rustack dependencies, bounded process groups and structured
  lifecycle events.
- Added contract-aware module, operation, migration, seeder, worker, adapter,
  test and plugin generators plus app-owned stubs. Plans are deterministic,
  existing/symlinked paths fail closed, and operation specifications start
  failing rather than inventing business behavior.
- Added `cargo minco contract diff` and `cargo minco upgrade report` for bounded
  structural compatibility and redacted schema/feature inventory.
- Added immutable application packaging, deployment receipts, guarded
  CloudFormation change-set/apply phases, hosted contract/readiness/
  authentication/smoke/artifact verification and exact-artifact promotion.
- Defined zero provisioned application compute, residual cost classes/pricing
  confidence, the repository-native Verified Review Loop and an explicit AWS
  service doctrine.

### Changed

- `cargo minco explain --json` now identifies an operation's deployment
  function/trigger, `cost --json` includes runtime resource dimensions, and
  `perf --json` reports every function artifact and available SHA-256.
- The release manifest is schema 3 and binds configuration, migration and seed
  digests in addition to source, OpenAPI, Plan, template, lockfile, toolchain
  and artifact identity.
- The deployment CLI now separates plan, change-set review, apply, hosted
  verify and promote behind independent receipts and exact-digest approvals.
- The M9 application-lifecycle milestone is complete in source. Rollback/
  canary, static-site domains, review-environment cleanup and later ecosystem/
  workbench programs remain deferred.

### Fixed

- Native Lambda packaging now normalizes ZIP entry timestamps, permissions and
  order after Cargo Lambda builds. Repeated builds of byte-identical Orders and
  SQS worker binaries therefore produce the same artifact digest, while
  unexpected archive entries fail before replacement.

### Documentation

- Defined Minco's five-plane contract-to-cloud product identity, developer and
  deployment golden paths, measurable 1.0 completion boundary, explicit
  non-goals, and M9-M12 framework-completion program.
- Added the target Diátaxis information architecture without moving existing
  pages ahead of deterministic link and snippet validation.
- Corrected README drift against authoritative metadata: Feedback is stable,
  and the source inventory includes the static-site plugin, AWS adapters, and
  SQS worker runtime.
- Reconciled the adoption-measurement narrative with the authoritative current
  dependency, timing, and Lambda artifact report.
- Documented the trigger-aware Plan decision, schema 2 migration, explicit SQS
  worker constraints and unchanged minimal-idle default.
- Added a dedicated `0.3.1` to `0.4.0` guide covering the four new crates,
  schemas, configuration, database/dev/generator lifecycle, CLI and deferred
  operational boundaries.

### Compatibility boundary

The public plan types, Plan schema 2 fields, typed configuration and database
schemas, package/release model and lifecycle/deployment CLI form the Minco
`0.4.0` pre-1.0 minor boundary. API-only Plan schema 1 configurations remain
supported. Source and package qualification do not prove merge, tag, crates.io
publication, live AWS deployment or production promotion.

## [0.3.1] - 2026-07-27

### Fixed

- Enforced the Feedback text-only profile when `max_attachments = 0`: the
  widget hides screenshot, file and voice controls, and the server rejects
  multipart attachment fields without changing the JSON submission contract.
- Isolated SQLx PostgreSQL and SQLite backend features so PostgreSQL-only
  Feedback and Orders consumers no longer compile `sqlx-sqlite` or
  `libsqlite3-sys`, while SQLite-only consumers no longer compile
  `sqlx-postgres`.
- Added a complete normal/build dependency-graph regression covering Feedback,
  the official SQLx extensions, the Orders adapter/service, memory/no-default
  surfaces, and the deliberate all-backend workspace build.

### Compatibility boundary

This patch preserves the public Rust API and serialized contracts of `0.3.0`.
It tightens an existing zero-attachment configuration boundary and removes
unselected database backends from Cargo dependency graphs. The lock-step
package inventory remains 24 crates.

## [0.3.0] - 2026-07-27

### Added

- Added metadata-only ownership and deterministic installation provenance for
  typed singleton services and ordered contributions, including bounded
  `cargo minco inspect --json` output and duplicate diagnostics naming the
  first and attempted owners.

### Changed

- `PluginContext` service/contribution accessors now return owner-bound
  registrar views, and `ServiceError::Duplicate` carries a structured
  first-owner/attempted-owner payload. Ordinary chained registration call sites
  remain source-compatible; explicit mutable-collection annotations must
  accept the registrar type.

### Compatibility boundary

This is a pre-1.0 minor release because the registrar return types and
`ServiceError::Duplicate` payload are public API changes. The lock-step package
inventory remains 24 crates.

## [0.2.0] - 2026-07-26

### Added

- Added a typed, deterministic HTTP header policy that merges exact
  application and installed-plugin requirements without wildcard broadening;
  Feedback-specific headers are no longer global defaults.
- Added the opt-in publishable `minco-aws-worker` crate for SQS Lambda
  partial-batch responses, bounded concurrency, FIFO fail-forward ordering,
  payload limits and optional one-pass outbox dispatch.
- Added stricter OpenAPI object, idempotency, authentication and RFC 9457 media
  policy with positive/negative fixtures, effective path/reference parameters,
  OpenAPI-correct anonymous security alternatives and explicit
  permission-scoped metadata.
- Added deterministic repository-truth validation across versions, package
  inventory, catalog/facade/descriptor metadata, generated plans and
  roadmap/task status.
- Added graph-derived local PostgreSQL/Rustack startup, standard AWS endpoint
  configuration, isolated S3/SQS/SSM/STS conformance, and safe port/database
  overrides with a pinned multi-platform Rustack 0.9.1 image. The conformance
  gate also proves the real Minco SSM SDK adapter locally and in hosted CI.

### Changed

- `HttpRuntimeConfig` now carries `HttpHeaderPolicy`, and
  `apply_standard_middleware` returns `HttpConfigurationError`.
- Plugin catalog entries now identify their `kind` and facade `feature`.
- Plan/SAM request headers are normalized and sorted deterministically.
- Generated-consumer quality checks share the repository target cache while
  retaining fresh PostgreSQL and SQLite workspaces.

## [0.1.1] - 2026-07-24

### Fixed

- Added a library documentation target to `cargo-minco` so docs.rs can build
  and publish the CLI documentation while preserving the `cargo-minco`
  executable and `cargo minco` command behavior.
- Added a docs.rs-shaped Rustdoc gate to local quality, release dry runs, and
  hosted manual CI.

## [0.1.0] - 2026-07-24

### Added

- Provider-neutral plugin kernel, typed service injection, capability graph, and
  deterministic graph validation.
- OpenAPI 3.1 validation, operation inventory, canonical digest, and checked-in
  Rust binding generation.
- Axum/Tower HTTP conventions with request IDs, exact CORS, bounded bodies,
  timeout, tracing, compression, principal extraction, and RFC 9457 errors.
- PostgreSQL and SQLite SQLx foundations with explicit migrations.
- Native ARM64 Lambda/API Gateway HTTP API adapter and JWT claim mapping.
- Deployment Plan IR, SAM renderer, structural performance rules, and cost
  profiles for Neon, self-hosted PostgreSQL, RDS, Aurora Serverless v2,
  DynamoDB on-demand, and persistent SQLite.
- Immutable release manifests.
- Orders reference vertical slice with memory, PostgreSQL, and SQLite adapters.
- JJ-first task workspaces and repository-native roadmap/task tracking.
- Local unit, feature, and e2e runners plus an opt-in manual GitHub Actions
  workflow.
- `minco update`, plugin management, deployment, cost, test, VCS, and
  inspection commands.
- Publishable `minco` facade with feature-gated contract, HTTP, plugin, SQLx,
  Lambda, planning, release, and test capabilities.
- `cargo-minco` packaging as a real Cargo subcommand (`cargo minco`).
- A real `cargo minco new` layered application generator with PostgreSQL or SQLite profiles and JJ-first initialization.
- Application-relative test, migration, operation-trace, plugin-catalog, and architecture validation commands.
- crates.io metadata, versioned path dependencies, package content allowlists,
  dual-license files, deterministic publish validation, guarded multi-package
  dry-run/upload scripts, and manual OIDC trusted-publishing workflow.
- Strengthened plugin lifecycle with core compatibility, typed multi-contributions,
  strict configuration contracts, graph-before-install validation, deterministic
  finalization, and application-provided composition dependencies.
- Official sessions, identity, object-storage, events/outbox, notifications,
  audit, static-site, and Feedback plugins.
- Feedback Web Component with configurable FAB placement, browser-authorized
  screenshots, bounded voice notes, optional transcription, threaded
  clarification, PostgreSQL/SQLite/memory storage, developer API/CLI, and
  deterministic AI handoff.

### Verification boundary

The pinned Rust compiler, Docker-backed persistence, JJ, Cargo Lambda, SAM
linting, read-only CloudFormation/IAM validation and package publication dry
runs pass. Real AWS deployment, provider-adapter conformance, crate upload and
the deferred repository-wide Codex Security Deep Scan remain outside the
verified boundary; see `VERIFICATION.md`.
