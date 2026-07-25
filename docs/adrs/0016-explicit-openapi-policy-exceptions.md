# ADR 0016: Explicit OpenAPI policy exceptions

Status: Accepted

## Context

Existing applications sometimes need map-shaped JSON while Minco's constrained
OpenAPI profile closes object schemas by default. Authentication and
idempotency metadata can also drift from effective OpenAPI declarations when
path parameters, references or anonymous security alternatives are ignored.

## Decision

- An object schema is closed with `additionalProperties: false`, or explicitly
  declares both an `additionalProperties` value policy and a non-empty
  `x-minco-open-object.rationale`.
- Validation traverses actual Schema Object positions and recursive schema
  keywords, not examples or arbitrary extension payloads.
- Effective idempotency parameters include path-level parameters, operation
  overrides and local references. The reverse rule applies to mutating methods.
- Effective security follows OpenAPI semantics: absent security, an empty
  array, or an empty security-requirement alternative allows anonymous access.
  Every non-empty Security Requirement is an object whose scheme values are
  arrays of string scopes; malformed entries fail with a stable diagnostic.
- `x-minco-auth` distinguishes public, authenticated and permission-scoped
  declarations but never proves an unverified claim or replaces application
  authorization.
- Error responses in the constrained profile use
  `application/problem+json`.

## Alternatives rejected

Silently accepting omitted `additionalProperties` makes compatibility and data
retention accidental. Scanning every JSON object misclassifies examples.
Trusting `x-minco-auth` without OpenAPI security, or checking only inline
operation parameters, produces incorrect generated authorization metadata.

## Compatibility impact

Previously accepted ambiguous schemas or contradictory metadata fail with
stable diagnostics. Applications can migrate deliberately by closing the
schema or documenting a bounded open-map value policy. Generated operation
inventory remains structurally compatible.

## Security and cost impact

The policy prevents accidental unbounded object shapes and public/private route
misclassification. It adds validation work only at contract load/generation
time and introduces no runtime, provider or infrastructure cost.

## Rollback and removal

The validator changes can be removed by reverting this ADR and its fixtures;
there is no persisted state or deployment mutation. Applications should not
remove an explicit exception until consumers no longer rely on the open keys.
