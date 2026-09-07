---
id: M15-T01
title: Desk workspace, project and integration-profile isolation foundation
milestone: M15
status: ready
priority: critical
area: security/isolation
depends_on: [M14-T74]
operations: []
owned_paths:
  - docs/adrs/0076-workspace-project-integration-isolation.md
  - docs/DECISIONS.md
  - docs/research/desk-isolation-design-review-2026-09.md
  - docs/reference/generated/features.md
  - docs/reference/generated/packages.md
  - docs/reference/generated/plugins.md
  - roadmap/roadmap.yaml
  - tasks/M14/M14-T74-desk-stabilization-merge-gate.md
  - tasks/M15/M15-T01-desk-isolation-foundation.md
  - plugins/catalog.toml
  - plugins/minco-plugin-workspace
  - plugins/minco-plugin-ticketing
  - examples/minco-desk
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p minco-plugin-workspace -p minco-plugin-ticketing -p minco-desk-example --all-targets --locked
  - cargo clippy -p minco-plugin-workspace -p minco-plugin-ticketing -p minco-desk-example --all-targets --locked -- -D warnings
  - cargo minco plugin validate
  - ./scripts/quality.sh
  - bash scripts/ci/local-release.sh
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/source_manifest.py --check
---

# M15-T01 - Desk workspace, project and integration-profile isolation foundation

The first bounded follow-up after PR #187 (merged `d7939910`, 2026-09-07):
a fully enforced but deliberately simple isolation boundary for Minco Desk,
per ADR-0076 and the settled design review
([docs/research/desk-isolation-design-review-2026-09.md](../../docs/research/desk-isolation-design-review-2026-09.md)).

## Goal

Give Desk a first-class workspace/project/integration-profile isolation
boundary: a new `minco-plugin-workspace` static plugin owning workspace,
project and integration-profile definitions, ownership relationships, bounded
identifiers, and scope/grant resolution; ticketing consuming a
transport-neutral resolved scope with full enforcement in use cases and bound
persistence; migrations that provision one stable deployment-bound workspace
identity under a separate ledger while preserving every existing project
identity; and integration profiles that are persisted, configuration-seeded,
and fail-closed.

Enforcement happens now, inside use cases. This task does not defer
workspace/project enforcement to a later authorization layer, and it never
treats origin matching as authentication. Effective authority is the
intersection of the authenticated principal's grants, the integration
profile's ceiling, the resolved workspace/project scope, and the existing
ticket-specific rules — never a union.

## Acceptance

The acceptance contract uses ISO identifiers (distinct from PR #187's settled
ACs). Required evidence per criterion:

- **ISO-1 — Deployment binding.** Fresh provisioning and repeated startup
  preserve one workspace identity. Conflicting configuration, a second
  workspace, and concurrent conflicting provisioning fail closed. An existing
  database cannot be silently rebound.
- **ISO-2 — Verified caller/profile binding.** Forged headers, request-body
  scope, wrong-profile credentials, ambiguous matches, disabled profiles,
  unsupported kinds, and disallowed origins cannot grant access. Existing
  valid browser/service/bootstrap paths remain functional.
- **ISO-3 — Application and repository isolation.** Direct use-case and
  real-SQLite tests cover reads, writes, lists/counts, child records,
  revision checks, and receipt recovery. Use two databases/workspaces with
  deliberately identical local IDs, plus two projects within one workspace.
  Cross-scope attempts disclose no foreign data and produce no business
  mutation. A silo-only "database A contains no B rows" test is not
  sufficient — deliberately exercise mismatched contexts and colliding
  identifiers against the real bound services.
- **ISO-4 — Durable and indirect boundaries.** Sessions, handoffs, queued
  work, activity/audit dispatch, external references, attachment access, and
  retention operations remain bound to their authoritative scope. Misrouted
  execution cannot adopt the worker's default scope. Legacy-artifact behavior
  is explicit and tested.
- **ISO-5 — Upgrade integrity.** Upgrade a populated database produced by
  the merged tree. Preserve existing project identities, data, cascades, and
  published migration checksums. Verify ledger separation, constraints,
  foreign-key checks, restart behavior, and migration
  interruption/recovery.
- **ISO-6 — Neutrality and compatibility.** Product scan,
  dependency/contract checks, neutral fixtures (two fictional
  integrations/projects), supported feature combinations,
  standalone-consumer behavior, and applicable public-API compatibility
  gates pass. No new provider contact or hidden infrastructure.
