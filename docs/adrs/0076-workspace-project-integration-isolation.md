# ADR 0076: Workspace, project and integration-profile isolation

## Status

Accepted.

## Context

Minco Desk (ADR-0072–0075) runs the standalone private-beta composition on one
SQLite database behind one native process. Ticketing already carries a project
scope — `TicketingConfig.project_id`, `require_project(...)` checks in use
cases, project-filtered persistence queries, and composite `(project_id, id)`
database keys. What it lacks is a first-class project registry, workspace
ownership, and integration-profile binding: the deployment trusts a session
cookie or a `DESK_AGENT_TOKEN` bearer without any notion of which workspace,
project, or integration the caller acts through.

The design for closing that gap was settled by external review on 2026-09-07
(archived verbatim in
[docs/research/desk-isolation-design-review-2026-09.md](../research/desk-isolation-design-review-2026-09.md)).
The review accepts the isolation-first sequence and the new-plugin direction,
with the amendments recorded below. Two grounding corrections carry through
every decision: preserve and strengthen the existing project scope rather than
introducing a parallel identifier, and never treat origin matching as
authentication.

## Decision

1. **Placement — a new static `minco-plugin-workspace` plugin.** Identity keeps
   verified principal identity and the existing permission vocabulary, with no
   workspace persistence or ticketing policy. The workspace plugin owns
   workspace/project/profile definitions, ownership relationships, bounded
   identifiers, and scope/grant resolution services. The Desk composition root
   owns concrete credential verification, HTTP/proxy configuration, adapter
   selection, provisioning, and mapping verified transport evidence into a
   scoped application caller. Ticketing keeps ticket-specific authorization and
   consumes the scope without depending on Desk or HTTP types.

2. **Scope types are transport-neutral.** Ticketing accepts a
   `ResolvedProjectScope`-style type or scoped caller — never
   `examples::minco_desk::DeskContext`, which remains the HTTP composition's
   wrapper. The scope model pulls in no Axum, SQLx, or credential-verification
   machinery. The one-workspace-per-deployment restriction is private-beta
   operating policy bound to this adapter/deployment, not a universal domain
   invariant; a separate scope crate is created only if the dependency graph
   proves a real need.

   **Normative scope-token format (round 1, finding 6).** The canonical
   carrier is the checked caller's scope set; tokens are
   `workspace:<payload>` and `project:<payload>` where `<payload>` is the
   identifier percent-encoded (bytes outside `A–Z a–z 0–9 - . _ ~` become
   `%XX`), so every historically valid identifier — including those
   containing whitespace — survives the whitespace-tokenized principal
   scope claim losslessly. Resolution semantics are uniform across every
   parser: exactly one token of each reserved prefix; a reserved-prefix
   token whose payload is empty, undecodable, or not a valid identifier
   denies resolution (it is never filtered away); duplicates deny;
   unrelated scope vocabulary is ignored; workspace identifiers are
   minted-form (no leading/trailing whitespace, ≤64 chars) and project
   identifiers follow the ticketing rule (visible, ≤100 chars,
   control-free). The conformance vectors are normative and live
   identically in the workspace plugin's and ticketing's test suites;
   both parsers must agree on every row.

3. **Isolation is enforced now, inside use cases — not deferred.** Every
   exposed ticketing operation runs against an explicitly resolved
   workspace/project scope, keeps its existing action/ownership
   authorization, and uses persistence that cannot escape the bound scope.
   A shared authorization helper or scoped service handle is acceptable;
   mechanical duplication is not the goal. Membership-management APIs,
   invitations, configurable roles, group policies, cross-project
   collaboration, and a general authorization engine are explicitly
   deferrable — but an explicit principal-to-workspace/project grant is
   required now: valid identity must never imply membership of whichever
   workspace the request selected.

