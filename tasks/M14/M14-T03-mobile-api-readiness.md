---
id: M14-T03
title: Add frontend-neutral mobile API readiness
milestone: M14
status: complete
priority: high
area: http/api-clients
depends_on: [M14-T01]
operations: []
owned_paths:
  - crates/minco-cli/templates/app/environments/dev-postgres.toml.tmpl
  - crates/minco-cli/templates/app/environments/dev-sqlite.toml.tmpl
  - crates/minco-http/README.md
  - crates/minco-http/src/lib.rs
  - crates/minco-http/src/middleware.rs
  - crates/minco-http/src/response.rs
  - crates/minco-plan/README.md
  - crates/minco-plan/src/lib.rs
  - crates/minco-plan/src/model.rs
  - crates/minco-plan/src/sam.rs
  - crates/minco-plan/src/sam_cross_client.rs
  - docs/how-to/mobile-api.md
  - docs/reference/generated/diagnostics.md
  - docs/reference/generated/schemas.md
  - examples/orders/config/minco.aurora-serverless-v2.toml
  - examples/orders/config/minco.dev.toml
  - examples/orders/config/minco.dynamodb.toml
  - examples/orders/config/minco.local-sqlite.toml
  - examples/orders/config/minco.neon-launch.toml
  - examples/orders/config/minco.rds-postgres.toml
  - examples/orders/config/minco.self-hosted-postgres.toml
  - infra/aws/generated/plan.json
  - infra/aws/generated/template.yaml
  - tasks/M14/M14-T03-mobile-api-readiness.md
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - rustfmt +1.97.1 --edition 2024 --check crates/minco-http/src/lib.rs crates/minco-http/src/middleware.rs crates/minco-http/src/response.rs crates/minco-plan/src/lib.rs crates/minco-plan/src/model.rs crates/minco-plan/src/sam.rs
  - cargo +1.97.1 clippy -p minco-http --all-targets --locked -- -D warnings
  - cargo +1.97.1 clippy -p minco-plan --all-targets --locked -- -D warnings
  - cargo +1.97.1 test -p minco-http -p minco-plan -p cargo-minco --locked
  - scripts/docs/generate-reference.sh --check
  - bash scripts/aws/plan.sh
  - jj file show -r @- infra/aws/generated/plan.json | cmp - infra/aws/generated/plan.json
  - jj file show -r @- infra/aws/generated/template.yaml | cmp - infra/aws/generated/template.yaml
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/source_manifest.py --check
---

## Goal

Make Minco's existing frontend-neutral HTTP boundary safe and practical for
browser, iOS, Android, desktop, automation, and server clients without creating
a second mobile business API or adding always-on infrastructure.

## Acceptance

- the default exact runtime CORS policy accepts the conditional request fields
  used by Minco resource APIs and exposes their standard response metadata to
  browser JavaScript;
- the generated AWS HTTP API policy mirrors those required request and exposed
  response headers through explicit Plan IR instead of relying on Lambda CORS
  fields that API Gateway discards when gateway CORS is configured;
- applications can attach RFC bearer challenges, retry timing, deprecation,
  sunset, and migration links to any Axum response without changing the
  `ApiFailure` representation;
- tests cover response status preservation, numeric and HTTP-date retry values,
  repeated links, pre-Unix timestamp rejection, exact policy inventory,
  applied runtime CORS behavior, and generated SAM CORS metadata;
- project templates, example profiles, and checked-in AWS evidence agree on the
  conditional request boundary;
- documentation distinguishes browser constraints from native-client
  constraints and gives a current OAuth, retry, direct-transfer, compatibility,
  AWS ingress, and optional app-integrity profile; and
- the change remains additive, provider-neutral, zero-idle, and compatible with
  the canonical OpenAPI 3.1 and direct Axum/Tower decisions.

## Non-goals

- adding mobile-only routes, DTOs, repositories, or a second API version;
- implementing a hosted OAuth provider, refresh-token store, device-attestation
  verifier, push service, offline merge engine, or app-version gate;
- weakening exact CORS, moving authorization out of application use cases, or
  treating CORS or attestation as authentication;
- adding product- or plugin-specific ingress response-header configuration;
- changing generated OpenAPI bindings or application business operations; or
- publishing, deploying, promoting, or enabling a live application.

## Checklist

- [x] Refresh the local checkout, `origin/main`, the existing PR bookmark, PR
  metadata, checks, and release truth before editing.
- [x] Read the repository contract, roadmap, task, relevant ADRs, contract,
  generator, plugin, and AWS deployment documentation.
- [x] Research the version-matched Rust HTTP dependencies and current primary
  OAuth, HTTP metadata, API Gateway CORS/JWT, SAM, and AWS mobile-sync sources.
- [x] Rebase the existing PR change onto refreshed `main` in a dedicated JJ
  workspace without touching the unrelated existing M14-T03-named workspace.
- [x] Deep-review the complete PR and remove hidden SAM post-processing.
- [x] Keep one external OpenAPI 3.1 contract and one set of application use
  cases, DTOs, ports, and adapters for every client class.
- [x] Make exact request and response header inventories visible at runtime,
  in Plan IR, and in generated SAM, with exact-inventory regression tests.
- [x] Regenerate reference and AWS evidence through their generators.
- [x] Run exact-file rustfmt, package-scoped Clippy, targeted tests, contract,
  generator, static, Plan inspection, documentation, and SAM validation gates.
- [x] Confirm the generated plan has no NAT Gateway, provisioned concurrency,
  scheduled wakeup, fixed compute, or other new idle-cost resource.
- [x] Restore exact-SHA hosted qualification by coupling source-manifest
  evidence to the task and requiring a green manual essential run before merge.
