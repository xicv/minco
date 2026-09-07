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
  - docs/reference/generated/schemas.md
  - roadmap/roadmap.yaml
  - tasks/M14/M14-T74-desk-stabilization-merge-gate.md
  - tasks/M15/M15-T01-desk-isolation-foundation.md
  - crates/minco/Cargo.toml
  - crates/minco/src/lib.rs
  - Cargo.toml
  - Cargo.lock
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

- [x] Record the exact starting state (checkout, `origin/main`, workspace,
  toolchain) before edits.
- [x] Ownership inventory: name every Desk business/security persistence
  surface (ticketing rows, sessions, handoffs, receipts, jobs, activity/audit
  records, attachment access, external references) and its scope-binding
  mechanism; shared plugins use existing server-written attributes or
  namespace contracts without breaking unrelated consumers.
- [x] Workspace plugin: descriptor, typed services, explicit configuration,
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
  (Partial, 2026-09-07: startup provisioning, the scope-carrying bearer
  principal, the scoped worker principal, the workspace ledger in the
  bootstrap order, and the critical workspace health check are wired into
  the desk; the remaining items — route-class inventory documentation,
  origin/CSRF refinements, resource-type consumption enforcement — land
  with the ticketing slice.)
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

### Ownership inventory (2026-09-07)

Every Desk business/security persistence surface and its current
scope-binding mechanism, from the merged tree at `d7939910`:

| Surface | Persistence | Scope binding today |
| --- | --- | --- |
| Tickets/messages/attachments/followers/tags/source refs/resource refs | `ticketing_*` tables | composite `(project_id, id)` keys, FK cascades to `ticketing_tickets(project_id, id)` |
| Handoffs | `ticketing_handoffs` | `digest` PK; `project_id` column, **no FK**; `portal_origin` checked at exchange |
| Requester sessions | `minco_sessions` | project only inside JSON attributes (`ticketing.project`), enforced at `resolve_requester_session` |
| Session exchange grants | `ticketing_session_exchange_grants` | `exchange_key` PK; `project_id` column, no FK |
| Operation receipts | `ticketing_operation_receipts` | `idempotency_key` PK; `project_id`/`subject_digest` scope columns from 0017, not enforced at the port |
| Queued jobs | `minco_jobs` (+ publications/locks) | **no project column**; scope only in envelope `partition = project_id` |
| Activity/audit intents | `ticketing_activity_intents` | `id` PK; `project_id` column; dispatched by the worker with the deployment project |
| Audit records | `minco_audit` | no project column (append-only evidence) |
| Delivery evidence / send intents | `ticketing_delivery_evidence` / `ticketing_send_intents` | composite keys / `logical_send_id` PK with `project_id` column |
| External identities | `ticketing_external_messages` | dedupe key `(project_id, provider, mailbox_scope, external_id)` — profile-locality not yet bound |
| Attachment bytes | object store (memory in desk) | keys derived per ticket |
| Retention erasure | `erase_tickets_resolved_before(project_id, …)` | project-scoped at the port; no service-level use case yet |

Trust boundary today: bearer `DESK_AGENT_TOKEN` (constant-time compare,
injects `Principal { subject: "desk-agent", … }`) and the requester session
cookie (`minco_ticketing_session`, CSRF-bound); development headers are
never trusted; `Identity.scopes` is never consulted by ticketing. Store
methods that load globally (no project parameter): receipts, session
exchange grants, send intents, `mark_*_published`. Job handlers
`run_development_automation` and `deliver_public_notification` bypass
`Identity` entirely; only `process_inbound_email` authorizes through the
worker principal. These are the concrete enforcement targets for the
composition and ticketing slices.

### Workspace plugin slice (2026-09-07)

`plugins/minco-plugin-workspace` v0.1.0 (experimental, unpublished): model
(bounded identifiers preserving historical project spelling; reserved
kind taxonomy with only `service_api`/`service_bearer_token`,
`portal`/`portal_session`, `email`/`inbound_email` supported), provisioning
service (atomic binding, idempotent convergence, fail-closed rebind/drift,
orphans reported not deleted), grants and scope resolution (explicit grant
required; session/project selectors validated against the registry; service
principals bounded by the persisted ceiling), SQLite persistence under the
exclusive `_minco_workspace_migrations` ledger with composite foreign keys
(profiles/grants → registered projects), and the static plugin registration
(catalog, root workspace, facade feature `plugin-workspace`,
`official-plugins`, critical `workspace-store` health check, no HTTP
module). Gates run: `cargo +1.97.1 test -p minco-plugin-workspace
--all-features --offline` (26 tests), default-feature test (22 tests),
clippy `-p minco-plugin-workspace -p minco --all-targets --all-features
--offline` clean for the new code, rustfmt clean, `cargo minco plugin
validate` (`[]`), `cargo minco plugin doctor` (`"status": "passed"`),
`scripts/docs/generate-reference.sh` (7 files; features/plugins/schemas
regenerated), `scripts/source_manifest.py` (1958 files,
`ce1b7aba…f2085`), `scripts/validate_static.py` (ok, 0/0). Note: a
pre-existing `unused async` warning surfaces in `minco-plugin-ticketing`
only under facade `--all-features`; not introduced or modified by this
task. `--locked` evidence runs after the updated `Cargo.lock` is committed.

### Desk composition slice (2026-09-07)

The desk is now an isolated deployment (ADR-0076): `migrate()` applies the
workspace ledger after plugin storage and ticketing (bootstrap order:
plugin storage → ticketing → workspace); `build_desk()` provisions the
registry before serving — registering `DESK_PROJECT_ID` verbatim, seeding
the `desk-agent` `service_api`/`service_bearer_token` profile with the
9-permission agent ceiling and the portal origin, and refusing conflicting
pins, drift, and second workspaces; the bearer middleware injects the
principal resolved once from the provisioned profile (subject, ceiling,
canonical `workspace:`/`project:` scope tokens) instead of minting one per
request, with a startup round-trip check because the principal scope claim
is whitespace-tokenized (identifiers containing whitespace fail closed
with a precise error); the mail-worker principal carries the same
validated project scope; a critical `workspace-store` health check
covers the registry; `minco-desk-migrate` probes the binding; new env
inputs `DESK_WORKSPACE_ID` (optional pin) and
`DESK_WORKSPACE_DISPLAY_NAME`. `BuiltDesk` exposes the provisioning
report and the resolved agent principal for proofs. Gates: desk suites
19/19 (including the two new ISO-1 proofs: bind-once/rebuild-convergence
with the principal-scope equality and the exclusive-ledger row count;
conflicting-pin fail-closed), workspace plugin 27/27 all-features, clippy
and fmt clean, `source_manifest.py` regenerated
(`5b099bd5…e685e`), `validate_static.py` ok 0/0. The existing
upgrade-from-v1 proof now also crosses the workspace bootstrap: a
first-generation ticketing database gains the registry with its existing
project registered verbatim.
