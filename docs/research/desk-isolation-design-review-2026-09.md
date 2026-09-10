# Desk isolation design review — settled 2026-09-07

Verbatim reviewer reply to the ADR-0076 / M15-T01 design proposal
(six questions, answered 2026-09-07; relayed manually). This document is the
authoritative record of the accepted design and its amendments; ADR-0076 and
the M15-T01 acceptance contract derive from it.

---

## Design disposition

**Accept the isolation-first sequence and the new-plugin direction, with the amendments below. Do not defer workspace/project enforcement inside use cases, and do not treat origin matching as authentication.** With those corrections, this is a suitable bounded PR before native authentication, mobile sync, or product integrations.

I confirmed that PR #187 is merged and inspected `main` at **`d79399108eb802ca2268b1306661dc781a327e3c`**. This is a design review, not a requalification of the merged release.

One important grounding correction: **Ticketing already has a project scope.** `TicketingConfig.project_id`, use-case `require_project(...)` checks, project-filtered persistence queries, and composite `(project_id, id)` database keys already exist. What is missing is a first-class project registry, workspace ownership, and integration-profile binding—not the project identifier itself. Preserve and strengthen the existing scope rather than introducing a parallel identifier.

## Q1. Placement — accept a new `minco-plugin-workspace`

**Yes. Prefer the new plugin over extending Identity or introducing only an uncomposed utility crate.**

The responsibility split should be:

| Component        | Responsibility                                                                                                                                                         |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Identity         | Verified principal identity and existing permission vocabulary. No workspace persistence or ticketing policy.                                                          |
| Workspace plugin | Workspace/project/profile definitions, ownership relationships, bounded identifiers, and scope/grant resolution services.                                              |
| Desk composition | Concrete credential verification, HTTP/proxy configuration, adapter selection, provisioning, and mapping verified transport evidence into a scoped application caller. |
| Ticketing        | Ticket-specific authorization and behavior, consuming that scope without depending on Desk or HTTP types.                                                              |

This fits the repository’s explicit plugin-selection and typed-service conventions. The operating contract already requires real descriptors, configuration, capabilities, health/resource/cost behavior, and tests; it also places concrete adapter selection in the composition root.

Two boundaries matter here.

**Keep the scope types transport-neutral.** Ticketing should accept a type such as `ResolvedProjectScope` or a scoped caller—not `examples::minco_desk::DeskContext`. `DeskContext` can remain the HTTP composition’s wrapper. Avoid pulling Axum, SQLx, or credential-verification machinery into the pure scope model.

**Keep the silo restriction deployment-specific.** “This adapter/deployment is bound to exactly one workspace” is the private-beta operating policy. Do not encode “only one workspace can ever exist” as a universal domain invariant that later requires dismantling the plugin.

A separate tiny scope crate is unnecessary unless the dependency graph demonstrates a real need. The new plugin can expose lightweight, transport-neutral types without creating another package immediately.

## Q2. Ticketing consumption — accept scoped consumption, reject deferred isolation

**Choose (a), but “lite” must describe the amount of policy machinery—not incomplete enforcement. Reject (b) as the isolation foundation.**

Existing use cases already combine permission checks, project checks, and requester ownership checks. For example, `create_ticket` checks permission and configured project; requester reads/replies also check requester ownership. Extend that established boundary rather than moving authorization into persistence or replacing it with middleware alone.

### The PR 1 line

**Mandatory now:** every exposed ticketing operation must run against an explicitly resolved workspace/project scope, preserve its existing action/ownership authorization, and use persistence that cannot escape the bound scope. A shared authorization helper or scoped service handle is fine; duplicating checks mechanically into every function is not the goal.

**Deferrable:** membership-management APIs, invitations, configurable roles, group policies, cross-project collaboration, and a general authorization engine. However, there must already be an explicit principal-to-workspace/project grant. “Valid identity” must not automatically mean “member of whichever workspace the request selected.” OWASP similarly distinguishes authentication from authorization and recommends deny-by-default, per-request authorization. ([OWASP Cheat Sheet Series][1])

I would establish the following application contract:

> Effective authority is the intersection of the authenticated principal’s grants, the integration profile’s ceiling, the resolved workspace/project scope, and the existing ticket-specific authorization rules.

A profile must **never union additional permissions into a human identity** merely because its origin matched. Its service principal represents actual machine-to-machine activity; it must not silently replace the human requester.

For PR 1, **binding each enabled profile to one project is a reasonable simplification**. Multiple profiles may share a project. Arbitrary project switching is unnecessary. Existing project IDs in paths, query parameters, or request bodies remain untrusted selectors or compatibility assertions: validate them against the resolved scope; never use them to mint authority.