4. **Effective authority is an intersection, never a union.** Authority is the
   intersection of the authenticated principal's grants, the integration
   profile's ceiling, the resolved workspace/project scope, and the existing
   ticket-specific rules. A profile must never union additional permissions
   into a human identity merely because its origin matched; its service
   principal represents machine-to-machine activity and never silently
   replaces the human requester. For this boundary each enabled profile binds
   to exactly one project (multiple profiles may share a project); project IDs
   in paths, query parameters, or request bodies remain untrusted selectors or
   compatibility assertions validated against the resolved scope — never a
   source of authority.

5. **Persistence and non-HTTP paths cannot escape the bound scope.** Reads,
   lists, counts, searches, pagination, mutations, revision checks, and child
   operations are all workspace/project-constrained; loading an object
   globally and checking its workspace afterwards must not become the normal
   access pattern. The jobs worker, activity/audit dispatch, and retention
   erasure run through scope-bound service handles or validated execution
   contexts. Durable work persists a bounded execution envelope whose
   ownership is validated against the bound deployment and authoritative
   records before execution — a serialized `DeskContext` is never trusted on
   deserialization. Workspace-level scope and project-level scope stay
   distinct; no empty or wildcard project ID is invented for health checks or
   workspace operations.

6. **Migrations own the workspace identity; runtime never mints it.** The
   workspace store migrates under its own explicit, fixed, non-colliding
   ledger (following the `_minco_plugin_storage_migrations` pattern); no
   established ledger is renamed and no checksum/missing-migration validation
   is weakened. Provisioning creates one stable, deployment-specific workspace
   ID, persisted and bound to the database — "Default workspace" is a display
   name, not a shared authority identifier — atomically and repeatably, and
   fails closed on conflicting configuration, a second workspace, or an
   inconsistent existing binding. Restarts and restores preserve identity.
   Environment-specific workspace IDs are never templated into published SQL;
   an explicit provisioning/backfill phase and persisted binding data carry
   them, and the bootstrap order is specified in this ADR's task. Existing
   distinct project IDs are registered under the provisioned workspace with
   identifier spelling and case preserved — never collapsed into
   `DESK_PROJECT_ID`, never trimmed, lowercased, or merged by a stricter
   parser. Relational ownership uses composite foreign keys: a project
   belongs to its workspace; a ticket to its workspace/project pair; each
   child to the same workspace/project/ticket as its parent. SQLite table
   rebuilds follow the documented create-copy-drop-rename procedure,
   preserving indexes, triggers, uniqueness, and cascades; foreign-key
   enforcement is connection-specific, so enforcement is verified on multiple
   pooled connections against the real migration arrangement. Backfill is a
   migration action: after migration, isolated-mode writes supply or derive
   scope through the trusted bound adapter, never an ambient fallback. Legacy
   artifacts — sessions, handoffs, operation receipts, pending jobs, and other
   durable security artifacts — are deterministically bound where ownership
   can be established and invalidated or quarantinated where it cannot, never
   silently reinterpreted against the current request. Writer quiescence
   during migration and the treatment of older binaries against the new
   schema are documented explicitly.

