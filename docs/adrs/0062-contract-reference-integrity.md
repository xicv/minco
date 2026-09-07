# ADR 0062: Contract validation resolves every local reference

## Status

Accepted.

## Context

The ticketing plugin contract (`plugins/minco-plugin-ticketing/openapi/openapi.yaml`)
shipped since M14-T45 with two response references —
`#/components/schemas/AgentBootstrap` and
`#/components/schemas/TicketSummaryCollection` — whose target definitions do
not exist in the document. The definitions were written in the default
workspace in front of the M14-T45 anchor and never committed with the task;
the task branch carried the paths and `AgentManagement` but not the three
response schemas.

Every gate run stayed green because `minco_contract::load_contract` never
resolved local `$ref` targets: plugin contracts are validated for structure,
operation inventory and request-schema bounds, but a dangling reference is
invisible to all of them. The plugin openapi document is a standalone artifact
(merged into nothing, shipped inside the crate), so any consumer that resolves
references — including the generated-request tooling in principle — receives an
invalid OpenAPI document.

## Decision

1. `minco-contract` treats every local `#/…` `$ref` that does not resolve
   inside the same document as an Error finding (`MINCO-CONTRACT-037`), with
   the reference location and target in the finding message. External
   references are out of scope for local validation.
2. Resolution walks the parsed document with JSON Pointer semantics
   (`~0`/`~1` unescaping, array indexing), so the check covers references in
   paths, responses, parameters, examples and components alike — not only the
   shapes the generator consumes.
3. Every contract validated through `load_contract`/`load_contract_source`
   gains the check: the orders reference, each plugin contract, and the
   in-crate tests that assert `report.is_valid()`.
4. The missing ticketing definitions are restored to match the implementation
   exactly (`AgentConsoleBootstrap` and the summary wire shape behind
   `ResourceCollection<TicketSummary>`), and the generated request boundary is
   re-synced deterministically.

## Consequences

- Adding a `$ref` without its definition now fails the contract owner's own
  tests and the full gate; the class of defect that survived fifteen green
  gates is closed at the tooling level, not per plugin.
- The three restored schemas extend `src/generated.rs` with deterministic DTOs
  (`AgentBootstrap`, `TicketSummary`, `TicketSummaryCollection`); response-only
  schemas are not extracted by handlers, and the boundary test pins the sync.
- Documents that intend fragment semantics (references resolved elsewhere)
  would now be rejected; none exist in the repository, and any future need
  must arrive with an explicit decision.
