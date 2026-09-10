# M15-T01 candidate review — round 1 (2026-09-07)

Verbatim reviewer reply for draft PR #189 candidate c5b65977 against base
d7939910, delivered through the ego-chat binding `minco-m15-t01-review`
(workflow 07d49a76-1da5-4e95-b773-525c8084f6c2, response digest
824affd0da656f4e4fbc7509458529a4f0dfdffdd067368dbad9891b3c40fa83).
Verdict: REQUEST CHANGES — ISO-1 and ISO-7 PASS; ISO-2..ISO-6 FAIL with
severity-ranked blockers below.

---

Verdict: REQUEST CHANGES

I reviewed the candidate at c5b659779d8d4796e606ae34139f882c40e08fe2, against base d79399108eb802ca2268b1306661dc781a327e3c and the archived acceptance contract. The PR metadata matches the submitted candidate. This is a source-and-evidence review; I did not independently execute the repository's tests or release controller.

The shared authorization boundary and deployment registry are useful foundations, but the candidate does not yet establish the complete isolation boundary promised by ISO-2 through ISO-6. The blockers include unchecked session-exchange operations, incomplete profile-policy enforcement, project-only job validation, and upgrade/compatibility gaps—not merely insufficient test counts.

## ISO verdicts

| Criterion | Verdict | Evidence-based assessment |
| --- | --- | --- |
| ISO-1 — Deployment binding | PASS | The singleton binding, transactional provisioning, conflicting-pin rejection, repeat provisioning, and concurrent-conflict tests support the deployment-identity requirement. This does not extend to profile-policy integrity, which fails separately below. |
| ISO-2 — Verified caller/profile binding | FAIL | Persisted policy drift can evade reconciliation; deleted profiles are recreated; profile origin/resource restrictions are not carried into their enforcement points; portal and email execution are not resolved through their corresponding persisted profiles. |
| ISO-3 — Application and repository isolation | FAIL | The submitted "colliding identifiers" test explicitly requires different ticket IDs and principally proves separate-database absence. The required identical-record-ID, mismatched-context, real-SQLite matrix is not established. There are also unchecked direct application boundaries described below. |
| ISO-4 — Durable and indirect boundaries | FAIL | Session-exchange rotation is globally keyed and lacks a project ownership check before revocation/minting. Job validation checks project spelling, not originating workspace ownership. Session resolution derives workspace authority from current configuration rather than a persisted artifact binding. |
| ISO-5 — Upgrade integrity | FAIL | Startup registers only the configured project, not all existing projects. The registry migration does not establish the promised ticket-to-registry relationship. Historically valid whitespace-containing project IDs cannot pass the principal carrier. The upgrade fixture is narrower than the accepted populated-merged-tree upgrade contract. |
| ISO-6 — Neutrality and compatibility | FAIL | The neutral fixture exercises workspace resolution, not ticketing behavior. Required public trait signatures changed despite the default-off compatibility claim. The neutrality scanner also exempts the entire proof file, not just its token list. |
| ISO-7 — Exact-candidate qualification | PASS on the recorded qualification, with the evidence-only qualification explained below | The task records complete-controller exit 0 at 6ba484e6. The subsequent transition consists of the task record, receipt/manifest rebinding, and an empty final commit—not post-run implementation changes. I do not interpret this as controller execution at c5b65977. |

## Severity-ranked merge blockers

### 1. P1 — Session-exchange operations can act on another project's grant

Locations: plugins/minco-plugin-ticketing/src/service.rs:2484–2620, particularly rotate_session_exchange; related exchange-record/revocation operations in the surrounding section.

rotate_session_exchange(exchange_key) loads a grant globally, checks expiry/revocation, then revokes previous or staged sessions and issues a replacement using the loaded grant's subject, project, origin, and permissions. It never first establishes that the grant belongs to this service's configured project.

A concrete direct-application counterexample is two project-bound services over the same SQLite/session stores: call project B's rotation method with project A's valid exchange key. The code proceeds to revoke and replace A's session and returns the replacement bearer. This is an application-boundary failure even without claiming that an arbitrary HTTP caller can obtain another exchange key.

The session workspace binding is also incomplete: session issuance persists project/origin/permissions/exchange key, while resolution constructs workspace scope from the receiving service's configuration. That is not a persisted workspace/profile binding, and the required legacy-artifact transition has not been demonstrated.

Required change: Scope the exchange lookup and all subsequent fenced mutations, session revocations, and recovery operations before their first effect. Establish authoritative workspace/project/profile ownership for issued artifacts, with explicit binding or invalidation of legacy artifacts. Add real-SQLite tests proving that a wrong-project service cannot rotate, revoke, replace, or recover another project's grant and leaves all affected records unchanged.

### 2. P1 — Profile reconciliation trusts a stale digest and recreates deleted profiles

Locations: plugins/minco-plugin-workspace/src/service.rs:343–449; plugins/minco-plugin-workspace/src/persistence.rs:116–137.

