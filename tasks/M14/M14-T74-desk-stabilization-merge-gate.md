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
  - Cargo.lock
  - Cargo.toml
  - tasks/M14/M14-T74-desk-stabilization-merge-gate.md
  - examples/ticketing-agent-console
  - examples/minco-desk
  - plugins/minco-plugin-ticketing
  - plugins/minco-plugin-audit/Cargo.toml
  - plugins/minco-plugin-audit/src/lib.rs
  - plugins/minco-plugin-audit/tests/downstream_witness.rs
  - plugins/minco-plugin-feedback/minco-plugin.json
  - plugins/minco-plugin-feedback/src/plugin.rs
  - plugins/minco-plugin-notifications/src/lib.rs
  - crates/minco-cli/src/delivery_evidence.rs
  - crates/minco-cli/src/plugin_cmd.rs
  - crates/minco-contract/src/generate.rs
  - crates/minco-contract/src/validate.rs
  - crates/minco-plan/src/inbound_mail.rs
  - crates/minco-plan/src/lib.rs
  - crates/minco-plan/src/model.rs
  - crates/minco-plan/src/sam.rs
  - crates/minco-plan/src/durable_work.rs
  - crates/minco-plan/examples/render_inbound_mail.rs
  - crates/minco-plan/tests/downstream_witness.rs
  - crates/minco-plan/tests/render_inbound_mail.rs
  - extensions/minco-aws-adapters/src/s3.rs
  - extensions/minco-aws-worker/Cargo.toml
  - extensions/minco-aws-worker/examples/ticketing_mail_seam.rs
  - extensions/minco-aws-worker/src/lib.rs
  - extensions/minco-aws-worker/src/ticketing_wake.rs
  - extensions/minco-sqlx-sqlite/src/plugin_adapters.rs
  - extensions/minco-sqlx-sqlite/migrations/plugins
  - extensions/minco-sqlx-sqlite/tests/fixtures/minco_1_12_plugin_migrations
  - extensions/minco-sqlx-sqlite/tests/published_migration_upgrade.rs
  - extensions/minco-sqlx-postgres/src/plugin_adapters.rs
  - extensions/minco-sqlx-postgres/migrations/plugins
  - extensions/minco-sqlx-postgres/tests/audit_migration_parity.rs
  - extensions/minco-sqlx-postgres/tests/fixtures/minco_1_12_plugin_migrations
  - proofs/realtime-appsync/aws-handler/Cargo.lock
  - scripts/dev/ticketing-mail-seam.sh
  - scripts/quality.sh
  - scripts/source_manifest.py
  - scripts/test/desk_binary_lifecycle.py
  - scripts/test/examples/validate.py
  - scripts/test/inbound_mail_template_parse.py
  - scripts/test/quality_assurance.py
  - docs/DECISIONS.md
  - docs/adrs
  - docs/reference/generated
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
  - verification/operational-evidence-validation.json
  - verification/publish-validation.json
  - verification/1.9-performance-baseline.json
  - verification/release-identity.json
  - verification/agent-workflows.json
  - verification/quality-assurance-policy.toml
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

- **R4 DONE** (`fix(ticketing): durable outbound send intents...`):
  migration 0014 ticketing_send_intents; the logical send identity
  commits BEFORE provider contact and rides the mail as a
  minco-send-id tag; provider acceptance resolves sent WITH the
  provider's own message identity (SES overwrites caller
  Message-IDs); ambiguity holds in recovery_required; reconciliation
  drives the intent (accepted → sent, no-send → pending for one
  identity-stable resend); proven: retry after
  intent-resolved-but-evidence-failed never contacts the provider.
- **R5 DONE** (`fix(ticketing): activity intents dispatch to a real
  audit service...`): migration 0015 audit delivery marks; portal
  carries the audit service; dispatch_pending_audit appends intents
  as semantic records with the intent id as the audit event id; the
  plugin wires the fetched AuditService; the desk worker drives the
  audit pass; proof of once-only delivery.