### Persistence and non-HTTP coverage

Require workspace/project constraints for reads, lists, counts, searches, pagination, mutations, revision checks, and child operations. Loading an object globally and checking its workspace afterward should not become the normal access pattern.

Also cover workers and operator entry points. The merged composition has an explicit jobs worker, activity/audit dispatch, and retention erasure, all outside ordinary request middleware. Those paths need scope-bound service handles or validated execution contexts too.

Do not serialize a supposedly trusted `DeskContext` into a job and trust it on deserialization. Persist a bounded execution envelope, then validate its ownership against the bound deployment and authoritative records before execution.

Finally, distinguish **workspace-level scope** from **project-level scope**. Do not invent an empty or wildcard project ID for health checks or workspace operations.

## Q3. Migrations — accept forward-only backfill, with several concrete traps

**Yes, add new migrations. No, a permanent `DEFAULT 'default-workspace'` is not an adequate final design.**

### Preserve published migration history and ledger ownership

Ticketing currently uses a standard SQLx `Migrator` without a custom ledger name. Shared plugin storage explicitly uses `_minco_plugin_storage_migrations`. A new workspace migrator must not independently start another `0001` history in Ticketing’s ledger. Give the new stream an explicit, fixed, non-colliding ledger, following the existing pattern. Do not rename either established ledger or disable checksum/missing-migration validation to make composition pass.

SQLx documents `_sqlx_migrations` as the default and specifically warns that changing an existing production ledger can cause already-applied migrations to run again. ([Docs.rs][2])

### Provision an identity, not a shared label

Provision one **stable, deployment-specific workspace ID**, persist it, and bind the database to it. “Default workspace” can be its display name; it should not mean every deployment receives the same authority identifier.

Provisioning must be atomic and repeatable. A conflicting configured workspace, second workspace, or inconsistent existing binding must fail closed—not rewrite ownership. Restarting or restoring the database must preserve its identity rather than generate a new one.

Specify the migration/bootstrap order in the ADR. Do not template environment-specific workspace IDs into published SQL migration files. Use persisted binding data and an explicit provisioning/backfill phase.

### Preserve existing projects and relational ownership

The existing schema has project-qualified parent and child keys, including tickets, messages, attachments, references, external messages, and activity intents.

Register the existing distinct project IDs under the provisioned workspace; **do not collapse them into the current `DESK_PROJECT_ID`**. Preserve identifier spelling and case. A newly restricted `ProjectId` parser must not silently trim, lowercase, truncate, or merge historically valid IDs.

The resulting relational contract should establish:

* A project belongs to its workspace.
* A ticket belongs to that workspace/project pair.
* Each child belongs to the same workspace/project/ticket as its parent.

Prefer composite foreign keys for these relationships. Adding independent workspace and project foreign keys would not, by itself, prove that they belong together.

### Account for SQLite’s actual migration mechanics

SQLite does not permit the straightforward combination of adding a non-null, non-null-default column with `REFERENCES` while foreign keys are enabled. A table rebuild may therefore be necessary to reach the intended final constraints. Preserve indexes, triggers, uniqueness, and cascades; follow the documented create-copy-drop-rename procedure rather than renaming the old parent first. ([SQLite][3])

Foreign-key enforcement is connection-specific, and changing `PRAGMA foreign_keys` inside an active transaction has no effect. Test the actual SQLx migration transaction arrangement and verify enforcement on multiple pooled connections—not merely the presence of a pragma in a migration file. ([SQLite][4])

**Backfill is a migration action; missing scope must not become normal runtime authority.** After migration, isolated-mode writes must supply or derive scope through the trusted, bound adapter, never an ambient fallback.

### Define legacy-artifact treatment

Include sessions, handoffs, operation receipts, pending jobs, and other durable security artifacts in the upgrade inventory. Deterministically bind artifacts whose ownership can be established. Invalidate or quarantine ambiguous artifacts according to their semantics; do not silently reinterpret them using the current request’s host or project.

Document writer quiescence during migration and whether older binaries are refused afterward. Forward-only migration does not imply that an old writer remains safe against the new schema.

## Q4. Integration profiles — accept persisted + seeded, but strengthen the contract

**Agree with persisted entities, explicit configuration-seeded provisioning, runtime enforcement, and no management APIs.** Origin/kind checks alone are insufficient.

### Separate profile kind from authentication mechanism

Your kind list mixes client/channel categories with authentication approaches. That is acceptable as classification, but **`kind` must not be the credential verifier**.