There are two distinct violations of the accepted lifecycle contract.

First, reconcile_profile compares the stored policy_digest against the digest of configured policy, but does not recompute the digest from the persisted policy fields. The SQLite adapter independently loads those fields and the stored digest. Consequently, changing a persisted permission ceiling or service subject while leaving its digest unchanged can pass startup reconciliation; scope resolution subsequently uses the changed persisted values.

Second, on an already-bound database, a missing configured profile follows None => profile_from_spec(...), which constructs an enabled profile. Deleting a seeded profile therefore causes startup to recreate it rather than demand reconciliation.

Required change: Validate the actual persisted policy, recompute its digest, and compare it against both its stored seal and the expected configuration. Distinguish a genuinely new configuration seed from a previously provisioned profile that was removed—for example through retained provisioning history/tombstones or an explicit reconciliation operation. Test unchanged-digest field drift, deletion followed by restart, disabled profiles, and unsupported persisted kind/mode combinations.

### 3. P1 — Integration-profile restrictions are stored but not enforced end to end

Locations: examples/minco-desk/src/lib.rs:515–560, 685–700, 964–999; plugins/minco-plugin-ticketing/src/service.rs:545–638.

The composition resolves agent_scope, but constructs the principal from only its subject, permission ceiling, and workspace/project tokens. Its allowed_origins and resource_types do not reach the bearer middleware or ticketing policy boundary. DeskAgentAuth contains only the token and principal; the middleware authenticates the bearer and continues without checking Origin.

The surrounding middleware configures CORS response behavior from DeskConfig.allowed_origins; it does not provide a rejecting profile-origin authorization check. Therefore an authenticated service request with a disallowed Origin is not rejected by the profile policy before reaching the business handler. This is a failure to enforce an additional restriction—not a claim that Origin should authenticate callers.

The same missing policy propagation affects resource-reference acceptance: create_ticket performs scope/action/project/requester checks, then constructs the ticket without consuming a resolved profile resource-type policy. The composition also provisions only the service profile; the mail worker obtains scope through validate_session_scope(project), rather than an email profile, and requester-session resolution has no persisted portal-profile binding.

Required change: Carry an immutable checked profile identity/policy snapshot to the points that enforce it. Bind the supported service, portal, and email paths to their applicable profiles; enforce origin and credential-mode policies at ingress, and resource restrictions/profile-local reference namespaces at consumption. This requires neither management APIs nor implementations of reserved adapters.

### 4. P1 — Project-only job validation cannot reject a foreign workspace with identical local IDs

Locations: plugins/minco-plugin-ticketing/src/jobs.rs:35–73, 658–701.

DeliverPublicNotification carries project, ticket, and message IDs, but no workspace binding. require_job_project checks only:

```rust
config.workspace_isolation && command_project != config.project_id
```

The notification and automation registration closures also ignore the execution context. Thus a foreign command with the same project spelling passes this guard; the receiving handler then resolves its ticket/message identifiers against the receiving store. With the deliberately colliding identifiers required by the contract, the command cannot be distinguished from local work.

The foreign-project helper test establishes only rejection of a different project string, not preservation of originating workspace authority.

Required change: Give durable execution an authoritative workspace/project binding and validate it against the deployment and owning records before execution. A versioned envelope or a rigorously bound queue/store namespace can satisfy this; a new workspace column on every shared-plugin table is not inherently required. Include legacy-command treatment and a real misrouting test with identical local IDs that proves zero notification, automation, or ticket mutation.

### 5. P1 — Upgrade provisioning does not inventory existing ownership

Locations: examples/minco-desk/src/lib.rs:515–540; plugins/minco-plugin-workspace/migrations/sqlite/0001_workspace_isolation.sql:14–55.

The provisioning input is:

```rust
projects: vec![desk_project.clone()]
```

There is no inventory of existing ticketing project IDs in this path. A populated database containing additional historical projects therefore gains a registry containing only the currently configured project.

The new migration establishes binding→projects and projects→profiles/grants relationships, but does not connect ticketing rows to registered ownership. These are useful registry constraints, but they are not the full relational contract recorded in ADR-0076.

Required change: Inventory and register every existing project identity verbatim during the explicit upgrade phase, and establish the promised ticket/project/workspace ownership constraint through the appropriate forward migration and bound persistence boundary. Cover projects represented by durable security artifacts as well as live tickets. Specify and test writer quiescence, interruption/restart, legacy-artifact handling, and enforcement on multiple pooled connections.

### 6. P1 — The scope carrier breaks historically valid project IDs; its parsers also disagree

Locations: examples/minco-desk/src/lib.rs:550–580; crates/minco-http/src/principal.rs:36–55; plugins/minco-plugin-ticketing/src/service.rs:2929–2945; plugins/minco-plugin-workspace/src/model.rs:442–492.