- **R6 DONE** (`fix(ticketing): structural inbound authentication...`):
  RFC 8601 structural parsing of ALL Authentication-Results headers
  with configured authserv-id trust (foreign ignored, two-trusted
  quarantines), inbound_auth_policy (LocalTrusted default;
  RequireAlignedSpf/Dkim/Dmarc quarantine missing/GRAY/malformed and
  demand aligned passes via header.d/smtp.mailfrom), X-SES-Spam/
  Virus-Verdict FAIL quarantines always; forged/ambiguity/misalignment
  unit tests.

- **R7 DONE** (`fix(ticketing): atomic pool-mode assignment...`):
  assign_ticket_atomically performs revision check, cursor advance /
  locked workload, update and intent append in one transaction
  (SQLite BEGIN IMMEDIATE); stale round-robin burns no slot (proved).
- **R8 DONE** (`fix(ticketing): freshness-bound automation runs...`):
  dedicated ticketing-development profile; dedupe identity bound to
  revision + subject/description digest + unique run id; the handler
  classifies stale work ticketing.automation_superseded and stores no
  proposal (proved).
- **R9 DONE** (`fix(plan): SES rendering hardening...`): TlsPolicy
  Require; PutObject scoped to the key prefix; aws:SourceArn names the
  exact receipt-rule ARN; the bucket DependsOn the queue policy.
- **R10 DONE** (`fix(desk): non-local fail-closed startup...`):
  generated credentials rejected outside the local profile; explicit
  persistent SQLite URL required; the token prints local-only; real
  /live and /ready routes execute the registered checks (verified on
  the spawned binary).

**Round 3 (2026-08-28, exact-head review 5048859089 at 4d225fd2)**:
all six P0 and three P1 residual findings closed, one described
change per finding:

- **R11/P0-1** replay grants carry the handoff's ACTUAL permission
  subset; rotation is a store-owned CAS (claim_session_rotation) with
  a FIXED deadline never extended by rotation; race losers revoke
  their minted session; revoke failures surface. Proofs: read-only
  handoff replay stays read-only (403 on write), 50 concurrent
  replays leave exactly one live bearer.
- **R12/P0-2** requester-reply idempotency keys/fingerprints are
  principal-scoped (operation+project+subject+ticket+body+revision);
  effective key = reply:<subject>:<client key>; cross-requester reuse
  of an identical key gets its own record and fails ownership —
  proven with zero cross-principal leaks and no duplicate mutation.
- **R13/P0-3** fenced send state machine: SafeAfterBackoff persists
  sending->pending_send; the re-attempt must win a
  claim_send_attempt CAS (pending_send->sending) before any provider
  contact. Proofs: throttled-then-success retries genuinely resend;
  a pre-claimed sending intent makes zero provider calls.
- **R14/P0-4** the SES bucket policy builds each !Sub substitution
  whole and quotes it exactly once (the nested-quote YAML defect is
  gone); a new structural gate parses the COMPLETE rendered template
  with a CFN-tag-aware YAML loader (render_inbound_mail example +
  scripts/test/inbound_mail_template_parse.py, wired into
  quality.sh). sam validate --lint remains unavailable locally.
- **R15/P0-5** bounded RFC 8601 grammar: balanced CFWS comment
  stripping before tokenization; tokens classified by key name (SES
  property-only clauses parse); quoted RFC 5322 values and
  mailbox->domain extraction; SPF envelope-from / DKIM header.i /
  DMARC header.from alignment per RFC 7489; malformed untrusted
  headers are ignored, malformed trusted-claim headers quarantine.
  Byte-accurate AWS SES fixture tested across all strict policies.
- **R16/P0-6** the desk worker cycle runs Jobs, domain-Events and
  Audit dispatch independently (a failing pass never starves the
  others; failures aggregate). The spawned binary proves durable
  intents are published; an in-process test proves one cycle advances
  both published_at and audit_published_at.
- **R17/P1** AuditSink contract is now idempotent-by-event-id
  (memory dedupes; SQLite/Postgres ON CONFLICT DO NOTHING; contract
  test).
- **R18/P1** automation dedupe includes run_id (explicit second runs
  are distinct; identical submissions still dedupe); the handler
  recomputes and compares the subject+description digest.
