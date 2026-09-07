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
  (Partial, 2026-09-07: `workspace_isolation`/`workspace_id` config with
  fail-closed validation; scope enforcement folded into the shared
  authorize boundary (all 40 permission-checked use cases); the HTTP
  conversion preserves principal scope tokens instead of dropping them;
  requester sessions are stamped with the deployment's canonical scope;
  receipt recovery is scope-checked and foreign-project receipts never
  cross; misrouted job commands fail permanently under isolation; the
  desk enables isolation with the provisioned workspace id. Remaining:
  project parameters on the mark-published store methods and the
  route-class inventory documentation, landing with the ISO evidence.)
  (2026-09-07, completed: `mark_activity_published`/`mark_audit_published`
  are project-bound at the port, the memory store, and the SQLite
  statement (a foreign project's intent id updates nothing — proven at
  the adapter level); the route-class inventory is documented in the
  desk crate docs.)
- [ ] Compatibility: additive scoped constructors/facades; no legacy API
  escape hatch from isolated mode; standalone-consumer behavior verified.
- [ ] Neutrality: intentional-token static scan over the new plugin,
  Ticketing/Desk source, migrations, and contract surfaces; dependency/
  contract boundary inspection; neutral behavioral fixtures.
  (Partial, 2026-09-07: the intentional-token gate and the neutral
  two-fixture proof are implemented in
  `examples/minco-desk/tests/isolation_proofs.rs`; the dependency/
  contract boundary holds by construction — the workspace plugin pulls
  no product adapter/schema/role vocabulary, and ticketing's new
  dependency on it is scope types only.)
- [ ] ISO-1…ISO-7 evidence (including the two-workspace colliding-ID and
  populated-database upgrade proofs).
  (Partial, 2026-09-07: ISO-1 desk proofs, the ISO-2/3 enforcement and
  colliding-identifier proofs (two desks, same project id, credential,
  requester and ticket subject — each sees only its own rows and the
  foreign ticket 404s), the ISO-5 populated-database upgrade proof
  (pre-isolation stack migrators + a real legacy writer; published
  ticketing checksums byte-identical before/after; the workspace
  registry arrives under its own ledger; the legacy project registers
  verbatim and the legacy ticket serves through the isolated stack), and
  the ISO-6 neutrality gate all pass in `isolation_proofs.rs`. ISO-7 is
  the final-head release qualification.)
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

### Ticketing enforcement slice (2026-09-07)

Ticketing consumes the resolved scope (ADR-0076): the HTTP
Principal→Identity conversion preserves the scope claim instead of
dropping it; `TicketingConfig.workspace_isolation`/`workspace_id` (default
off — standalone consumers keep the pre-isolation behavior, and isolation
without a bound workspace identity fails configuration validation) gate a
`require_scope` check folded into the shared authorize boundary, so every
permission-checked use case — agent, requester, ingest, integrate,
ai-context, console — denies scopeless, ambiguous, and foreign-scoped
callers with `ticketing_scope_denied` (HTTP 403); requester sessions
rebuilt from validated bindings carry the deployment's canonical scope
tokens; receipt recovery is scope-checked and a foreign-project receipt
never crosses; the two Identity-bypassing job handlers fail permanently
(`ticketing.job_scope_denied`) for misrouted commands under isolation;
the desk enables isolation bound to the provisioned workspace id, and its
proofs now inject the desk's resolved scoped principal (the BFF proof
identity was also renamed off the product term to `desk-bff`). Gates:
ticketing 158 (was 154; +3 isolation tests +1 job-guard test), desk 20
(+1 enforcement proof: scopeless and foreign principals get 403 on
business routes while the bearer path passes), workspace plugin 27;
combined 205 passed / 0 failed; clippy and fmt clean; `plugin validate`
`[]`; `plugin doctor` passed; `source_manifest.py` (`025dd9fb…c4bb`);
`validate_static.py` ok 0/0.

### Persistence binding and ISO evidence slice (2026-09-07)

`mark_activity_published`/`mark_audit_published` are project-bound at
the port, the memory store, and the SQLite statement — a foreign
project's intent id updates nothing (adapter proof asserts the no-op and
that the intent stays pending for its own project). The route-class
inventory (business / session-handoff exchange / public-asset-probe) is
documented in the desk crate docs. New
`examples/minco-desk/tests/isolation_proofs.rs` carries the acceptance
evidence: the ISO-5 upgrade proof (pre-isolation stack: both merged-tree
migrators plus a real legacy isolation-off writer; the isolated desk
then upgrades with published ticketing checksums byte-identical, the
workspace registry arriving under its own ledger, the legacy project
registered verbatim, and the legacy ticket serving through the isolated
stack), the ISO-3 colliding-identifier proof (two desks with identical
project id, credential, requester subject and ticket subject — distinct
workspace identities, each listing exactly its own row, the foreign
ticket 404), the ISO-6 neutral two-fixture proof (two fictional
integrations in one workspace resolving distinct bounded scopes with
per-project grants), and the ISO-6 product-neutrality gate
(intentional-token scan over the workspace plugin, ticketing
source/migrations, and desk source/tests; the gate's own token list is
the single documented exception). Gates at this slice: combined 209
passed / 0 failed (ticketing 158, desk 24, workspace plugin 27); clippy
and fmt clean; `plugin validate` `[]`; `source_manifest.py` (1959 files,
`f26e7706…a8f2`); `validate_static.py` ok 0/0. Remaining for ISO-7: the
full release-controller qualification at the frozen final head.

### ISO-7 qualification status (2026-09-07, open)

Three `bash scripts/ci/local-release.sh` runs against the task tree:

1. Run 1 aborted in `quality.sh` at `generate-reference.sh --check`:
   `docs/reference/generated/plugins.md` and `schemas.md` were stale
   after the ticketing descriptor gained the isolation configuration
   fields — regenerated and committed (`02419c86`). The gate caught a
   real omission.
2. Run 2 aborted at `RELEASE-IDENTITY-004` (stale projection). Fixed by
   the disclosed evidence-only rebind chain (`e81066a1`, `61ea33b0`):
   `release-identity.json` regenerated, `source-manifest.json` rewritten
   (`86fb2bee…def4a0`), the 1.9 performance baseline's `source_tree_sha256`
   rebound to that digest, and the operational-evidence receipt
   regenerated to PASS. No new measurement, hosted run, or provider
   contact is claimed — hosted Linux performance evidence and live-AWS
   qualification remain NOT RUN (carried warnings, exactly as on the
   merged tree).
3. Run 3 passed every stage up to `scripts/test/feedback_browser.sh`
   (the static validators, receipts chain, unit suites, docs, snippets
   and shell portability all green) and aborted in the feedback widget's
   firefox browser tests on 15-second locator timeouts.

The browser failure is environmental, not a property of this tree:
the identical suite fails identically on the merged `main` tree
(`d7939910`, 20 failed / 20 passed), chromium passes 20/20 on this tree,
and firefox failure counts tracked the machine load minute by minute
(20 failures at load ~350, 3 at load ~72, 17 at load ~110) — the load
comes from iOS Simulator processes of another project on this machine
(`OKLens-CI…` device; one CoreSimulator process at ~780% CPU that
reboots itself after `xcrun simctl shutdown all`). No gate was modified
and no failure is converted into a pass: ISO-7's complete-controller
exit 0 remains OPEN until the machine is calm enough for the firefox
suite to run at parity with the merged tree's last green controller run
(M14-T74 r9).
