---
id: M14-T03
title: Add frontend-neutral mobile API readiness
milestone: M14
status: active
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
  - crates/minco-plan/src/lib.rs
  - crates/minco-plan/src/sam_cross_client.rs
  - docs/how-to/mobile-api.md
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
checks:
  - rustfmt +1.97.1 --edition 2024 --check crates/minco-http/src/lib.rs crates/minco-http/src/middleware.rs crates/minco-http/src/response.rs crates/minco-plan/src/lib.rs crates/minco-plan/src/sam_cross_client.rs
  - cargo +1.97.1 test -p minco-http -p minco-plan -p minco-cli --locked
  - bash scripts/aws/plan.sh
  - git diff --exit-code -- infra/aws/generated/plan.json infra/aws/generated/template.yaml
  - uv run --locked python scripts/validate_static.py
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
  response headers instead of relying on Lambda CORS fields that API Gateway
  discards when gateway CORS is configured;
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

## Evidence

Started on 2026-08-06 from exact merged main
`96a964f5663cbff66892601d041987280fe60618` on
`task/m14-t03-mobile-api-readiness`.

Research reviewed the current native-app OAuth and PKCE security guidance,
bearer challenges, HTTP Problem Details, retry semantics, deprecation and
sunset fields, AWS JWT authorizers and HTTP API CORS ownership, Apple App
Attest, Google Play Integrity, and current AWS mobile-sync availability before
implementation. The code audit confirmed that Minco already had idempotency,
strong entity tags, conditional writes, bounded cursors, stable problems,
request IDs, direct object-access signing, and API Gateway principal mapping.
It also found that the runtime policy did not allow `If-Match`/`If-None-Match`
or expose standard resource and lifecycle fields, and that the generated API
Gateway CORS policy would override backend CORS without exposing or accepting
those fields.

The established SAM renderer remains byte-identical to the task baseline. A
small `sam_cross_client` wrapper owns the intentional header normalization and
response-exposure injection while preserving the existing public render API and
all original renderer tests. Checked-in example inputs and generated evidence
were updated only at the relevant header lists.

No repository-wide formatting or linting is authorized by this task. The
available environment has `uv` but no local Rust toolchain or `chub`, and the
private repository is available through the GitHub connector rather than a
runnable checkout. The repository's qualification workflow is manual-only and
does not run automatically for this pull request. Source and exact-diff review
are therefore not recorded as compiler, test, formatter, linter, generator, or
static-validation evidence. The exact targeted commands above remain pending
in an equipped checkout or manually dispatched hosted workflow.