- [x] Keep PR #124 draft and make no merge, deploy, publication, promotion,
  release, version, changelog, tag, registry, or live-AWS mutation.

## Evidence

### Exact starting state

On 2026-08-09, before task edits, the repository root was
`/Users/xicao/Projects/minco`, Git reported `## HEAD (no branch)`, and `origin`
was `git@github.com:xicv/minco.git`. After `git fetch origin --prune`:

- `origin/main` was `d4cfe76736c26f414b94aa39481943e083e3d336`;
- `origin/task/m14-t03-mobile-api-readiness` was
  `f54f9308c2f9d3de6403fd8a98102388829c7841`;
- PR #124 was open, draft, mergeable, based on `main`, and still pointed to
  that exact branch head; its recorded base OID was the older
  `4d81543f7c5adb773655f23278abfe084de9f3e0`;
- `gh pr checks 124 --repo xicv/minco` reported no checks; and
- GitHub release `v1.1.0` was already published and remained untouched.

The existing `/Users/xicao/Projects/minco-task-m14-t03` workspace contained
unrelated work and was preserved. This PR was rebased with JJ onto refreshed
`main` in `/Users/xicao/Projects/minco-task-m14-t03-mobile-api-readiness`.

### Research and review corrections

The repository pins Rust 1.97.1 and resolves Axum 0.8.9, tower-http 0.7.0,
http 1.5.0, serde 1.0.229, serde_json 1.0.151, and uuid 1.24.0. Context had no
matching package and Context Hub had no suitable result, so the exact local
published crate sources and primary AWS/IETF sources were used. Research
confirmed API Gateway HTTP API gateway-owned CORS and JWT behavior, native-app
external-browser OAuth with PKCE, bearer challenge syntax, `Retry-After`,
`Deprecation`, `Sunset`, SAM `ExposeHeaders`, and the current Cognito Sync
availability boundary.

The deep review corrected the original PR in these material ways:

- removed `sam_cross_client.rs`, which mutated the Plan and post-processed
  rendered YAML outside the primary SAM renderer;
- made `If-Match` and `If-None-Match` part of the default DeploymentConfig
  boundary, including omitted-config defaults;
- added a derived, serialized `exposed_headers` Plan field and made the primary
  SAM renderer consume that visible field directly;
- added exact ordered runtime, applied CORS, Plan, and generated SAM inventory
  assertions instead of containment-only tests;
- kept response metadata transport-only and application authorization owned by
  the application use case;
- corrected the task package name and replaced Git-only generated-file checks
  with JJ-compatible checks; and
- regenerated diagnostics, schema, Plan, SAM, static-validation, and deep-review
  evidence only through their generators.

Exact default request headers are `authorization`, `content-type`,
`idempotency-key`, `if-match`, `if-none-match`, and `x-request-id`. Exact exposed
response headers are `deprecation`, `etag`, `link`, `location`, `retry-after`,
`sunset`, `www-authenticate`, and `x-request-id`. Origins remain exact; no
wildcard origin or header was added.

### Local qualification

The following commands completed successfully in the dedicated JJ workspace:

- exact-file `rustfmt +1.97.1 --edition 2024 --check` for the six modified Rust
  files listed in `checks`;
- package-scoped `cargo +1.97.1 clippy` for `minco-http` and `minco-plan`, with
  all targets, the lockfile, and warnings denied;
- `cargo +1.97.1 test -p minco-http -p minco-plan -p cargo-minco --locked`;
- `cargo minco contract check` and `cargo minco contract sync --check`, with
  OpenAPI 3.1.0, seven operations, zero findings, and contract digest
  `f0e0d1aee9858e54f814270b6f78a5c63ea6993311210f6a6f6ee7323838843f`;
- `cargo minco inspect --json`, `cargo minco explain updateOrder --json`, and
  `cargo minco deploy plan --stdout --json`;
- `scripts/docs/generate-reference.sh`, `scripts/docs/check-snippets.sh` (252
  fenced blocks), and `bash scripts/aws/plan.sh`;
- `uv run --locked python scripts/validate_static.py`, with zero errors and
  zero warnings; and
- `bash scripts/aws/validate.sh`, including a successful
  `sam validate --lint` of `infra/aws/generated/template.yaml`.

The inspected Plan carries the exact six allowed and eight exposed headers,
`scheduled_wakeups: []`, `uses_nat_gateway: false`, zero provisioned
concurrency, and policies denying fixed compute, NAT, provisioned concurrency,
and scheduled wakeups. `explain updateOrder` traces the canonical OpenAPI
operation through one HTTP handler and one application use case to the existing
memory, PostgreSQL, SQLite, and DynamoDB adapters.

One attempted `cargo minco check --with-cargo` run is not claimed as passing.
Its repository-defined Rust gate unexpectedly invoked the expressly forbidden
`cargo clippy --workspace`; that run stopped on seven redundant `#[must_use]`
annotations. The annotations were corrected, the broad command was not rerun,
and the affected `minco-http` and `minco-plan` packages were instead linted
successfully at the narrowest supported package scope.

The repository owner later authorized the existing hosted essential workflow's
check-only broad formatting on its ephemeral runner. Exact-head run
`31288686621` passed static validation, generated-reference verification, 41
repository-truth tests, four hosted-policy tests, ten recipe checks, and the
workspace compiler check before finding that
`verification/source-manifest.json` was stale after this PR added a Rust source
file. This candidate regenerates that evidence with
`scripts/source_manifest.py`, owns the coupled output, and requires a fresh
green exact-SHA hosted run before merge.

No live client/device, AWS runtime, deployment, artifact promotion, registry,
release, or publication proof is claimed. The source task is complete; PR #124
remains draft until the replacement hosted essential run passes.
