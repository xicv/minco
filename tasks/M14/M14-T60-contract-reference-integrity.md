---
id: M14-T60
title: Enforce local contract reference integrity and restore the dangling ticketing schemas
milestone: M14
status: active
priority: high
area: contracts/ticketing
depends_on: [M14-T59]
operations: []
owned_paths:
  - crates/minco-contract/src/validate.rs
  - docs/DECISIONS.md
  - docs/adrs/0062-contract-reference-integrity.md
  - docs/reference/generated/diagnostics.md
  - plugins/minco-plugin-ticketing/openapi/openapi.yaml
  - plugins/minco-plugin-ticketing/src/generated.rs
  - tasks/M14/M14-T60-contract-reference-integrity.md
  - verification/1.9-performance-baseline.json
  - verification/source-manifest.json
  - verification/operational-evidence-validation.json
  - verification/static-validation.json
  - verification/publish-validation.json
  - verification/deep-review.json
checks:
  - cargo test -p minco-contract --locked
  - cargo test -p minco-plugin-ticketing --all-features --locked
  - cargo clippy -p minco-contract -p minco-plugin-ticketing --all-targets --all-features --locked -- -D warnings
  - cargo minco check --with-cargo
---

# M14-T60 - Enforce local contract reference integrity and restore the dangling ticketing schemas

Blocker fix found while repairing local VCS state: the ticketing plugin
contract has shipped since M14-T45 with two dangling response references
(`#/components/schemas/AgentBootstrap`,
`#/components/schemas/TicketSummaryCollection`). The definitions were
written in the default workspace in front of the M14-T45 anchor and never
committed with the task branch. `minco_contract::load_contract` never
resolved local `$ref` targets, so every gate stayed green while the shipped
document was invalid for any consumer that resolves references.

## Goal

- `minco-contract` resolves every local `#/…` `$ref` with JSON Pointer
  semantics (`~0`/`~1`, array indices) across the whole document and reports
  unresolved targets as Error finding `MINCO-CONTRACT-037`. External
  references stay out of scope.
- Restore the three missing ticketing schemas (`AgentBootstrap`,
  `TicketSummary`, `TicketSummaryCollection`) exactly matching the
  implementation (`AgentConsoleBootstrap`, the summary wire shape, and the
  `ResourceCollection` page envelope).
- Re-sync `src/generated.rs` deterministically; the boundary test pins the
  new contract digest.
- The existing plugin tests that assert `report.is_valid()` become the
  end-to-end failing tests: before the schema fix they fail on the new
  finding, after it they pass.

## Non-goals

- SES receiving-rule configuration, Lambda bindings, Rustack seam proof,
  outbound email (Stage D2 slice 3b).
- Any change to request handling; the new generated DTOs are response-only
  and extracted by no handler.

## Evidence

Run 2026-08-25 in the `minco-task-m14-t60` workspace:

- `cargo test -p minco-contract --locked` — ok, 5 passed (3 new: dangling
  reference flagged with location and target; resolving reference accepted;
  JSON Pointer escapes and array indices).
- Before the openapi fix, `cargo test -p minco-plugin-ticketing
  --all-features --locked generated_request_boundary_is_current` failed on
  the two `MINCO-CONTRACT-037` findings (defect proven detected by the
  existing gate-facing test).
- After the fix and `UPDATE_MINCO_GENERATED=1` regeneration: full plugin
  suite green; `cargo clippy -p minco-contract -p minco-plugin-ticketing
  --all-targets --all-features --locked -- -D warnings` clean; contract
  digest `fe1bf153d70e00e1aa2b4415803f51cc2d1e48b6b2aff57062281015da4df7ac`.
- All other repository contracts (orders reference, feedback,
  object-transfers) scanned: zero dangling references; blast radius is the
  ticketing contract alone.
- `docs/reference/generated/diagnostics.md` regenerated
  (`uv run --locked python scripts/docs/generate_reference.py`): 590 declared
  codes, `MINCO-CONTRACT-037` inventoried.
- Evidence chain: `validate_static --output`, `validate_publish --output`,
  `source_manifest` (stable across re-runs after the final content freeze),
  baseline re-bound to the frozen tree, `validate_operational_evidence
  --output` PASS, `deep_review.py` rerun.
- `cargo minco check --with-cargo` — result recorded at finish.
