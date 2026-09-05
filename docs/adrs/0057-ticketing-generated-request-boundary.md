# ADR 0057: Ticketing adopts the contract-derived request boundary

## Status

Accepted.

## Context

ADR-0047 established generated request validation: contracts annotated
`x-minco-request-validation: generated` get deterministic request DTOs
whose `ContractValidate` implementations enforce schema-derived bounds at
extraction time. The ticketing plugin never adopted it — its handlers
hand-rolled `ApiJson` extraction. The Phase-2 review flagged this as the
last open correctness finding: bounds lived in two places (OpenAPI and
hand-written validators) with no guarantee they agree.

## Decision

1. The ticketing OpenAPI contract declares
   `x-minco-request-validation: generated`. A deterministic
   `src/generated.rs` is committed for the plugin contract and guarded by
   a freshness test that regenerates and compares on every run (write
   only under an explicit `UPDATE_MINCO_GENERATED=1`, mirroring the
   deterministic-sync rule for `@generated` files). The application-level
   `minco contract sync` command is app-manifest scoped, so the plugin
   owns its own deterministic sync through this test.
2. Every request-body handler extracts through
   `ValidatedJson<generated::…Request>`: generated bounds run before
   handler logic, then the handler maps the DTO into the existing
   application inputs. Handlers still contain no business rules;
   authorization stays in the service layer, which is finer-grained than
   operation scope, so `authorize_operation` coarse claims are not
   adopted.
3. Query parsing for list endpoints stays strict and bounded as-is; the
   generator does not cover query schemas today, and adopting a
   hand-rolled duplicate would recreate the two-sources problem.

## Consequences

- Schema bounds have one source of truth: the contract. Editing an
  OpenAPI bound and running the guarded sync changes extraction
  validation deterministically.
- The plugin gains a `minco-contract` dependency (core-only, no HTTP or
  provider crates) and a committed generated module.
- Request DTO mapping code lives in handlers where shapes differ from
  application inputs; identical shapes map field-for-field.

## Alternatives considered

- **Keep hand-rolled extraction** — rejected: the two-sources drift is
  the defect.
- **Adopt coarse operation authorization too** — rejected: ticketing
  permissions (requester isolation, own-ticket enforcement) are
  finer-grained than ADR-0047's operation-scope claims.