- **R19/P1** non-local startup requires >=32-char secrets, explicit
  mode=rwc non-memory SQLite, explicit portal origin and allowed
  origins — each refusal proven on the spawned binary.

**Round 4 (2026-08-29, exact-head review 5055654066 at 389be158)**:
all four P0 and three P1 residual findings closed:

- **R20/P0-1** fenced exchange generations (migration 0016) + logout
  revokes the replay grant: stale workers cannot clobber the winner;
  50-concurrent single-bearer proof holds; logout kills replay.
- **R21/P0-2** canonical hashed effective key (SHA-256 of length-framed
  identity); receipts carry operation/project/subject-digest/expiry
  (migration 0017); recovery verifies ALL scope fields; genuine
  cross-requester proof (user-2 handoff through the same service,
  403/404 on stranger reply, one mutation).
- **R22/P0-3** attempt-fenced send state machine (migration 0018):
  claim_send_attempt issues a unique attempt UUID with lease; every
  transition validates the SAME attempt (resolve_send_intent_fenced);
  no state write is ignored; stale worker's transition fails.
- **R23/P0-4** per-method AuthenticationMethodResult (verdict + its own
  properties); cross-clause association for SES's envelope-from;
  DMARC header.from only (no header.d fallback); GRAY and
  PROCESSING_FAILED spam/virus quarantine. Cross-result assembly
  attack (dkim=none/@victim + dkim=pass/@attacker) rejected.
- **R24/P1-1** AuditSink contract is idempotent by (event id, semantic
  fingerprint): same id + different content returns AuditError::Conflict.
- **R25/P1-2** automation payload carries bound_policy_digest; handler
  verifies both context and policy digests; dedupe identity is
  SHA-256 hashed (raw concatenation exceeded the 128-byte envelope
  limit, surfacing as opaque 'jobs-handler-failed').
- **R26/P1-3** non-local requires ≥32 chars AND ≥8 distinct chars;
  HTTPS portal origin (no wildcard/credentials/fragment/slash);
  HTTPS return paths (local keeps default); liveness = critical checks
  only, readiness = all checks. 10 spawned-binary proofs green.

**Round 5 (2026-08-29, exact-head review 5057195399 at 37ceb32b)**:
all four P0 and three P1 residual findings closed (R27–R33):

- **R27/P0-1** SQLite plugin migration 0001 restored byte-identical to
  the Minco 1.12 release (an in-place edit failed every real upgrade
  with sqlx VersionMismatch); the audit fingerprint column now lands
  as forward-only 0004 on BOTH SQLite and PostgreSQL. Upgrade proofs
  build a database from the exact released migration bytes (real
  recorded checksums) and upgrade it; immutability guards pin the
  shipped files to the released bytes (`tests/fixtures/
  minco_1_12_plugin_migrations/` in both extension crates).
- **R28/P0-2** rotation is revoke-first with durable staging
  (migration 0019): the previous bearer dies BEFORE a replacement is
  minted and the mint is staged on the grant between mint and CAS, so
  an interrupted rotation recovers (staged bearer retired, marker
  cleared) instead of leaking a second live bearer. stage/complete/
  clear are store-owned CAS operations whose SQLite implementations
  check rows_affected; the takeover UPDATE gained the missing
  revoked_at IS NULL and unstaged guards; the initial INSERT race
  returns the winner via ON CONFLICT + affected-row check; the loser's
  abandon removes the grant only when it records the loser's own
  session; logout fails closed (grant-revoke failure = 503, cookie
  retained, retryable). Proofs: insert race, stale takeover, revoked
  refusal, staging fences + recovery on memory AND SQLite; concurrent
  initial exchanges converge on one live bearer under any
  interleaving; injected rotation-stage failure leaves zero live
  bearers with recovery on the next replay; logout-fails-closed HTTP
  proof; replay storm raised to 100 concurrent.
