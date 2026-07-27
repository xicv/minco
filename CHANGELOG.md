# Changelog

All notable changes to Minco will be documented here. The project follows
Semantic Versioning once public releases begin.

## [Unreleased]

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
