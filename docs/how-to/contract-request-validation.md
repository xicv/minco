# Enforce request contracts

Use generated request validation when an OpenAPI operation accepts untrusted
JSON and its structural assertions should be enforced before the use case.

## Opt in and describe the request

Add the one document-level profile and keep request DTOs under
`components.schemas`:

```yaml
openapi: 3.1.1
x-minco-request-validation: generated
paths:
  /widgets:
    post:
      operationId: createWidget
      security:
        - bearer: [widgets:write]
      x-minco-auth:
        mode: permission_scoped
        permissions: [widgets.create]
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/CreateWidget'
      responses:
        '422':
          description: Semantic validation failed
          content:
            application/problem+json: {}
components:
  schemas:
    CreateWidget:
      type: object
      additionalProperties: false
      required: [name]
      properties:
        name: {type: string, minLength: 2, maxLength: 64}
```

Run the contract authority before compiling:

```bash
cargo minco contract check
cargo minco contract sync
cargo minco contract sync --check
```

An unsupported request-reachable assertion fails with a stable
`MINCO-CONTRACT-*` diagnostic. A response-only use of that assertion does not
block request generation. JSON request bodies must reference a named closed
object or named string enum. Inline objects, standalone scalar/array roots,
`$ref` siblings, `readOnly` request properties and other shapes the generator
cannot represent losslessly fail before code generation. Enum schemas cannot
combine assertions such as `minLength` or `const`, because the generated enum
would otherwise discard them. Malformed parameters, request bodies and JSON
media entries without schemas also fail closed.

## Extract once and authorize before the use case

```rust,ignore
use minco_http::{ValidatedJson, authorize_operation};

async fn create_widget(
    principal: Option<&minco_http::Principal>,
    ValidatedJson(request): ValidatedJson<generated::CreateWidget>,
) -> Result<(), minco_http::ApiFailure> {
    authorize_operation(
        principal,
        &generated::CREATE_WIDGET_AUTHORIZATION,
        "request-1",
    )?;
    // Call one application use case. It still owns tenancy, resource ownership,
    // database checks and business invariants.
    let _ = request;
    Ok(())
}
```

`ValidatedJson` delegates to Axum once; it does not create an intermediate
`serde_json::Value`. `ValidatedQuery` and `ValidatedPath` use the corresponding
parts extractors. Keep a stronger specialized query parser when the operation
already has one, as Orders does for bounded resource lists.

## Understand structural and semantic failures

Malformed JSON, wrong types, unknown fields and an explicit `null` for an
optional non-null property are structural `400 invalid_request` failures. A
missing optional property remains `None`. A successfully decoded value outside
`minLength`, an integer `maximum`, `minItems` or another generated assertion is
`422 validation_failed`, with bounded dot/index paths such as
`lines.2.quantity`.

Validation is not a business-invariant layer. Availability, uniqueness,
authorization over stored state and transitions remain application concerns.

## Keep runtime policy explicit

Install `apply_standard_middleware` to normalize request IDs before tracing and
to apply the configured streamed body limit and timeout. Missing or unsafe
request IDs become UUIDv7. Minco-owned 408/413 failures use Problem Details;
explicit overflow provenance normalizes streamed failures for every extractor
under the standard stack, while application responses with those statuses are
not rewritten.

See [HTTP request boundary reference](../reference/http-request-boundary.md) for
the supported assertions, limits and stable error taxonomy.