- **R29/P0-3** complete_send_attempt is ONE store transaction
  (migration 0020 scopes evidence rows to their attempt): fence
  validation, the sent transition, the attempt-scoped accepted
  evidence and the threading identity commit together — sent can
  never exist without its evidence, and a stale attempt writes
  NOTHING (returns reconciliation_required, never Ok; the round-4
  warn-and-still-record path is gone). Ambiguous/permanent-failure
  evidence is attempt-scoped and refused for stale attempts. Proofs:
  stale provider success writes nothing (state, evidence, threading
  all absent) while the current owner completes atomically; sent
  implies evidence and a retry never re-contacts the provider; stale
  ambiguity evidence refused; concurrent claims admit one owner;
  SQLite single-transaction proof.
- **R30/P0-4** policy-scoped evaluation replaces the global
  any-failure loop (AWS SES's own documented sample — one valid DKIM
  signature, one unrelated permerror, SPF+DMARC pass — was
  false-quarantined): each policy judges only its own mechanism's
  aligned pass; unrelated failures are evidence; reject_any_auth_
  failure is the explicit operator opt-in. Full bounded RFC 8601
  tokenizer (quoted values with spaces/escapes, escaped parens,
  unterminated comments, dkim/1 versions, reason= with embedded
  semicolons, angle-bracket domains). ScanVerdictPolicy (local |
  require_clean) makes missing/empty/malformed verdicts unverified in
  the production SES profile — never a silent pass; wired through
  DESK_INBOUND_AUTH_POLICY / DESK_INBOUND_SCAN_VERDICTS /
  DESK_INBOUND_AUTHSERV_ID. AWS-official-sample, parser-structure,
  attacker-injected-header and scan-verdict proofs.
- **R31/P1-1** audit fingerprints are SHA-256 over a length-framed
  canonical encoding (no delimiter collisions, fixed 64-hex digest,
  storage-normalizing canonical timestamp); PostgreSQL gains the same
  same-id-different-content Conflict semantics (no more ON CONFLICT DO
  NOTHING swallow) with content-verified legacy adoption on both
  engines (safe backfill). Proof against the live local PostgreSQL
  server (env-gated as the seed proofs).
- **R32/P1-2** the automation context digest covers EVERY proposal
  input (schema version, subject, description, ticket type, full form
  answers, full knowledge links, revision; length-framed) and the
  policy digest binds the COMPLETE effective policy (schemas, full
  AutomationConfig including review posture, exclusion list) — a
  review-posture change under the same profile now invalidates stale
  runs. The run id derives from the client Idempotency-Key (UUIDv5
  over project|ticket|operation): a retried submission creates ONE
  durable job; distinct keys are distinct runs.
- **R33/P1-3** non-local credentials accept real key material:
  DESK_AGENT_TOKEN_FILE / DESK_CSRF_SECRET_FILE read from a file
  (rotation = update + restart; never argv or logs), env values
  decoding as hex/base64 to ≥32 random bytes pass on decoded strength
  (a 64-hex token with two distinct characters accepted; a repeated
  predictable passphrase still rejected). Readiness grows from two
  checks to six: sessions + idempotency probes, audit dispatch backlog
  (10k threshold) and a real object-storage write/delete probe join
  ticketing and jobs stores (all non-critical; liveness unchanged).
  Six new spawned-binary proofs.

**Round-5 qualification (2026-08-29)**: `sam validate --lint` became
RUNNABLE via `uv tool run --from aws-sam-cli sam` (SAM CLI 1.165.0) —
and it immediately caught a real E3004 circular dependency
(TicketingRawMailBucket DependsOn TicketingMailQueuePolicy while the
policy references the bucket ARN). The dependency was moved to the
ReceiptRule (the consumer), the template now validates clean, and the
sam validate --lint gate is wired into
scripts/test/inbound_mail_template_parse.py. Full
`./scripts/quality.sh` re-run at the round-5 head (evidence chain
regenerated: source manifest 1,943 files, tree digest
51b38b5d…; 1.9 baseline re-bound; operational evidence PASS with the
two known no-provider warnings). Mimosa pre-push scanner remains
scanner_enobufs/inconclusive — a full exact-head scan was requested
and recorded as such, never converted into a pass. The round-5
closure matrix rides the PR body; the next independent exact-head
re-review is the human gate before merge.