7. **Integration profiles are persisted, config-seeded, and fail-closed.**
   Profiles are persisted entities with explicit configuration-seeded
   provisioning, runtime enforcement, and no management APIs. `kind` is
   classification, never the credential verifier: each enabled profile
   identifies its concrete supported authentication mode, with issuer/audience
   requirements only for token-based modes and redirect policy only for flows
   that redirect. The existing `VerifiedClaims` contract is preserved — claims
   arrive only after signature, issuer, audience, expiry, and transport checks
   have succeeded; the type's name or successful deserialization is never
   verification evidence. Resolution follows the fixed sequence: trusted
   deployment/route configuration identifies eligible profiles → the
   applicable credential verifier authenticates the caller → the
   credential/session binding selects or confirms the profile → profile
   ownership and grants resolve scope → host/origin restrictions further
   constrain the request. Host, Origin, and requested profile IDs never
   independently grant authority or select a weaker mode. Origin policy uses
   exact parsed matching, a defined missing/`null`-Origin policy, and an
   explicit trusted-proxy policy; CSRF protection is preserved for
   cookie-authenticated mutations; Origin is not required on ordinary
   navigation and an Origin-less caller is never classified as a privileged
   service. Ambiguous profile matches and conflicting credential modes are
   rejected outright — never retried in an order permitting weaker fallback.
   Resolution produces an immutable, checked caller context, and resource-type
   allowlists are enforced where the application accepts or consumes a
   resource reference, not merely during resolution. Profile-local external
   identities bind their deduplication/reference keys to the owning profile —
   two profiles presenting the same external ID never share authority — while
   tickets are not made exclusively owned by their creating profile:
   collaboration between authorized profiles in one project remains possible
   where explicitly permitted. Profiles persist policy, ownership, status, and
   secret references — never raw bearer credentials. Repeated provisioning is
   idempotent; conflicting ownership or policy demands explicit
   reconciliation; startup never silently broadens permissions, recreates
   deleted profiles, or re-enables disabled ones. A validated startup snapshot
   with explicit reconciliation/restart on change is sufficient — no live
   profile-management subsystem. Only mechanisms actually supported and
   exercised are enabled (including the existing Desk entry points that
   remain available); reserved kinds fail closed rather than falling through
   to the service bearer path. The route inventory distinguishes business
   routes (resolved scoped caller required), session/handoff exchange routes
   (own bound grant and bootstrap policy — never the session they are
   creating), and public assets/preflight/operational probes (explicit public
   or deployment-operational policies, never fabricated authenticated
   identities).

8. **Product neutrality is proven by three complementary checks, not a scan
   alone.** The intentional-token static scan (banned product terms over the
   workspace plugin, relevant Ticketing/Desk source, migrations, and contract
   surfaces, with narrowly documented exceptions) is one guardrail. It is
   paired with a dependency/contract boundary inspection — Workspace and
   Ticketing must not depend on a product adapter, product schema,
   product-specific role vocabulary, or product-specific defaults — and with
   neutral behavioral fixtures exercising the same scope and ticketing
   behavior under two differently configured fictional integrations/projects.
   Static validation is not compiler verification; the distinction is recorded
   in the evidence.

9. **Compatibility and bounded runtime cost are design constraints.**
   Mandatory parameters, new required trait methods, and fields added to
   public structs respect existing consumers through additive scoped
   constructors/facades and bound adapters; a legacy API must not become an
   escape hatch from isolated mode. The boundary adds no per-request remote
   discovery, implicit credential-provider calls, schedulers, or
   infrastructure; profiles and deployment binding are validated once and
   served through explicit local services.

10. **Out of scope for this boundary.** Management UI/APIs, generalized RBAC,
    cross-project sharing, workspace transfer, pooled cross-workspace storage,
    new native authentication flows, and adapter implementations for reserved
    kinds. Later client foundations — native authentication/device sessions,
    change feed/sync, mobile delivery, and separately authorized product
    integrations — are separately scoped tasks and are not delivered or
    claimed by this boundary.

## Consequences

- The boundary is delivered as milestone M15 ("Desk isolation and client
  foundations"), task M15-T01; M15 depends on completed prerequisites, not on
  completion of the open-ended M14 release train, and M14 remains active.
- Qualification follows the ISO-1…ISO-7 acceptance contract recorded in
  `tasks/M15/M15-T01-desk-isolation-foundation.md`: deployment binding,
  verified caller/profile binding, application and repository isolation with
  deliberately colliding identifiers, durable and indirect boundaries,
  upgrade integrity from a populated pre-isolation database, neutrality and
  compatibility, and exact-candidate qualification.
- The silo restriction remains liftable: dismantling private beta does not
  require dismantling the plugin, because single-workspace enforcement lives
  in deployment binding rather than domain invariants.
- Published migration checksums and ledger identities are preserved; the
  upgrade path from the merged desk tree is part of the deliverable, not an
  afterthought.