- **ISO-7 — Exact-candidate qualification.** Preflight converges; the
  complete release controller exits 0 against the frozen final candidate;
  source/tree identity and commands are recorded. Post-run
  implementation/configuration/migration/gate changes require
  requalification. An evidence-only rebind is disclosed separately and is
  not described as execution at a different SHA.

## Non-goals

- membership-management APIs, invitations, configurable roles, group
  policies, cross-project collaboration, or a general authorization engine;
- management UI/APIs, generalized RBAC, workspace transfer, pooled
  cross-workspace storage, or live profile management;
- native authentication flows, OIDC/PKCE, device sessions, mobile change
  feed/sync, push/deep links/uploads, or mobile-readiness claims;
- PeoplePlanner/MSS product integration or product-specific branches; and
- implementing adapters for reserved integration kinds — reserved kinds fail
  closed rather than falling through to the service bearer path.

## Checklist

- [ ] Record the exact starting state (checkout, `origin/main`, workspace,
  toolchain) before edits.
- [ ] Ownership inventory: name every Desk business/security persistence
  surface (ticketing rows, sessions, handoffs, receipts, jobs, activity/audit
  records, attachment access, external references) and its scope-binding
  mechanism; shared plugins use existing server-written attributes or
  namespace contracts without breaking unrelated consumers.
- [ ] Workspace plugin: descriptor, typed services, explicit configuration,
  capabilities/dependencies, health/resource/cost behavior, tests
  (`cargo minco plugin new`/`validate`); transport-neutral scope types; silo
  restriction as deployment policy, not a domain invariant.
- [ ] Migrations and provisioning: separate fixed non-colliding ledger;
  atomic, repeatable, fail-closed provisioning of one stable
  deployment-specific workspace ID; register existing distinct project IDs
  verbatim; composite foreign keys via documented SQLite create-copy-drop-
  rename rebuilds; legacy-artifact binding/quarantine; writer quiescence and
  old-binary policy documented.
- [ ] Composition scope resolution: explicit route inventory (business /
  session-handoff exchange / public-asset-preflight-probe); profile-bound
  credential verification with concrete authentication modes; exact parsed
  origin matching, missing/`null`-Origin and trusted-proxy policies; CSRF
  preserved for cookie mutations; immutable checked caller context;
  resource-type allowlists enforced at consumption; profiles persist secret
  references, never raw credentials; idempotent seeding with explicit
  reconciliation and no silent broadening.
- [ ] Ticketing scoped consumption: every exposed operation runs against a
  resolved workspace/project scope with existing action/ownership
  authorization preserved; scope-constrained reads, writes, lists, counts,
  searches, pagination, revisions, and child operations; scope-bound handles
  for the jobs worker, activity/audit dispatch, and retention erasure;
  bounded execution envelopes revalidated before execution.
- [ ] Compatibility: additive scoped constructors/facades; no legacy API
  escape hatch from isolated mode; standalone-consumer behavior verified.
- [ ] Neutrality: intentional-token static scan over the new plugin,
  Ticketing/Desk source, migrations, and contract surfaces; dependency/
  contract boundary inspection; neutral behavioral fixtures.
- [ ] ISO-1…ISO-7 evidence (including the two-workspace colliding-ID and
  populated-database upgrade proofs).
- [ ] Full quality, plugin validation, and local release qualification at
  the final head; record exact commands and results; draft PR and candidate
  review loop.

## Evidence

### Exact starting state

On 2026-09-07, before task edits: `origin/main` was
`d79399108eb802ca2268b1306661dc781a327e3c` (merge of PR #187); the local
stale `main` bookmark (`ee884379`) was fast-forwarded to `origin/main` and
this dedicated JJ workspace `/Users/c/Projects/minco-task-m15-t01` was
created on top via `./scripts/jj/task-start.sh M15-T01` and rebased directly
onto `main`. The design had been settled the same day by external review
(archived verbatim at
`docs/research/desk-isolation-design-review-2026-09.md`); ADR-0076, the M15
milestone, and this task are part of the starting commit. `M14-T74` is
marked complete in the same commit as bookkeeping: PR #187 merged at the
above SHA with all review rounds closed, so its merge gate is discharged —
`M14` itself remains the active post-1.0 release train.