**Local release gate (2026-08-30 follow-up)**: the six missing
prerequisites were installed at their pinned versions (cargo-nextest
0.9.143, cargo-llvm-cov 0.8.7, cargo-mutants 27.1.0,
cargo-semver-checks 0.50.0, `sam` on PATH via
`uv tool install aws-sam-cli`, zig 0.16.0, plus the rustup llvm-tools
component), and every local-release step was executed:

- `./scripts/quality.sh` (inside local-release.sh): exit 0 — full gate
  green as recorded above.
- `scripts/ci/local-assurance.sh --ephemeral` with
  `MINCO_QUALITY_TOOL_ROOT="$HOME/.cargo"` (the documented tool-root
  installation): nextest parity PASS (160 executable + 1 doctest after
  the policy's stale pin was refreshed 153→160 — round 4 added seven
  minco-plan tests while this lane was unrunnable), measured coverage
  PASS, mutation PASS (24+19 caught, 3 unviable, 0 missed, 0 timeouts
  — exactly the policy budget), then **semver FAILED CLOSED**:
  `cargo semver-checks` vs the pinned v1.9.0 baseline found two
  genuine breaking public-API additions introduced by this PR —
  `DeploymentPlan.inbound_mail` (exhaustively-constructible struct
  gained a field) and `AuditError::Conflict` (exhaustive enum gained a
  variant). `#[non_exhaustive]` was tried and reverted: semver-checks
  0.50.0 lints that addition itself. Resolution requires a maintainer
  decision — a workspace major version (2.0.0) or an API redesign
  (sidecar-carried topology, non-typed conflict channel) — and is
  recorded as an OPEN limitation, never converted into a pass.
- `proofs/realtime-appsync/scripts/test-local.sh`: PASS after its
  stale `aws-handler/Cargo.lock` was regenerated offline (the lock had
  drifted from the workspace's newer dependency set while this proof
  was unrunnable).
- `scripts/release/candidate-recovery.sh`: PASS.
- `scripts/release/candidate-load.sh`: PASS.

**Round 6 (2026-08-31, exact-head review 5060065907 at 2bdd7e2f)**:
the SemVer verdict was implemented as ruled — the lock-step 1.x family
stays 1.x, both breaking APIs were redesigned compatibly, and the
complete release controller exited 0:

- **`DeploymentPlan.inbound_mail` REMOVED.** The inbound-mail topology
  is a true explicit sidecar (the durable-work pattern): apply
  projects only into the EXISTING queues/triggers/function
  collections; the renderer threads the bindings through an internal
  `render_sam_template` entry so the worker IAM environment is scoped
  by the applied sidecar; validate/cost/render keep receiving the
  topology. A downstream witness
  (`crates/minco-plan/tests/downstream_witness.rs` — an external
  integration-test crate) constructs `DeploymentPlan` with the FULL
  published v1.12 struct literal and compiles; any future public
  field breaks that witness exactly as it would break downstream.
- **`AuditError::Conflict` REMOVED.** The integrity conflict rides the
  existing `Append(String)` variant behind the stable
  `MINCO-AUDIT-CONFLICT` machine code with a public constructor
  (`audit_conflict_error`) and detector (`is_audit_conflict`); the
  canonical fingerprint, same-id idempotence, conflict semantics,
  migration 0004, legacy adoption and memory/SQLite/PostgreSQL parity
  all keep their round-5 behavior. A downstream witness exhaustively
  matches the original two variants with no wildcard arm and
  compiles.
- **cargo-semver-checks: PASS** — `no semver update required` for
  both crates against exact v1.12.0 AND the policy-pinned v1.9.0; the
  complete family lane passed inside the controller.
- **local-release.sh exit 0** (the full controller, one invocation,
  at the source-frozen tree): the aggregate gate with chromium +
  firefox 40+40 green in-gate, 1,657 cargo tests, the measured
  assurance lane (nextest parity 161 — the plan witness adds one to
  the lane's core/plan/release count; coverage; mutation; family
  semver), AppSync local proof, candidate recovery, candidate load,
  the packaged-crate verification (which surfaced and fixed a latent
  break: ticketing's default-feature unit tests referenced the
  jobs-gated plugin — now cfg-gated), the publication dry-run on a
  clean tree, and the docker-runtime E2E. Two latent infrastructure
  gaps were also closed en route: the generated reference docs and
  the AppSync aws-handler lockfile were regenerated, the two runtime
  container images pre-pulled, and Docker Desktop started for the
  runtime lane.
