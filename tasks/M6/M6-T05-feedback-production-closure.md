---
id: M6-T05
title: Close Feedback production and security release gates
milestone: M6
status: complete
priority: high
area: plugins/feedback
depends_on: [M6-T03, M6-T04]
operations:
  - createFeedback
  - getClientFeedback
  - replyToFeedback
  - listDeveloperFeedback
  - getFeedbackAiContext
owned_paths:
  - plugins/minco-plugin-feedback/**
  - plugins/catalog.toml
  - docs/adrs/0014-plugin-lifecycle-and-feedback.md
  - docs/architecture/capability-audit.md
  - tasks/M6/M6-T03-feedback-loop.md
  - tasks/M6/M6-T05-feedback-production-closure.md
checks:
  - cargo minco plugin validate
  - cargo test -p minco-plugin-feedback --all-features --locked
  - cargo test --workspace --all-features --locked
  - cargo minco deploy plan
---

## Goal

Make the explicit production-release decision for the official Feedback plugin
after its compiler, browser, database, provider-adapter, bounded real-AWS,
cleanup, and repository-wide security gates have all produced reviewable
evidence.

## Non-goals

- create an SES identity when the account has no pre-existing verified sender;
- create a slow, cost-bearing CloudFront distribution solely to change a
  lifecycle label;
- represent local emulation, template validation, or compiler coverage as a
  live provider operation;
- stabilize unrelated optional plugins.

## Acceptance

- The completed M2-T01, M6-T03, and M6-T04 prerequisites are reflected
  consistently in task and architecture evidence.
- The repository-wide Deep Security Scan completes and every validated finding
  is fixed and reverified, or an owner-approved, release-scoped waiver records
  repeated external scanner failure, validates any partial candidates, and
  requires the independent compensating-control matrix to pass.
- Feedback's runtime descriptor and catalog stability labels agree.
- Exact-head compiler, plugin, test, deployment, dependency, license, and
  secret checks pass.
- Live-cloud boundaries remain explicit, and no cloud service is touched
  without an append-only action and cleanup record.
- A focused single-task review finds no remaining release-blocking defect.

## Current evidence

M6-T03 and M6-T04 are complete. Feedback's compiler, HTTP, memory, PostgreSQL,
SQLite, CLI, Chromium, and Firefox gates pass. The selected AWS adapter suite
has exact-resource IAM, local Rustack conformance, bounded real-AWS provider
proof, and verified cleanup. M6-T05 owns the final exact-head review and the
narrow security-scan risk decision below.

## Release-scoped Deep Scan waiver

The repository-wide Deep Security Scan for Git revision
`c22b7e10ebef61f7f84dd19996e61b4316e7f8da` started on 2026-07-25 as scan
`09732e72-2643-4198-8780-07d9fa18bda3`. It terminated in discovery after the
service classified the defensive repository review as possible cybersecurity
risk. The failed run produced no canonical coverage ledger, validated findings,
completion seal, or report. It is not security evidence and cannot be
interpreted as a no-findings result.

Rejoin and replacement attempts did not produce a canonical completed report.
On 2026-07-25 the repository owner explicitly decided that repeated external
scanner failure must not block the otherwise reviewable release. This is a
one-release risk acceptance, not a scan pass or no-findings claim. It applies
only if the final source passes the exact-head matrix below, the available
partial Feedback candidates are manually validated and resolved, and the
focused review finds no release blocker. It expires for the next release or any
later security-sensitive Feedback change.

The partial discovery artifacts were from an older revision and were never
validated or sealed by the scan service. They were nevertheless treated as
review leads. Five Feedback roots were traced against current source:

- provider and infrastructure diagnostics in client-visible warnings were
  already replaced with stable public details and regression-tested;
- remote plaintext developer API URLs are now rejected while loopback HTTP
  remains available for local development;
- developer list/read/mutation/export/attachment operations are now bound to
  the configured project before data access;
- anonymous and project-key submissions can no longer choose the authoritative
  audit actor or client-notification subject, and developer mutations record
  the authenticated principal or fallback-token actor;
- provider/process transcription now requires authenticated
  `feedback.create`; configuration rejects transcription combined with
  `allow_anonymous` or the browser-visible `project_key`.

This compensating review does not replace repository-wide scanner coverage. It
does resolve every Feedback-specific lead that the incomplete discovery work
made available.

## Non-scan exact-source verification

- `cargo minco plugin validate` returned an empty finding array.
- `cargo test -p minco-plugin-feedback --all-features --locked` passed 42
  unit/HTTP/plugin tests and two persistence harness tests.
- `cargo test --workspace --all-targets --all-features --locked` passed the
  complete workspace suite under normal local execution. The bounded real-AWS
  and Rustack tests remain ignored in this generic command because their
  dedicated harnesses own their setup and cleanup.
- `cargo check --workspace --all-features --locked` passed.
- Warnings-denied Clippy passed for every target and feature of the modified
  Feedback crate. `rustfmt --edition 2024 --check` passed for only the four
  modified Rust files; no repository-wide formatter was run.
- The Feedback contract validator passed all 13 operations, Node syntax passed,
  and the browser suite passed `38/38` across Chromium and Firefox.
- The dedicated persistence harness passed SQLite and a temporary local
  PostgreSQL 18 instance. Its exact test container was removed, and a
  post-cleanup Docker query confirmed it absent.
- `cargo minco deploy plan` returned an empty diagnostics array and generated
  the local plan without contacting AWS.
- `./scripts/test/e2e.sh` passed the Orders HTTP journey.
- `cargo minco explain createFeedback --json` traced the OpenAPI contract,
  handler, application service, memory/PostgreSQL/SQLite/application-provided
  adapters, and test locations.
- `./scripts/dev/rustack-smoke.sh` passed S3, SQS, SSM, and STS CLI seams plus
  the compiled Minco S3/SQS/SSM adapter tests against the pinned local image.
  Its final unique Compose project was `minco-rustack-smoke-47094`; post-run Docker
  inspection confirmed that both its container and network were absent.
- `cargo audit` scanned 393 locked dependencies against 1,169 RustSec
  advisories and found no vulnerability.
- `cargo deny check advisories licenses bans sources` passed all four policies.
  It retained only the repository's existing unmatched-license allowances and
  duplicate-version warnings.
- `jj diff --from @-- --to @ --git | gitleaks stdin --redact` scanned the full
  M6-T05 patch and found no leaks. A second directory scan covered the current
  repository files and also found no leaks.
- Static validation returned zero errors and warnings. Deep review returned no
  error, retaining two pre-existing unwrap/expect warnings outside Feedback and
  one informational AWS example boundary.
- Whitespace validation passed by reverse-checking the parent task-definition
  change and forward-checking the child task-workspace change against their
  matching source states.

## Issue and external-service log

- The first full workspace test inside the restricted Codex sandbox failed when
  the sandbox denied a loopback TCP listener and native TLS trust-store access.
  The focused 21-test AWS adapter suite and then the complete workspace suite
  passed outside that restriction. The sandbox result is not counted as source
  evidence.
- The first deployment-plan invocation could not write its generated plan in
  the separate JJ workspace under the sandbox. The same compiled command passed
  with normal local workspace access and did not contact AWS.
- `cargo audit --locked` failed because this installed `cargo-audit` version
  does not support that flag. The supported `cargo audit` command passed.
- The first `cargo deny` invocation could not lock the read-only advisory cache
  under the sandbox. The identical policy check passed with normal local cache
  access.
- The first sandboxed secret scan received zero bytes because JJ could not lock
  the separate working copy; that output was discarded. The normal rerun
  scanned the full patch and passed.
- The first combined reverse-apply whitespace check used the parent workspace
  against a patch that also contained its child change, so the patch correctly
  did not apply. Splitting the validation by change/source state passed.
- The first new loopback-transport regression exposed that `Url::host_str`
  retains brackets for IPv6 literals. Loopback validation now normalizes those
  brackets before parsing, and the rerun passed HTTPS rejection plus localhost,
  IPv4, and IPv6 loopback cases.
- The first browser command used a global Playwright launcher before this fresh
  JJ workspace had its locked local package set, so it could not resolve
  `@playwright/test`. `npm ci` installed the four locked packages, reported zero
  vulnerabilities, and the unchanged browser command then passed `38/38`.
- `uv run` created the ignored local Python environment and resolved its one
  locked dependency before the contract/static checks. This was local tooling
  state plus read-only public package access.
- `scripts/validate_publish.py` reported seven existing `PUBLISH-042` package
  order errors in root `Cargo.toml`: the PostgreSQL/SQLite adapter crates precede
  internal session, audit, idempotency, or events dependencies. `Cargo.toml` is
  outside M6-T05 ownership, runtime/compiler tests are green, and this remains a
  separate M8 packaging-release blocker rather than an unreviewed task expansion.
- The standard `task-finish` wrapper always invokes the repository-wide
  `cargo minco check --with-cargo` gate. That gate includes repository-wide
  rustfmt and Clippy, contrary to this task's explicit modified-files-only lint
  boundary, and also includes the unrelated failing `PUBLISH-042` check above.
  Release transport therefore uses the wrapper's direct JJ
  describe/bookmark/push equivalents only after this task's scoped exact-source
  matrix passes.
- The deep-review command refreshed `verification/deep-review.json`, which is
  outside this task's owned paths. That generated side effect was removed; the
  command result is recorded above without committing the unrelated projection.
- `cargo llvm-cov`, `cargo vet`, and OSV-Scanner were not installed. They are not
  represented as passes. In particular, `cargo vet init` was not used because
  exempting the existing dependency set would not provide independent release
  assurance.
- `cargo audit` performed read-only fetches from the public GitHub RustSec
  advisory repository and crates.io index and updated only the local Cargo
  cache. No remote state was mutated and no cleanup was required.
- `npm ci` used read-only public npm registry access and changed only the
  ignored local `node_modules` tree. No remote state was mutated.
- Rustack used only the local Docker daemon and loopback endpoints. Its
  resources were deleted by the harness and independently confirmed absent.
- The PostgreSQL 18 test used only local Docker and loopback. Its exit trap
  removed the uniquely named test container, and an explicit absence check
  passed.
- No AWS API or other real cloud-resource mutation was performed by M6-T05.

## Focused single-task review

The review first removed an unnecessary disclosure of Deep Scan internal worker
accounting from task evidence. It then used the incomplete older scan only as an
untrusted lead inventory and validated every Feedback-specific root against
current source. That review found the four security defects fixed in this task,
plus two adjacent boundary gaps: developer mutations were audited as the client
instead of the authenticated manager, and exact client tokens were not also
checked against the configured project. Both were fixed and regression-tested.

The final diff was reviewed across authorization, project scoping, bearer
transport, audit identity, transcription cost boundaries, public diagnostics,
descriptor/catalog agreement, and waiver wording. M2-T01, M6-T03, M6-T04,
ADR-0014, the capability audit, the runtime descriptor, and the plugin catalog
now agree. No release-blocking defect remains within M6-T05's owned paths.

The repository has no configured Markdown linter for these modified Markdown
files. Their exact patches passed the repository-compatible whitespace check;
no formatting command or repository-wide formatter was run. The unrelated
publish-order errors remain explicitly outside this completed Feedback task.
