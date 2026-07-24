# Changelog

All notable changes to Minco will be documented here. The project follows
Semantic Versioning once public releases begin.

## [Unreleased]

### Added

- Added graph-derived local PostgreSQL/Rustack startup, standard AWS endpoint
  configuration, isolated S3/SQS/SSM/STS conformance, and safe port/database
  overrides with a pinned multi-platform Rustack 0.9.1 image. The conformance
  gate also proves the real Minco SSM SDK adapter locally and in hosted CI.

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