- No lint was relaxed, no baseline moved, no exception recorded: the
  two findings were fixed by compatible redesign.

**Round 7 (2026-09-01, exact-head review 5064401898 at cd8c8c92)**:
the Plan-sidecar blocker was closed exactly as prescribed:

- **Derived state refresh**: `apply_inbound_mail` now recomputes
  `local_aws_services` and `iam_intents` from the final collections
  through ONE package-private helper (`refresh_derived_plan_state`)
  shared with `apply_durable_work` — a third sidecar cannot repeat the
  omission. The wake queue's visibility timeout derives from the bound
  worker's timeout (six-fold + batching window) instead of a fixed
  300s constant that fell one second short of the validator's own
  MINCO-SQS-002 rule.
- **Fail-closed disabled rendering**: `render_sam_with_inbound_mail`
  refuses a disabled topology carrying bindings BEFORE any binding
  reaches the base renderer (stable PlanError); a cleanly disabled or
  empty topology renders the unmodified base template.
- **Six regressions**: applied plan passes ordinary
  DeploymentPlan::validate; sqs appears in local_aws_services and both
  derived fields equal fresh derivations; double-apply structural
  stability; disabled-with-bindings rejected by validation AND
  rendering; durable-work + inbound-mail composition in both orders
  (ordinary validation, both sidecar validators, derived-field
  equality, set-level collection convergence, composed template
  renders).
- **Task ownership truth**: the owned_paths front matter now lists
  every path rounds 5–7 actually touched.
- **Qualification at the frozen head**: focused minco-plan tests and
  clippy -D warnings clean; rustfmt --check on every changed Rust
  file; cargo semver-checks reports no update required vs v1.12.0 AND
  the policy-pinned v1.9.0; `sam validate --lint` PASS on the rendered
  template (wired in the parse gate); measured lane pin refreshed
  161→166 (the six regressions are executable lane tests); evidence
  chain converged (manifest a60857b1…); and the COMPLETE
  `scripts/ci/local-release.sh` exited 0 in one invocation — 1,744
  cargo/doc tests, chromium + firefox 40+40 in-gate, the measured
  assurance lane, AppSync local, candidate recovery/load,
  packaged-crate verification, publication dry-run on a clean tree,
  multi-release rehearsal, and the docker-runtime E2E.

**Round 8 (2026-09-01, exact-head review 5072859042 at fe6fae26)**:
the three residual findings closed as prescribed:

- **P0 resource ownership**: the inbound-mail sidecar owns its
  resources under an exact-shape contract. apply/validate share
  expected_wake_queue/expected_wake_dlq/expected_wake_trigger builders
  (a same-ID resource is reused only when semantically identical);
  validate compares queue FIFO/visibility/retention/DLQ/max-receive,
  trigger function/queue/batch/window/partial-batch/concurrency, and
  rejects a second consumer on the wake queue (competing Lambda
  consumers steal messages) and binding ids collapsing to one
  CloudFormation logical id (MINCO-MAIL-014…018); the renderer refuses
  any plan whose validation is non-empty. The boundary is
  bidirectional: durable-work profiles now prove their event-source
  mapping exists (MINCO-JOBS-020) because apply's queue-key dedup
  silently skips a foreign-owned queue.
- **P1 rule-set identity**: the shared SES receipt rule set is named
  `{application}-{environment}-inbound-mail-{12-hex digest}` where the
  digest covers the ORDER-INDEPENDENT binding set — reordering
  bindings no longer replaces the provider rule set and two
  applications never collide; the name is bounded to the 64-character
  SES limit; activation stays an explicit operator step. The
  structural python gate asserts the new identity shape.
