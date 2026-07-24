# Changelog

All notable changes to Minco will be documented here. The project follows
Semantic Versioning once public releases begin.

## [Unreleased]

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

### Known verification gap

The handoff environment did not provide Rust, Cargo, Docker, JJ, Cargo Lambda,
or SAM CLI. `VERIFICATION.md` records the exact checks that could and could not
run; this entry must be updated after compiler and real-runtime verification.
