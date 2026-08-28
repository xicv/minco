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

Blocker 1 (generated dependencies), run 2026-08-27 in the
`minco-task-m14-t74` workspace on base `4c8683ea`:

- `jj file untrack examples/ticketing-agent-console/node_modules` —
  189 files / 99,344 lines removed from the diff; directory retained on
  disk for local browser testing and ignored via a new `node_modules/`
  rule in `.gitignore`.
- `uv run --locked python scripts/source_manifest.py` — 1,924 files,
  tree digest `caeb13c91a8cf80a590cde496b9a6892948f67d95092bc0f46770f1041d9995d`;
  `--check` PASS (the manifest always excluded `node_modules`, so only
  the `.gitignore` hash moved the digest).
- `verification/1.9-performance-baseline.json` re-bound to the new
  digest (same re-bind flow as `4c8683ea`); status remains the honest
  `NOT RUN`.
- `uv run --locked python scripts/validate_operational_evidence.py
  --output ...` — PASS, 0 errors, 2 known warnings (no live provider
  evidence; hosted Linux performance NOT RUN — both required by the
  no-provider-contact policy).
- `scripts/validate_static.py`, `scripts/validate_publish.py`,
  `scripts/deep_review.py` regenerated; deep-review status `ok`.
- `scripts/test/repository_truth.py` — OK (11 prior errors were the
  missing `cargo-minco` binary in the fresh workspace; fixed by
  `cargo build -p cargo-minco --locked`, 5m46s).
- `scripts/test/deep_review_exclusions.py` — passed;
  `scripts/test/operational_evidence.py` — OK.

Blockers 2–12, run 2026-08-27 in the `minco-task-m14-t74` workspace;
one described jj change per blocker (or pair), all tests green at each
slice:

- **Blocker 11** `fix(ticketing): typed form slots...` — the populated
  value slot must match the declared kind; OpenAPI wording regenerated.
- **Blocker 7** — external ingress is revision-free: both stores reload
  authoritative state inside their transaction; IngressMessage drops
  expected_revision; retries converge (proven after concurrent
  revision movement).
- **Blocker 10** — requester flows authorize
  ticketing.requester.read/.write; agent reads unify on
  ticketing.agent.read; handoff grants constrained to the requester
  portal set; every safe GET declares idempotent true (ticketing +
  feedback descriptors and manifests) and `cargo minco plugin validate`
  enforces GET=>idempotent repo-wide.
- **Blocker 4** — the session exchange's completion record carries the
  bearer server-side so a lost-response replay re-issues the identical
  Set-Cookie at 201 and the body never contains the token;
  idempotency.complete failures return 503
  ticketing_session_persist_uncertain and keep the lease claimed;
  logout expires the browser cookie.
- **Blocker 9** — activity intents publish through the events outbox
  claim/lease lifecycle with the event id equal to the intent id;
  concurrent passes proven to publish each intent exactly once.
- **Blocker 8** — one public reply carries one deterministic mail id
  (uuid v5) driving the rendered RFC Message-ID; unresolved ambiguity
  fails closed (ticketing.notification_reconciliation_required) until
  the new reconcile_outbound_delivery use case records the verified
  verdict; accepted sends register their threading identity so emailed
  replies resolve by message-id local part in both stores.
- **Blockers 2+3** `fix(desk): durable authenticated standalone
  composition...` — SQLite sessions/CSRF/idempotency/audit wired into
  the portal services and the plugin graph; the trust boundary is the
  session cookie plus the DESK_AGENT_TOKEN bearer (forged development
  headers authorize nothing); SqliteTicketingStore carries the
  same-transaction enqueue adapter; DurableJobDispatcher +
  DeskWorker::run_once form the operated dispatch path; real-TCP
  proofs cover atomic job commit, full-restart session/job/worker
  recovery, the bearer boundary, and logout expiry
  (tests/desk_durability_proofs.rs, 3 tests).
- **Blocker 6** — inbound From + Authentication-Results verdicts are
  parsed: threaded replies must come from the requester email
  participant, explicit spf/dkim failures quarantine permanently, and
  unthreaded verified mail creates a ticket atomically (new
  create_ticket_from_external store op) when the profile opts in
  (inbound_email_first_contact, default off).
- **Blocker 5** — full mailbox recipient + ScanEnabled true; one shared
  named receipt-rule set; SES writes bound by aws:SourceAccount and
  the rule-set SourceArn; wake queues gain DLQs with bounded
  max-receive; the wake handler processes bounded batches (<=10) and
  binds every record to the expected bucket and prefix.
- **Blocker 12** — creation dialog (novalidate + handler validation,
  inline aria-described errors, focus restoration), per-operation
  AbortControllers, capability-gated hiding of create/reply/note/
  manage controls; 34 chromium+firefox browser tests green; the
  requester-portal claim narrowed to the honest session-cookie API
  surface statement in the desk docs.

Test totals at the final slice: ticketing 112, desk 16 (13 prior + 3
durability), worker 22, plan 60+, browser 34; clippy -D warnings and
rustfmt --check clean on every touched crate.

**Exact-head re-review round 2 (2026-08-28, review comment 5046662764
at head 170f434a)** — continue M14-T74, ten new findings R1-R10:

- **R1 DONE** (`fix(desk): the shipped worker waits for SIGINT...`):
  with_graceful_shutdown aborted the worker immediately; now waits for
  Ctrl-C, aborts and awaits. Spawned-binary proof
  scripts/test/desk_binary_lifecycle.py (in quality.sh): real process,
  HTTP-driven durable job completed by the background loop, SIGINT →
  exit 0.
- **R2 DONE** (`fix(ticketing): atomic operation receipts...`):
  migration 0012 ticketing_operation_receipts commits the serialized
  authoritative result with the append in one transaction (memory +
  SQLite); the requester-reply wrapper surfaces completion failure as
  503 ticketing_idempotency_persist_uncertain and, after lease
  staleness, replays the receipt instead of re-executing. Audit: only
  requester reply + session exchange advertise Idempotency-Key today;
  the append-path mechanism is generic for future surfaces.
- **R3 DONE** (`fix(ticketing): rotation-based session replay...`):
  migration 0013 ticketing_session_exchange_grants stores only
  non-secret rotation material; replays ROTATE (new bearer, old
  revoked), bodies + shared records are token-free with
  Cache-Control: no-store; completion failure revokes + releases and
  503s; stale-lease takeover revokes the replaced session; missing
  grant fails closed. Desk durability proofs updated to rotation.

**Final-head qualification (2026-08-27)**: `./scripts/quality.sh`
exit 0 at the final head — 1,225 cargo tests passed workspace-wide,
every python evidence suite OK (including the recipe matrix after
ADR-0072 gained its required Features/Provider-assumptions/Cost/
Verification/Unsupported-gates sections and the minco-desk-example
check was registered), docs build/check-links/browser suites green,
five cargo check feature boundaries clean, workspace clippy -D
warnings clean, cargo deny/audit and npm audit clean, gitleaks clean,
and the source-manifest check verified (tree digest f6d4e1c8-era,
re-bound after each final edit; operational evidence PASS with the two
known no-provider warnings). The M14-T74 blockers are closed; the
independent exact-head re-review requested by the finding-12 verdict
remains the human gate before merge.
