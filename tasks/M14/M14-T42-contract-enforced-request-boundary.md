---
id: M14-T42
title: Add a contract-enforced request boundary
milestone: M14
status: complete
priority: high
area: contract/http/identity/orders
depends_on: [M14-T40]
operations:
  - placeOrder
  - updateOrder
owned_paths:
  - Cargo.lock
  - Cargo.toml
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - VERIFICATION.md
  - crates/minco/Cargo.toml
  - crates/minco/src/lib.rs
  - crates/minco-contract/**
  - crates/minco-http/**
  - plugins/minco-plugin-identity/**
  - examples/orders/openapi/openapi.yaml
  - examples/orders/api/Cargo.toml
  - examples/orders/api/src/generated.rs
  - examples/orders/api/src/lib.rs
  - examples/orders/api/tests/**
  - scripts/test/publish_validation.py
  - scripts/test/e2e.sh
  - docs/DECISIONS.md
  - docs/adrs/0047-contract-derived-request-boundary.md
  - docs/how-to/contract-request-validation.md
  - docs/reference/generated/diagnostics.md
  - docs/reference/http-request-boundary.md
  - docs-site/next/guides/contract-request-validation.md
  - docs-site/next/reference/http-request-boundary.md
  - docs-site/.vitepress/config.mts
  - verification/static-validation.json
  - verification/1.9-performance-baseline.json
  - verification/deep-review.json
  - verification/operational-evidence-validation.json
  - verification/release-identity.json
  - verification/source-manifest.json
  - tasks/M14/M14-T42-contract-enforced-request-boundary.md
checks:
  - cargo minco contract check
  - cargo minco contract sync --check
  - cargo +1.97.1 check -p minco-contract -p minco-http -p minco-plugin-identity -p orders-api --all-targets --all-features --locked
  - cargo +1.97.1 test -p minco-contract -p minco-http -p minco-plugin-identity -p orders-api --all-targets --all-features --locked
  - cargo +1.97.1 clippy -p minco-contract -p minco-http -p minco-plugin-identity -p orders-api --all-targets --all-features --locked -- -D warnings
  - cargo semver-checks -p minco-contract --baseline-version 1.10.0
  - cargo semver-checks -p minco-http --baseline-version 1.10.0
  - cargo semver-checks -p minco-plugin-identity --baseline-version 1.10.0
  - cargo semver-checks -p minco --baseline-version 1.10.0
  - scripts/docs/check-snippets.sh
  - scripts/docs/check-links.sh
  - ./scripts/quality.sh
  - scripts/ci/local-release.sh
---

# M14-T42 - Add a contract-enforced request boundary

## Goal and user value

Make the reviewed OpenAPI description the executable authority for untrusted
request shape, semantic validation and coarse operation access. Applications
receive one typed value, stable bounded Problem Details, a safe correlation ID
and an already-proven coarse principal policy while retaining application-owned
business invariants, resource ownership and persistence authorization.

## Current gap

Generated request DTOs currently preserve structural Serde shape but do not
enforce most request-reachable JSON Schema assertions. HTTP applications must
hand-code semantic checks and rejection mapping; identity scopes are not
available through the provider-neutral principal boundary; client request IDs
can reach tracing without a bounded grammar; and the standard body-limit and
timeout middleware return provider-owned responses that cannot be identified
reliably from status and content type.

## Compatibility boundary

Minco 1.10.0 is published. This task is additive: it does not change the public
fields or existing constructors of `ContractOperation`, `OwnedOperation`,
`Principal`, `ApiFailure` or `ProblemDetails`. Generated request validation is
enabled only by the single `x-minco-request-validation: generated` profile.
Contracts without the profile retain their existing public DTO shape and
behavior. Authorization metadata is generated separately from the frozen
operation type, and scopes use an additive reserved principal-claim boundary.

## Architecture and performance boundary

- `minco-contract` owns a small static `ContractValidate` runtime, deterministic
  bounded errors and request-reachable schema analysis. It has no Axum, async,
  database, network, reflection, regex engine or runtime rule registry.
- Generated direct Rust checks cover the supported assertion subset; an
  unsupported request-reachable assertion fails contract validation with a
  stable diagnostic. Response-only schemas do not opt requests into the subset.
- `minco-http` adapts native Axum JSON/query/path extraction exactly once and
  maps only public-safe rejection classes. Valid requests use static dispatch
  and do not build an intermediate JSON tree or materialize field paths.
- A Minco-owned streamed request-body limit and typed timeout provenance return
  Problem Details without inspecting application response bodies. Axum's
  independent default limit is disabled under the standard runtime policy.
- The change adds no AWS resource, wake source, schedule, fixed compute,
  hosted service, cache, worker or database call. API Gateway HTTP APIs do not
  support request validation, so application/runtime enforcement is required.

## Security boundary

- Validation traversal, field paths, messages and total output are bounded;
  messages never include request values, parser internals, tokens or headers.
- Unsupported opted-in assertions, external references, unsafe recursion and
  excessive schema complexity fail closed before generation.
- Request IDs use a bounded ASCII correlation grammar and are replaced with a
  UUIDv7 before tracing or reflection when missing or invalid. Problem rendering
  validates again so application-created failures cannot reflect unsafe input.
- Generated policy enforces authentication, exact permissions and exact scopes
  before a use case is called. Database, tenancy, ownership and business-state
  authorization remain in the application layer as defence in depth.
- The configured body limit rejects excessive declared length early and also
  limits streamed bodies. Timeout/body failures have explicit Minco provenance;
  application-owned 408 and 413 responses remain unchanged.
- Existing exact CORS, sensitive-header and compression behavior remains in
  force. Direct object bytes remain outside the Lambda JSON request path.

## Supported generated request subset

The opt-in profile supports required/closed object shape, optional non-null
properties, `minLength`, `maxLength`, `minItems`, `maxItems`, `minProperties`,
`maxProperties`, inclusive and exclusive numeric bounds, scalar `enum`, scalar
`const`, nested local references and nested arrays. String length counts Unicode
code points. `format` remains an annotation except where an existing generated
typed parser deliberately asserts UUID or RFC 3339 date-time semantics.

Conditional/composition keywords, external references, unsupported vocabularies
and unbounded recursive graphs are not approximated. If request-reachable under
the generated profile, they produce stable `MINCO-CONTRACT-*` diagnostics.
Malformed parameter/request-body objects and enum schemas with assertions that
the generated enum cannot retain fail closed before source generation.

## Non-goals

- runtime validation DSLs, reflection, service locators or dynamic plugins;
- database uniqueness, resource ownership, tenancy or business validation in
  extractors;
- generic query/repository frameworks or replacement of Orders resource-query
  parsing;
- request decompression, file relay through Lambda, new AWS topology or hosted
  gateway validation;
- breaking public/serialized contracts or changing the 1.10.0 manual;
- workflow edits or dispatch, merge, deployment, publication, tag, release,
  provider mutation or production access; and
- inferring framework provenance from status codes, content type or response
  body heuristics.

## Acceptance

- [x] Current repository, remote heads, PR #170 patch/discussion, workflow
  triggers, authorities, task/ADR IDs and pinned dependency versions inspected.
- [x] OpenAPI 3.1.1, JSON Schema 2020-12, Axum 0.8.9, Tower 0.5.3,
  tower-http 0.7.0, Laravel 13, API Gateway HTTP API and Rust validation patterns
  researched from primary documentation or exact pinned source.
- [x] Provider-neutral validation errors are deterministic, bounded and cheap
  when empty; supported request assertions and Unicode semantics are proven.
- [x] Request-reachable traversal rejects unsupported assertions, malformed
  bounds, invalid/external refs, cycles and excess complexity deterministically.
- [x] Deterministic generation implements validation and missing-versus-null
  semantics without changing non-opted DTO public shapes.
- [x] `ValidatedJson`, `ValidatedQuery` and `ValidatedPath` use native extraction
  once and return the documented 400/413/415/422 taxonomy without raw errors.
- [x] Separate generated authorization preserves public structs, exact AND/OR
  semantics and application authorization; denied requests call no use case or
  persistence port. Identity scopes reach the additive principal boundary.
- [x] Unsafe request IDs are replaced before tracing and Problem rendering, and
  body/header correlation is exact.
- [x] Minco-owned timeout and streamed body-limit provenance returns 408/413
  Problem Details while preserving application responses and all existing CORS,
  sensitive-header and compression behavior.
- [x] Orders OpenAPI, generated code and in-process Axum tests prove the complete
  place/update vertical slice; generated source is never hand-edited.
- [x] Public compatibility fixtures, facade feature checks and published 1.10.0
  SemVer checks pass.
- [x] Focused checks, exact-file rustfmt checks, documentation checks,
  `scripts/quality.sh` and `scripts/ci/local-release.sh` pass on the final tree.
- [x] Three independent read-only reviews cover contract/schema correctness,
  HTTP/security/performance and public/repository compatibility; every valid
  finding is fixed and requalified.
- [x] PR #170 is replaced only with exact force-with-lease after confirming its
  remote head remains unchanged; no GitHub Action is triggered and no merge,
  deployment, publication or provider operation occurs.

## Research decisions (2026-08-21)

- OpenAPI 3.1.1 (2024-10-24) composes schemes within one Security Requirement
  as AND, entries in the security array as OR, and `{}` as anonymous. Generated
  policy retains that structure instead of flattening tokens.
- JSON Schema 2020-12 defines the listed validation keywords as assertions but
  separates `format` annotation from assertion. Generated code asserts only the
  deliberate typed formats already selected by Minco.
- Axum 0.8.9 applies a 2 MiB extractor-local default limit; Minco disables it
  only where its own configured global streamed limit is installed.
- Pinned tower-http 0.7.0 returns a private text/plain 413 for oversized declared
  length and an empty configured-status timeout response. Status/content-type
  rewriting cannot prove provenance, so the standard boundary is Minco-owned.
- Laravel 13 demonstrates typed request validation, dot-indexed public errors,
  validated input and authorization before controller execution. Minco adopts
  those ergonomics without its runtime DSL, facades, container or Active Record.
- API Gateway HTTP APIs explicitly do not support request validation during
  OpenAPI import; no managed gateway setting can substitute for this boundary.
- `garde` and `validator` provide useful derive/error-shape patterns, but a new
  default runtime/derive dependency would duplicate Minco's already-required
  deterministic OpenAPI generator. Direct generated checks are smaller and
  preserve source authority.

## Evidence

### Source and authority

- Work was rebuilt in the isolated `minco-task-m14-t42` JJ workspace from
  `main@origin` `b822ca40e72bcfd3967e3dbd9f757ff100775c2`; the original PR head was
  `a077dbc0c5abee54868f66ab41e028cce3d22cfd` and the old merge base was
  `ed9e8fc1c8e6a451603c7d8f16aeab7b257e666a`.
- Task ownership, dependency M14-T40, ADR 0047, the PR patch/discussion and the
  exact GitHub workflow trigger allowlist were checked before implementation.
- The primary repository's unrelated `.mimosa` state was not modified. No AWS,
  GitHub Actions, deployment, publication, release, provider or production
  operation was performed by these local gates.

### Focused and compatibility qualification

- Exact-file `rustfmt --check` passed for every changed Rust source file.
- `cargo test -p minco-contract -p minco-http -p minco-plugin-identity -p orders-api`
  passed 135 tests in 15.07 seconds; the matching `cargo check` passed in 2.68
  seconds and Clippy with `--all-targets -- -D warnings` passed in 1.90 seconds.
- `cargo minco contract check` passed with no findings in 2.19 seconds.
  `cargo minco contract sync --check` passed in 2.23 seconds with contract digest
  `231a11a35589d84cb3047e7e40c182f57fef1e1726c8a97eb04f266294dd09a8`.
- Published-1.10.0 SemVer checks for `minco-contract`, `minco-http`,
  `minco-plugin-identity` and `minco` each passed 223 checks with 31 skips and no
  required version update (15.05, 15.55, 15.69 and 17.43 seconds respectively).
- Documentation snippet checks passed 360 snippets; link checks passed 2,400
  internal, 13 external and 573 canonical links; the VitePress build passed.

### Full local qualification

- `/usr/bin/time -p ./scripts/quality.sh` passed with exit 0 in 620.21 seconds.
  This included full workspace compiler/test/documentation/browser/audit and
  secret-scanning gates. Cargo audit reported zero vulnerabilities; the
  explicitly allowed `lru 0.16.4` RUSTSEC-2026-0253 warning remains policy
  visible.
- `/usr/bin/time -p env MINCO_QUALITY_TOOL_ROOT=/Users/xicao/.cargo
  MINCO_LOCAL_CI_POSTGRES_PORT=55433 scripts/ci/local-release.sh` passed with
  exit 0 in 1,819.31 seconds. It covered the embedded full quality gate,
  nextest/doctest parity, coverage and mutation policies, SemVer checks, local
  AppSync/recovery/load gates, dry-run-only crate packaging, packaged-archive
  tests, external consumers, CLI installation, Plan/SAM rendering, Lambda
  artifact builds, disposable PostgreSQL/Rustack checks and Orders E2E.
- Candidate load evidence for the qualified tree passed 80 loopback API
  requests with zero failures (p95 3.643 ms, p99 43.692 ms) and 1,000 synthetic
  worker messages with zero failures. These are machine-local warm diagnostics,
  not hosted Linux, AWS, production or SLO evidence. Candidate recovery passed
  using temporary synthetic SQLite only.
- Operational evidence validation passed with two explicit limitations: no
  current exact-source live-provider evidence and no hosted-Linux performance
  run. Real AWS tests remained skipped and no provider contact occurred.
- After the evidence ledger was added, the exact pre-transport tree passed
  `./scripts/quality.sh` again in 498.15 seconds and
  `scripts/ci/local-release.sh` again in 1,471.38 seconds.

### Independent review closure

- Contract/schema review found lossy unsupported request shapes, shallow path
  limits, number precision, parameter/content reference, `readOnly`, enum-name,
  path-collision, enum-coassertion and malformed parameter/request-body gaps.
  Every valid finding was fixed with fail-closed validation and regressions; the
  closure review passed.
- HTTP/security/performance review found unbounded adversarial work after error
  truncation, incomplete streamed-body overflow provenance and a challenged-401
  request-ID gap for invalid development credentials. All three were fixed and
  the closure review passed.
- Public/repository compatibility review confirmed the additive facade and
  identity scope boundary, published API compatibility, exact task ownership,
  no forbidden workflow change and no valid residual blocker.

### GitHub transport closure

- Immediately before transport, `origin/main` remained
  `b822ca40e72bcfd39667e3dbd9f757ff100775c2`, PR #170 remained open and draft,
  and its remote head remained the expected
  `a077dbc0c5abee54868f66ab41e028cce3d22cfd`.
- The qualified pre-transport commit
  `2ad83d742ca2d7c8e1b9991df7bea71a1f2a359d` replaced that exact head using
  `--force-with-lease=refs/heads/agent/http-request-validation:a077dbc0...`.
  The lease succeeded, the remote head was re-read as `2ad83d742...`, GitHub
  reported the PR clean against current main, and the branch still had zero
  GitHub Actions runs.
- This final task/evidence closure is intentionally followed by regenerated
  source-bound evidence, full local quality and local release qualification,
  task-finish conflict checks and one final exact force-with-lease update. PR
  metadata/readiness may be updated only after that exact final head is proven.
