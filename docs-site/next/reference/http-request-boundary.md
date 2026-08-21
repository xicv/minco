# HTTP request boundary

The unreleased `x-minco-request-validation: generated` profile supports closed
object and required shape, optional non-null fields, Unicode string bounds,
array and property bounds, inclusive/exclusive whole 64-bit integer bounds,
scalar `enum`/`const`, local references and nested arrays. Unrepresentable roots,
inline objects, standalone `number`/`null`, `$ref` siblings, request `readOnly`
and content-based parameters fail closed before generation. The same applies to
enum schemas with additional value assertions, malformed parameter or
request-body objects, and JSON media entries without schemas.

| Status | Stable code |
|---:|---|
| 400 | `invalid_json`, `invalid_request`, `invalid_query`, `invalid_path` |
| 408 | `request_timeout` |
| 413 | `payload_too_large` |
| 415 | `unsupported_media_type` |
| 422 | `validation_failed` |

Validation retains at most 32 field paths, four messages per path, 256 bytes
per path/message and 16 path segments. `$._truncated` records omission.
Contract analysis is limited to 32 schema levels, 4,096 visited nodes,
256 properties, 128-byte identifiers and 128 enum members.
Deep valid paths remain valid; once truncated, generated traversal stops.

Request IDs accept 1–128 ASCII bytes from alphanumeric, `-`, `_`, `.`, and `:`.
Missing or invalid values become UUIDv7 before tracing and are revalidated at
Problem rendering.

Generated authorization requires all declared Minco permissions and exact
scope tokens, with AND inside an OpenAPI Security Requirement and OR between
requirements. Application authorization remains mandatory.