The project model preserves identifiers containing spaces, but Principal::with_scopes drops every token containing ASCII whitespace. A valid historical project such as `legacy Prj` therefore loses its project token and fails Desk startup's decoding check. The implementation is fail-closed, but it does not preserve the accepted legacy deployment behavior.

Separately, the duplicate parser is not behaviorally equivalent. For example:

```
workspace:
workspace:ws-a
project:desk
```

Ticketing discards the empty value and accepts the remaining pair. The workspace parser counts both reserved-prefix tokens and rejects the input as ambiguous. Matching prefix constants plus one successful end-to-end case does not prove fail-closed equivalence.

Required change: Use a lossless carrier/encoding for every historically valid identifier, without trimming or renaming existing projects. Require round-trip equality and a common normative conformance suite for both parsers: empty reserved tokens, conflicting tokens, invalid identifiers, whitespace, and unrelated scopes. Reject malformed reserved tokens rather than filtering them away.

### 7. P2 — The acceptance proofs omit the cases most likely to expose these gaps

Locations: examples/minco-desk/tests/isolation_proofs.rs:47–285, 289–426, 429–481.

The collision test contains `assert_ne!(ticket_a, ticket_b)`. It then uses each desk's own principal and asks B for A's absent UUID. That is precisely the weaker separate-database absence proof excluded by the acceptance contract; matching ticket subjects do not substitute for matching record identifiers.

The upgrade fixture uses the candidate's service with isolation disabled and candidate-linked migrators. It writes one current-project ticket. This is useful regression coverage, but it is not a fixture produced by the frozen merged tree, nor does it exercise the required historical project/artifact inventory. The neutral fixture uses MemoryWorkspaceStore and stops at scope/grant resolution, without exercising ticketing behavior.

Required change: Add deterministic identical ticket/child/receipt/external identifiers across two workspaces and two projects in one workspace, using real SQLite and crossed caller/execution contexts. Assert reads, lists/counts/search, revision handling, recovery, and zero foreign mutations. Produce upgrade data with the actual base writer or a provenance-pinned fixture. Extend the neutral integrations through ticketing operations. Narrow the scanner exception to its actual token declaration rather than excluding the entire proof file.

### 8. P2 — Default-off does not preserve Rust source compatibility

Locations: plugins/minco-plugin-ticketing/src/store.rs:473–494; plugins/minco-plugin-ticketing/src/service.rs:785–789.

The required TicketingStore::mark_activity_published and mark_audit_published methods gain mandatory project arguments. Comparing the base and candidate signatures shows that existing downstream trait implementations no longer match, regardless of workspace_isolation being false. The public receipt method also now requires an identity argument.

Required change: Preserve compatible entry points through additive scoped methods/facades or extension traits, while ensuring legacy entry points cannot bypass isolated mode. Add an unchanged pre-PR consumer fixture—including an external store implementation—to the compatibility evidence. A passing default-off runtime test does not establish this property.

## Deviations a–d

| Deviation | Disposition |
| --- | --- |
| a — Identity.scopes carrier instead of changing every use case | Accept the architecture; changes required before merge. The shared authorization helper is consistent with the settled design. The carrier must be lossless for historical IDs and must not become a substitute for profile-policy or durable-artifact binding. Findings 1, 3, and 6 remain. |
| b — Independent ticketing parser | Accept dependency separation; reject the current equivalence claim. A published crate need not depend on the unpublished plugin. A shared specification and identical positive/negative vectors can establish compatibility without that dependency. The empty-token discrepancy must be fixed. |
| c — Defer publication, facade feature, and catalog entry | Accept for this bounded PR. Do not represent those deferred surfaces as delivered. The publication constraint does not justify weakening runtime policy or compatibility requirements. The task records the dependency/publishing adjustment before the qualified run. |
| d — No workspace columns on jobs/audit tables | Not acceptable as implemented; columns themselves are not the requirement. Project-only command equality does not establish workspace ownership. Existing namespaces, envelopes, or bound adapters may be sufficient, but must be authoritative and tested. The project-bound mark methods are a useful improvement, not complete ISO-4 evidence. |

## ISO-7 qualification interpretation

I would not raise a blocker solely because c5b65977 differs from 6ba484e6. The written contract explicitly distinguishes evidence-only rebinding from implementation/configuration/migration/gate changes. The task records the complete controller at 6ba484e6; subsequent changes update the task evidence and three verification files, and c5b65977 itself is an empty commit over b8a148d8. The performance receipt still says NOT RUN, rather than inventing a new measurement.

That supports the recorded ISO-7 pass under the disclosed evidence-only exception, not a claim of independent rerun or execution at the final SHA. Hosted Linux performance and live-AWS qualification remain outside the demonstrated result. Fixing the implementation and proof blockers above will require qualification of the new frozen candidate; the existing green run cannot qualify those future changes.

Disposition: retain the draft. The isolation-first design remains sound, but this candidate needs boundary corrections and stronger acceptance proofs before merge.

EGO_CHAT_M15T01_R1_END