- **P0 repository truth**: ADR-0061 amended for the bounded ten-record
  envelope; ADR-0065 amended on all four drifted points (sidecar-only
  plan — no DeploymentPlan field, ScanEnabled true, wake DLQ required,
  sam lint real and passing) plus the exact-shape ownership and stable
  rule-set identity; ADR-0051/0063/0070/0072 carry dated amendments
  for the stabilization refinements; ADR-0056 audited with no drift
  found; docs/DECISIONS.md rows updated to Accepted (amended
  2026-09-01).
- Regressions: all nine reviewer collision cases (wrong queue shape,
  missing/wrong DLQ, FIFO, wrong-worker trigger, wrong batching,
  partial-batch off, second consumer, durable/inbound collisions in
  both orders, logical-ID collapse) plus idempotent exact-shape reuse
  and the reorder/two-application rule-set proofs — 19 tests in the
  sidecar suite.

**Round 9 (2026-09-02, exact-head review 5083559431 at 5a6c0dc0)**:
the provider/deployment and durable-ownership blockers closed:

- **P0-1 clean-create graph**: the S3-to-SQS queue policy builds
  aws:SourceArn from the EXPLICIT configured bucket name (never
  !GetAtt the bucket resource), the bucket DependsOn the queue policy,
  and the graph is Queue → QueuePolicy → Bucket(+Notification) →
  BucketPolicy → ReceiptRule — S3's notification-time destination
  permission validation can no longer race. The structural python gate
  asserts the explicit SourceArn, the bucket DependsOn and the
  provider order; a rendered-graph regression proves acyclicity by
  Kahn's algorithm; sam validate --lint passes on the new graph.
- **P0-2 SES rule ordering**: the receipt rule references the rule set
  with !Ref (a real CloudFormation dependency — identical literals
  create none) and DependsOn the SES-write bucket policy and the wake
  queue policy, so the bucket, its write grant and the rule set exist
  before the enabled rule; activation stays an explicit operator step.
- **P0-3 durable-work exact-shape ownership**: expected
  queue/function/trigger builders shared by apply and validate
  (MINCO-JOBS-021/022/023 for same-ID wrong-shape queue, function and
  mapping; MINCO-JOBS-024 for a competing second consumer on the
  profile queue) — the wrong-artifact/wrong-timeout/wrong-redrive/
  wrong-batching base-plan pre-provision attack now fails closed;
  exact-shape repeat application stays idempotent.
- **P0-4 rule-set name**: the digest is a SHA-256 over a canonical
  length-framed encoding of the FULL application, environment, region
  and sorted binding set, so visible-prefix truncation can never
  collide two deployments; the 64-character budget counts every
  separator (the round-8 budget could emit 65). Boundary regressions:
  max-length prefixes, punctuation-only input, same-first-20-chars
  applications, different environments, repeat stability.
- **P1 physical ingress ownership**: duplicate normalized mailbox
  scopes (MINCO-MAIL-019 — SES evaluates every matching recipient
  rule; duplicates are accidental fan-out) and duplicate physical
  bucket names (MINCO-MAIL-020) are rejected; shared-mailbox fan-out
  requires an explicit future model.
- ADR-0065 carries the 2026-09-02 amendment (graph, full-identity
  name, ingress ownership) and DECISIONS.md is updated.

Recorded statuses unchanged: Mimosa inconclusive; hosted Linux
performance and live AWS provider evidence NOT RUN per the
no-provider-contact policy. The PR stays draft pending the final
independent exact-head re-review.

**Round-2 final qualification (2026-08-28)**: ./scripts/quality.sh
exit 0 with 1,233 workspace cargo tests, every python suite OK
(including the spawned-binary lifecycle/health proof), chromium and
firefox suites green, five feature-boundary cargo checks clean,
clippy -D warnings clean, cargo deny/audit and npm audit clean,
gitleaks clean, and the manifest/release-identity/operational chain
converged. One pre-existing firefox widget flake (feedback widget,
unchanged since the reviewed head 170f434a) failed one run and passed
three consecutive reruns plus the final gate run; recorded as
environment flakiness, not converted into a pass. The R2-R10
described changes sit on top of the previously finished M14-T74 head;
the PR body carries the round-2 closure matrix. Next human gate: an
independent exact-head re-review of the new head.

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
