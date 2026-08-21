# ADR 0047: Contract-derived request validation and coarse authorization

## Status

Accepted

## Context

OpenAPI is already Minco's canonical external HTTP contract, but generated DTOs
previously enforced only their Serde shape. Applications repeated semantic
checks and translated Axum rejections independently. API Gateway HTTP APIs do
not perform request-schema validation, so a reviewed schema was not executable
at the deployed application boundary.

Research refreshed on 2026-08-21 used OpenAPI 3.1.1, JSON Schema 2020-12,
Axum 0.8.9, Tower 0.5.3, tower-http 0.7.0, Laravel 13 and AWS HTTP API primary
documentation plus the exact locked Rust sources. The relevant conclusions are:

- JSON Schema length is counted in Unicode code points; bounds, collection
  sizes, scalar `enum`/`const`, and object property counts are assertions.
- `format` is an annotation unless a vocabulary or a deliberately selected
  typed parser asserts it.
- security schemes inside one OpenAPI Security Requirement are AND-composed;
  requirement objects are alternatives, and `{}` permits anonymous access.
- Axum extraction can be wrapped without parsing a second time, but native
  rejection text is not a stable public contract.
- the pinned tower-http limit and timeout responses do not expose reliable
  public provenance. Rewriting arbitrary 408/413 responses by status and media
  type would corrupt application-owned responses.
- Laravel's Form Request and policy ergonomics are useful, but its runtime rule
  DSL, service container, facades and Active Record do not fit Minco.

Primary references:

- <https://spec.openapis.org/oas/v3.1.1.html>
- <https://json-schema.org/draft/2020-12/json-schema-validation>
- <https://docs.rs/axum/0.8.9/axum/extract/index.html>
- <https://docs.rs/tower/0.5.3/tower/struct.ServiceBuilder.html>
- <https://docs.rs/tower-http/0.7.0/tower_http/>
- <https://laravel.com/docs/13.x/validation>
- <https://laravel.com/docs/13.x/authorization>
- <https://docs.aws.amazon.com/apigateway/latest/developerguide/http-api-open-api.html>
- <https://github.com/jprochazk/garde>
- <https://github.com/Keats/validator>

## Decision

OpenAPI remains canonical. A contract opts into executable request validation
with exactly:

```yaml
x-minco-request-validation: generated
```

`minco-contract` analyzes only request-reachable JSON body, query and path
schemas. It follows bounded local references and rejects unsupported assertions,
external references, recursion, malformed bounds, unsafe generated identifiers
and excessive depth, properties, enum members or total nodes using stable
`MINCO-CONTRACT-*` diagnostics. Response-only schema constructs do not block
request generation or change response DTO deserialization.

The supported generated subset is closed objects and required shape,
missing-versus-null semantics, string/item/property bounds, inclusive and
exclusive whole 64-bit integer bounds, scalar `enum` and `const`, nested local
references and nested arrays. JSON body roots must reference a named object or
string enum. Inline object shapes, mathematical `number`, standalone `null`,
`$ref` siblings, content-based parameters, chained component references,
request `readOnly` and ambiguous public path segments fail closed because the
generator cannot represent them losslessly. String enums combined with lexical
or value assertions also fail closed instead of silently discarding the extra
assertions. Malformed parameter and request-body objects, including JSON media
types without schemas, are rejected before generation. Optional nullable
properties whose presence cannot be represented without changing the public
field shape fail closed when combined with presence-sensitive assertions.
Arbitrary conditionals, composition, external references and recursive schemas
are not approximated.

Generated DTOs implement a small provider-neutral `ContractValidate` trait.
The error collector uses inline path state on the success path and allocates
only when a failure is recorded. Paths, messages, nesting, field count and
messages per field are bounded. Deep valid paths do not fail merely because a
public path cannot be materialized; an actual deep failure produces the omission
sentinel. Once truncated, later validation work stops, and an oversized array
inspects no more than its declared maximum. Generated checks contain public rule
text only; they do not retain request values.

`minco-http` provides `ValidatedJson<T>`, `ValidatedQuery<T>` and
`ValidatedPath<T>`. Each delegates once to the native Axum extractor, invokes
static contract validation, and maps the outcome to a stable 400/413/415/422
Problem Details boundary. Extraction owns delivery concerns only. Async checks,
database uniqueness, tenancy, resource ownership and business invariants remain
application use-case responsibilities.

Authorization is generated as separate `ContractAuthorizationPolicy` constants,
not fields on the published `ContractOperation`. The runtime requires exact
permissions and exact scope tokens, preserves OpenAPI alternative semantics and
denies before calling the use case. Application authorization remains defence
in depth. Identity scopes cross the existing public `Principal` shape through a
reserved provider-neutral namespaced claim and additive methods.

Client request IDs are untrusted. The standard middleware replaces missing,
non-ASCII, unsafe or over-128-byte values with UUIDv7 before propagation and
tracing. Problem rendering validates once more so application-created failures
cannot reflect an unsafe identifier.

The standard HTTP stack owns an explicit streamed body wrapper and timeout.
Declared oversize is rejected early, streamed input remains bounded without
buffering, and Axum's independent default body limit is disabled under this
policy. The wrapper records shared overflow provenance, allowing the outer
middleware to normalize native and validated extractor failures. Minco-owned
failures produce Problem Details directly; application-owned 408 and 413
responses pass through byte-for-byte. No response-body inspection or
status/content-type provenance heuristic is used.

All additions preserve the published fields and constructors of
`ContractOperation`, `OwnedOperation`, `Principal`, `ApiFailure` and
`ProblemDetails`. Contracts without the opt-in profile keep their existing DTO
shape. Validation and authorization primitives are additive re-exports through
`minco-http` and the `minco` facade prelude.

## Consequences

- The reviewed request contract becomes executable without a runtime rule
  registry, reflection, regex engine, second JSON tree or network/database call.
- Valid string bounds scan Unicode code points once; validation paths and
  messages are materialized only on failure. Mathematical `number` is rejected
  in the generated profile instead of being rounded through `f64`.
- The direct dependencies added to `minco-http` are already pinned workspace
  runtime crates used to wrap streaming bodies and timeouts; no hosted service,
  AWS resource, wake source, schedule, fixed compute or default provider is
  added.
- Applications must opt in and adopt the typed extractor to receive the stable
  semantic request boundary. Specialized resource query parsing remains valid.
- Unsupported or unrepresentable opted-in request schemas fail during contract
  validation rather than silently losing assertions.

## Alternatives rejected

### Runtime validation DSL or derive dependency

This duplicates the canonical OpenAPI rules, adds runtime/default dependency
cost and permits drift. Direct deterministic generation is smaller and easier
to inspect.

### Validate in the domain or application layer only

Business invariants remain there, but structural and contract assertion errors
need a consistent delivery boundary before a use case is invoked.

### Let API Gateway validate requests

HTTP APIs do not provide the required request-schema validation during OpenAPI
import. This would also make local and provider behavior diverge.

### Rewrite every 408 or 413 response after the handler

Status and content type do not establish provenance, and inspecting response
bodies would break streaming. Explicit Minco ownership preserves application
responses and makes ordering testable.

### Add scopes or policy fields to published structs

Required public fields break downstream struct literals. Separate policy
constants and a reserved namespaced claim keep the change additive.
