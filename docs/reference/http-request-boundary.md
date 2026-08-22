# HTTP request boundary reference

## Contract profile

`x-minco-request-validation: generated` opts one OpenAPI 3.1 document into
request-reachable validation. Without it, existing generated DTO behavior is
unchanged.

Supported request assertions are closed object/required shape,
`minLength`/`maxLength`, `minItems`/`maxItems`,
`minProperties`/`maxProperties`, `minimum`/`maximum`,
`exclusiveMinimum`/`exclusiveMaximum`, scalar `enum`/`const`, local references
and nested arrays. String length is Unicode code-point length. UUID and
date-time remain asserted by their deliberately generated Rust types;
arbitrary `format` values remain annotations. Numeric bounds apply to generated
`integer` fields and must be whole values representable by the generated 64-bit
type; unconstrained mathematical `number` values fail closed rather than being
rounded through binary floating point.

Composition and conditional keywords, external references, recursive graphs,
request-body roots without a named generated DTO, inline object shapes,
standalone `null`, `$ref` siblings, content-based query/path parameters,
chained component references, request `readOnly`, ambiguous dot-path property
names, enum schemas with additional value assertions, malformed parameters or
request bodies, JSON media entries without schemas, unsupported assertion
vocabularies and combinations that cannot preserve missing-versus-null
semantics in the published DTO shape fail closed. Analysis is bounded to 32
levels, 4,096 visited nodes, 256 properties per object, 128-byte generated
identifiers and 128 enum members.

## Validation output limits

| Limit | Value |
|---|---:|
| Field paths, including omission marker | 32 |
| Messages per path | 4 |
| Path bytes | 256 |
| Message bytes | 256 |
| Generated path depth | 16 |

Additional failures collapse into the deterministic `$._truncated` path.
Messages contain rule text only, never request values or parser/provider errors.
Deep valid traversal does not itself create an error. Once omission is recorded,
later generated validation work is skipped; oversized arrays inspect at most
their declared `maxItems` elements.

## HTTP failure taxonomy

| Status | Code | Meaning |
|---:|---|---|
| 400 | `invalid_json` | malformed JSON |
| 400 | `invalid_request` | structurally undecodable JSON, invalid null, type, field or typed format |
| 400 | `invalid_query` | query decoding or shape failure |
| 400 | `invalid_path` | path decoding or shape failure |
| 408 | `request_timeout` | configured Minco timeout elapsed |
| 413 | `payload_too_large` | configured declared or streamed body limit exceeded |
| 415 | `unsupported_media_type` | JSON content type missing or unsupported |
| 422 | `validation_failed` | decoded DTO violates generated semantic assertions |

All failures use `application/problem+json`. A safe request ID appears in both
the body and `x-request-id` response header. `401 unauthenticated` includes
`WWW-Authenticate: Bearer`; authenticated principals missing an exact required
permission or scope receive `403 forbidden`. Invalid development credentials
use the same correlated challenged `401` boundary without exposing claim data.

## Authorization semantics

Generated `ContractAuthorizationPolicy` constants are separate from
`ContractOperation`. Every `x-minco-auth` permission is required. Scope tokens
within one OpenAPI Security Requirement are AND-composed; requirement objects
are alternatives. `{}` permits anonymous access. Multi-scheme AND requirements
fail contract validation rather than being flattened.

This is coarse delivery authorization only. Application use cases continue to
enforce tenancy, ownership, stored state and business policy.

## Runtime and cost

JSON is deserialized once and validation uses static dispatch. Valid requests
do not build a second JSON tree or allocate field-path strings. The body wrapper
limits streamed input without buffering the full request and records explicit
overflow provenance, so native and validated extractors share the Minco 413
response. Timeout and limit middleware do not inspect application response
bodies. The boundary adds no AWS resource, wake source, fixed compute, schedule
or managed validator.