An enabled profile must identify its concrete supported authentication mode. Issuer/audience requirements apply to token-based modes; they should not become meaningless mandatory strings on cookie or email profiles. Likewise, redirect policy belongs to flows that actually redirect.

Also, the existing `VerifiedClaims` contract says claims arrive only after **signature, issuer, audience, expiry, and other transport checks** have succeeded—not merely after a valid signature. Preserve that contract. The type’s name or successful deserialization is not verification evidence.  JWT guidance likewise requires issuer/subject and audience validation and protection against tokens being substituted across purposes. ([RFC Editor][5])

### Bind credentials to profiles; use origins as restrictions

The resolution sequence should be:

> Trusted deployment/route configuration identifies eligible profiles → the applicable credential verifier authenticates the caller → the credential/session binding selects or confirms the profile → profile ownership and grants resolve scope → host/origin restrictions further constrain the request.

Host, Origin, and a requested profile ID must not independently grant authority or select a weaker authentication mode.

Use exact parsed origin matching, a defined missing/`null`-Origin policy, and an explicit trusted-proxy policy. Preserve CSRF protection for cookie-authenticated mutations. Do not require `Origin` on every legitimate navigation, and do not classify an Origin-less caller as a privileged service. OWASP’s guidance specifically addresses exact origin comparison, absent headers, and configured target origins behind proxies. ([OWASP Cheat Sheet Series][6])

Reject ambiguous profile matches and conflicting credential modes rather than trying them in an order that permits a weaker fallback.

### Enforce policy where the action is known

Resolution should establish an immutable, checked caller context. **Resource-type allowlists must also be enforced when the application accepts or consumes a resource reference**, not merely loaded during resolution.

For profile-local external identities, bind the relevant deduplication/reference keys to the owning profile. Two profiles presenting the same external ID must not accidentally share authority. Conversely, do not make every ticket exclusively owned by its creating profile: collaboration between authorized profiles in the same project should remain possible where explicitly permitted.

### Make seed and lifecycle behavior deterministic

Persist policy, ownership, status, and secret references—not raw bearer credentials in serializable profile records. Repeated provisioning must be idempotent. Conflicting ownership or policy must produce an explicit reconciliation requirement; startup must not silently broaden permissions, recreate deleted profiles, or re-enable disabled ones.

For this bounded PR, a validated startup snapshot with changes requiring explicit reconciliation/restart is sufficient. A live profile-management subsystem is unnecessary.

**Do not implement all eleven adapters.** ADR-0076 may reserve the taxonomy. Enable only the mechanisms actually supported and exercised by this PR, including the existing Desk entry points that remain available. Unsupported kinds must fail closed; they must not fall through to the service bearer path.

### Correct “every HTTP path”

Use an explicit route inventory:

**Business routes** require a resolved scoped caller. **Session/handoff exchange routes** validate their own bound grant and bootstrap policy; they cannot require the session they are creating. **Public assets, preflight, and operational probes** have explicit public or deployment-operational policies, never fabricated authenticated identities.

That avoids both an authentication bypass and a bootstrap deadlock.

## Q5. Product-neutrality gate — useful, but not sufficient alone

**Accept the static scan as one guardrail, not as the complete proof.**

For the narrow claim “these named product types are absent,” it is useful. It cannot establish that the design has not encoded the same product assumptions under generic names.

Add two inexpensive checks alongside it.

**Dependency/contract boundary:** inspect the relevant Cargo dependency graph and public domain/application contracts. Workspace and Ticketing must not depend on a product adapter, product schema, product-specific role vocabulary, or product-specific defaults.

**Neutral behavioral fixtures:** exercise the same scope and ticketing behavior with two differently configured, fictional integrations/projects. Differences should be supplied as configuration or generic external references, not through product-specific branches.

The scanner should cover the new workspace plugin, relevant Ticketing/Desk source, migrations, and contract surfaces. Use intentional token matching rather than arbitrary substring matching; keep narrowly documented exceptions for the gate’s own tests or historical prose.

The repository already distinguishes executable architecture boundaries from static checks and explicitly warns that static validation is not compiler verification. Apply the same distinction here.

## Q6. Roadmap — accept M15, with a narrower delivered outcome

**Yes: M15 and M15-T01 are appropriate.** I would prefer the milestone name **“Desk isolation and client foundations”**, although your proposed name is acceptable provided “mobile readiness” is not reported as delivered by this PR.

Make **M15-T01** the bounded isolation-foundation task, with ADR-0076, implementation, upgrade behavior, proofs, and qualification as its deliverables. A separate ADR task is optional; it should not obscure responsibility for the complete boundary.

Keep later tasks separately scoped and uncompleted: native authentication/device sessions, then change feed/sync, then mobile delivery concerns, followed by separately authorized product integrations.

