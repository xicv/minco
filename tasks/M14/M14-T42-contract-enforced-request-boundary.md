---
id: M14-T42
title: Add a contract-enforced request boundary
milestone: M14
status: active
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
  - examples/orders/api/src/generated.rs
  - examples/orders/api/src/lib.rs
  - examples/orders/api/tests/**
  - docs/DECISIONS.md
  - docs/adrs/0047-contract-derived-request-boundary.md
  - docs/how-to/contract-request-validation.md
  - docs/reference/http-request-boundary.md
  - docs-site/next/guides/contract-request-validation.md
  - docs-site/next/reference/http-request-boundary.md
  - docs-site/.vitepress/config.mts
  - tasks/M14/M14-T42-contract-enforced-request-boundary.md
checks:
  - cargo minco contract check
  - cargo minco contract sync --check
  - cargo +1.97.1 check -p minco-contract -p minco-http -p minco-plugin-identity -p orders-api --all-targets --all-features --locked
  - cargo +1.97.1 test -p minco-contract -p minco-http -p minco-plugin-identity -p orders-api --all-targets --all-features --locked
  - cargo +1.97.1 clippy -p minco-contract -p minco-http -p minco-plugin-identity -p orders-api --all-targets --all-features --locked -- -D warnings
  - cargo semver-checks -p minco-contract -p minco-http -p minco-plugin-identity -p minco --baseline-version 1.10.0
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
- [ ] Provider-neutral validation errors are deterministic, bounded and cheap
  when empty; supported request assertions and Unicode semantics are proven.
- [ ] Request-reachable traversal rejects unsupported assertions, malformed
  bounds, invalid/external refs, cycles and excess complexity deterministically.
- [ ] Deterministic generation implements validation and missing-versus-null
  semantics without changing non-opted DTO public shapes.
- [ ] `ValidatedJson`, `ValidatedQuery` and `ValidatedPath` use native extraction
  once and return the documented 400/413/415/422 taxonomy without raw errors.
- [ ] Separate generated authorization preserves public structs, exact AND/OR
  semantics and application authorization; denied requests call no use case or
  persistence port. Identity scopes reach the additive principal boundary.
- [ ] Unsafe request IDs are replaced before tracing and Problem rendering, and
  body/header correlation is exact.
- [ ] Minco-owned timeout and streamed body-limit provenance returns 408/413
  Problem Details while preserving application responses and all existing CORS,
  sensitive-header and compression behavior.
- [ ] Orders OpenAPI, generated code and in-process Axum tests prove the complete
  place/update vertical slice; generated source is never hand-edited.
- [ ] Public compatibility fixtures, facade feature checks and published 1.10.0
  SemVer checks pass.
- [ ] Focused checks, exact-file rustfmt checks, documentation checks,
  `scripts/quality.sh` and `scripts/ci/local-release.sh` pass on the final tree.
- [ ] Three independent read-only reviews cover contract/schema correctness,
  HTTP/security/performance and public/repository compatibility; every valid
  finding is fixed and requalified.
- [ ] PR #170 is replaced only with exact force-with-lease after confirming its
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

Implementation and exact final-source command results will be appended here.
No check below this preliminary ownership revision is claimed as implementation
evidence, and no AWS, workflow, deployment, publication, release or production
operation is authorized.
