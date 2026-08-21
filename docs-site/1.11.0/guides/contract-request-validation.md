# Enforce request contracts

Minco can turn reviewed OpenAPI request assertions into direct Rust validation
without a runtime rule registry.

Add `x-minco-request-validation: generated`, describe a closed request schema,
then run `cargo minco contract check` and `cargo minco contract sync`. Adopt
`ValidatedJson<generated::Request>`, or the query/path equivalents, and call the
generated operation policy before the application use case.

Structural failures are stable `400` responses, unsupported JSON media type is
`415`, configured body overflow is `413`, and decoded semantic assertion
failures are bounded `422 validation_failed` responses. Optionality and
nullability remain distinct. Unsupported request-reachable schema assertions
or unrepresentable request shapes fail contract validation; response-only
constructs do not change request DTOs. JSON bodies reference named generated
DTOs, numeric bounds use exact whole 64-bit integer values, and enum schemas
cannot combine assertions that their generated Rust enum would discard.
Malformed parameter, request-body and JSON media schema declarations fail
closed before source generation.

Generated policy checks exact permissions and OpenAPI scope alternatives. It is
only the coarse delivery gate: tenancy, ownership, persistence and business
authorization remain in the application layer.

The standard middleware also bounds request IDs before tracing, limits declared
and streamed bodies without buffering, normalizes explicitly observed overflow
for every extractor, and creates explicit timeout failures without rewriting
application-owned 408/413 responses.

See the [request boundary reference](../reference/http-request-boundary) for the
supported subset, exact limits and failure taxonomy.
