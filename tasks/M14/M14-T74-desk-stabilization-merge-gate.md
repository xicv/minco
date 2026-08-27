---
id: M14-T74
title: Desk stabilization merge gate — close independent review blockers
milestone: M14
status: active
priority: critical
area: desk/stabilization
depends_on: [M14-T73]
operations: []
owned_paths:
  - .gitignore
  - tasks/M14/M14-T74-desk-stabilization-merge-gate.md
  - examples/ticketing-agent-console
  - examples/minco-desk
  - plugins/minco-plugin-ticketing
  - docs/DECISIONS.md
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
  - verification/operational-evidence-validation.json
  - verification/publish-validation.json
checks:
  - cargo test -p minco-plugin-ticketing -p minco-desk-example --all-targets --locked
  - cargo clippy -p minco-plugin-ticketing -p minco-desk-example --all-targets --locked -- -D warnings
  - ./scripts/quality.sh
---

# M14-T74 - Desk stabilization merge gate — close independent review blockers

Independent stabilization and merge gate for PR #187 at reviewed head
`9134cda492fb4eef6dbe323f4dbf012c93d9e89a` (review comment 5037158773:
not merge-ready). No feature expansion; no mobile or multi-product work
(per the 2026-08-27 multi-product/mobile review, stacked follow-ups wait
until this gate closes and PR #187 merges).

## Goal

Close every merge-blocking finding from the independent review, in the
order the review prescribes (generated dependencies first):

1. **Tracked generated dependencies.** Remove
   `examples/ticketing-agent-console/node_modules` (189 files) from the
   tree, add a `node_modules/` ignore rule, and regenerate the
   source-manifest/verification evidence.
2. **Standalone composition truth.** `examples/minco-desk` must either
   implement the durable, authenticated composition its documentation
   claims (wire the available SQLite stores and all portal services into
   the real `TicketingService`, add a verified-claims/service-auth
   boundary, prove restart/replay over real HTTP) or the
   `standalone-private-beta` claim must be renamed to a providerless
   composition demo.
3. **Standalone Jobs path.** Configure a same-transaction
   `TicketingJobEnqueue` adapter for `SqliteTicketingStore`, replace
   `FailClosedDispatcher` with an operated publisher/worker path, and
   add a local dispatch/worker proof for
   `notify_requester_on_public_reply`.
4. **Requester-session idempotency replay safety.** Make session
   issuance plus replay result atomic/recoverable (the bearer lives only
   in `Set-Cookie`), fail explicitly on `idempotency.complete` errors,
   expire the cookie on logout; add lost-response, stale-lease and
   completion-failure tests.
5. **Email receiving topology.** Fix the SES rule (full
   `mailbox_scope` recipients, re-enable spam/virus scanning, one shared
   receipt-rule set, DLQ for the wake queue,
   `aws:SourceAccount`/`aws:SourceArn` on the SES S3 writes) and process
   bounded multi-record S3 notifications with expected bucket/prefix
   binding, partial-failure/replay behavior and rule-set activation
   proofs.
6. **Inbound email trust and first contact.** Add a trusted-sender /
   participant policy or signed Reply-To token, quarantine failures, and
   support verified first-contact email ticket creation instead of
   rejecting unthreaded mail.
7. **Inbound stale-revision convergence.** Reload-and-append through an
   atomic idempotent ingress operation or classify the immutable stale
   payload as permanent/replan-required; retries with the same stale
   `expected_revision` must be impossible.
8. **Outbound reconciliation.** Use a stable RFC Message-ID / provider
   idempotency identity and an authoritative reconciliation operation
   before any resend; register outbound threading identity so replies to
   portal-originated tickets resolve.
9. **Activity dispatch single-publish.** Replace
   `pending_activity_intents -> publish -> mark` with an atomic claim
   (Events outbox lease lifecycle or Jobs), preserve a stable event ID
   derived from the intent, and add concurrent-dispatch tests.
10. **Permission separation and manifest parity.** Complete
    requester/agent capability separation, constrain handoff-granted
    permissions, mark safe GET operations `idempotent: true`, and
    enforce descriptor/OpenAPI/manifest parity.
11. **Typed form validation.** Reject mismatched union slots: the
    populated value slot must match the declared `kind`, not merely be
    the only populated slot.
12. **Frontend correctness.** Replace `window.prompt` ticket creation
    with an accessible dialog, use per-operation cancellation instead of
    one global abort controller, hide unavailable management controls,
    finish focus/restoration and error states, and either ship a
    requester portal page or narrow the standalone-product claim.

Then rerun, at the final head: focused tests, real
SQLite/PostgreSQL/Rustack/browser evidence, a fresh exact-head security
diff scan, `./scripts/quality.sh`, and the local release controller.
Because the PR spans ~312 files, provide stage-specific commit ranges
and an independent review checklist for the re-review request.

## Non-goals

- Mobile, multi-product, workspace/tenancy, OAuth/PKCE, push, sync or
  native-client work (stacked follow-up PRs after merge).
- Editing PeoplePlanner; contacting MSS or any provider/production
  system.
- New product features of any kind.

## Evidence

To be recorded at finish with exact commands and results.