The actual roadmap file is `roadmap/roadmap.yaml`, and M14 is currently an **active post-1.0 release train**. Do not mark it complete merely because #187 merged. Check dependency semantics before making M15 wait for completion of an open-ended release-train milestone; use the actual completed prerequisite tasks where appropriate.

## Scope adjustments I would make

**Add an ownership inventory, not a generic platform rewrite.** Name every Desk business/security persistence surface and its scope-binding mechanism: ticketing rows, sessions, handoffs, receipts, jobs, activity/audit records, attachment access, and external references. Shared plugins can use existing server-written attributes or namespace contracts where that avoids breaking unrelated consumers, but those bindings must be checked. Global migration ledgers do not need fictional tenant ownership.

**Add compatibility as a design constraint.** Mandatory parameters, new required trait methods, and fields added to public structs can affect existing consumers. Prefer additive scoped constructors/facades and bound adapters where necessary. A legacy API must not become an escape hatch from isolated mode. The merged roadmap and operating contract retain explicit public-API compatibility obligations.

**Keep runtime cost bounded.** No per-request remote discovery, implicit credential-provider calls, new schedulers, or extra infrastructure. Profiles and deployment binding can be validated once and served through explicit local services.

**Cut** management UI/APIs, generalized RBAC, cross-project sharing, workspace transfer, pooled storage, new native authentication flows, and adapter implementations for reserved kinds. None is needed to establish this boundary.

## Acceptance contract for the next candidate

I suggest using distinct **ISO** identifiers so these are not confused with PR #187’s already-settled ACs.

| ID                                               | Required evidence                                                                                                                                                                                                                                                                                                                                |
| ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **ISO-1 — Deployment binding**                   | Fresh provisioning and repeated startup preserve one workspace identity. Conflicting configuration, a second workspace, and concurrent conflicting provisioning fail closed. An existing database cannot be silently rebound.                                                                                                                    |
| **ISO-2 — Verified caller/profile binding**      | Forged headers, request-body scope, wrong-profile credentials, ambiguous matches, disabled profiles, unsupported kinds, and disallowed origins cannot grant access. Existing valid browser/service/bootstrap paths remain functional.                                                                                                            |
| **ISO-3 — Application and repository isolation** | Direct use-case and real-SQLite tests cover reads, writes, lists/counts, child records, revision checks, and receipt recovery. Use two databases/workspaces with deliberately identical local IDs, plus two projects within one workspace. Cross-scope attempts disclose no foreign data and produce no business mutation.                       |
| **ISO-4 — Durable and indirect boundaries**      | Sessions, handoffs, queued work, activity/audit dispatch, external references, attachment access, and retention operations remain bound to their authoritative scope. Misrouted execution cannot adopt the worker’s default scope. Legacy-artifact behavior is explicit and tested.                                                              |
| **ISO-5 — Upgrade integrity**                    | Upgrade a populated database produced by the merged tree. Preserve existing project identities, data, cascades, and published migration checksums. Verify ledger separation, constraints, foreign-key checks, restart behavior, and migration interruption/recovery.                                                                             |
| **ISO-6 — Neutrality and compatibility**         | Product scan, dependency/contract checks, neutral fixtures, supported feature combinations, standalone-consumer behavior, and applicable public-API compatibility gates pass. No new provider contact or hidden infrastructure is introduced.                                                                                                    |
| **ISO-7 — Exact-candidate qualification**        | Preflight converges; the complete release controller exits **0** against the frozen final candidate; source/tree identity and commands are recorded. Post-run implementation/configuration/migration/gate changes require requalification. An evidence-only rebind is disclosed separately and is not described as execution at a different SHA. |

A silo-only test that merely proves “database A contains no B rows” is not enough for ISO-3. Deliberately exercise mismatched contexts and colliding identifiers against the real bound services.

**Bottom line:** proceed with the new plugin, persisted profiles, and M15-T01. The settled design should deliver a **fully enforced but deliberately simple isolation boundary**—not workspace columns now and authorization later.

[1]: https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html "https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html"
[2]: https://docs.rs/sqlx/latest/sqlx/migrate/struct.Migrator.html "https://docs.rs/sqlx/latest/sqlx/migrate/struct.Migrator.html"
[3]: https://www.sqlite.org/lang_altertable.html "https://www.sqlite.org/lang_altertable.html"
[4]: https://sqlite.org/foreignkeys.html "https://sqlite.org/foreignkeys.html"
[5]: https://www.rfc-editor.org/rfc/rfc8725.html "https://www.rfc-editor.org/rfc/rfc8725.html"
[6]: https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html "https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html"
