# Contract-First Design

Minco supports a deliberately constrained OpenAPI 3.1 profile. Constraints trade breadth
for deterministic generation, clear diagnostics, and reliable AI reasoning.

## Required conventions

- Every operation has a globally unique lowerCamelCase `operationId`.
- Every operation declares at least one 2xx response and a Problem response.
- JSON is the initial request/response body format.
- Operations marked `x-minco-idempotent: true` declare an effective required
  `Idempotency-Key` header. On mutating operations, the required header also
  requires the extension. Path-level parameters, operation overrides and local
  parameter references participate in the check.
- OpenAPI's absent `security`, `security: []`, and any security alternative
  containing `{}` allow anonymous access. `x-minco-auth` may be `public`,
  `authenticated`, or a `permission_scoped` object with a non-empty permission
  list, and must agree with that effective security. Permission metadata is
  descriptive; application use cases still authorize verified principals.
- Object schemas set `additionalProperties: false`, or explicitly declare an
  `additionalProperties` value policy and a non-empty
  `x-minco-open-object.rationale`.
- Unsupported schema constructs fail; they are never silently approximated.

## Deterministic generation

`minco contract sync` writes checked-in Rust DTOs and constants. Generated output contains
the Minco generator version and canonical SHA-256 digest. CI and local checks compare the
committed file against regenerated output.

The operation inventory drives both Axum binding and API Gateway routes, preventing a
separate hand-maintained route table in infrastructure.

## Transport/domain separation

Generated DTOs are transport types. They are not domain entities. Handlers map them to
application commands, allowing the domain model to preserve stronger invariants and evolve
without leaking transport concerns inward.
