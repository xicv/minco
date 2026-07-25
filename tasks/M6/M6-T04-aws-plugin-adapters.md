---
id: M6-T04
title: Implement production AWS adapters for official plugin ports
milestone: M6
status: complete
priority: high
area: extensions/aws
depends_on: [M5-T01, M6-T02]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - extensions/minco-aws-*/**
  - extensions/minco-sqlx-postgres/**
  - extensions/minco-sqlx-sqlite/**
  - plugins/minco-plugin-identity/**
  - plugins/minco-plugin-object-storage/**
  - plugins/minco-plugin-static-site/**
  - plugins/minco-plugin-idempotency/**
  - plugins/minco-plugin-sessions/**
  - crates/minco/**
  - infra/aws/**
  - docs/architecture/capability-audit.md
  - docs/adrs/0014-plugin-lifecycle-and-feedback.md
  - docs/deployment/aws-plugin-adapters.md
  - docs/research/aws-plugin-adapters-2026-07.md
  - scripts/dev/rustack-smoke.sh
  - scripts/aws/run-adapter-smoke.sh
  - tasks/M6/M6-T04-aws-plugin-adapters.md
checks:
  - cargo test --workspace --all-features
  - cargo minco deploy plan
  - scripts/test/e2e.sh
---

## Goal

Add S3 object storage/signing, SQS event publication, transaction-integrated
PostgreSQL outbox recovery, SES and signed-webhook notifications, Cognito user
administration, and static-site S3/CloudFront rendering without changing Minco
core.

This closure also includes the persistent PostgreSQL/SQLite session,
idempotency, and audit adapters that the capability audit assigned to this task
but the original goal accidentally omitted.

## Acceptance

- Every adapter implements an existing provider-neutral official plugin port.
- IAM and cost intents are derived from selected capabilities.
- No adapter introduces a hidden schedule or fixed-capacity default.
- Local emulator and bounded real-AWS conformance evidence is recorded.

## Current evidence

Implementation and exact-source verification are complete. Research on
2026-07-25 found two prerequisite contract gaps: the identity plugin had no
administrative user port, and the static-site plugin had no publication port.
It also found that S3 presigned PUT cannot enforce the existing
maximum-upload-size contract; S3 requires a signed POST policy with
`content-length-range`. These additive provider-neutral seams are owned here so
the AWS adapters meet, rather than simulate, the acceptance contract.

## Issue and fix log

- 2026-07-25: `cargo check -p minco-aws-adapters --all-features`
  reached the new crate and failed with `E0382` in the SES adapter because the
  notification title was moved before the body renderer borrowed the
  notification. The adapter now renders the body before moving provider
  request fields. This is a source-level ownership fix; the check is rerun
  below rather than treating the first compile as partial evidence.
- 2026-07-25: the first
  `cargo test -p minco-sqlx-sqlite plugin_adapters` compile found that a
  `sqlx::Error` from fingerprint decoding was propagated through `?` without
  conversion to the provider-neutral `IdempotencyError`. The row decode now
  maps that database error through the store boundary before parsing the
  fingerprint; the behavioral test is rerun below.
- 2026-07-25: review of the PostgreSQL outbox found that implementing
  `OutboxStore::enqueue` with a pool is not sufficient for a transactional
  outbox: it cannot atomically join the application adapter transaction that
  writes domain state. `PostgresOutboxStore::enqueue_in` now accepts the
  caller's typed SQLx transaction; the provider-neutral `enqueue` method wraps
  the same operation in its own transaction. Claiming still uses
  `FOR UPDATE SKIP LOCKED`, and no recovery schedule is introduced.
- 2026-07-25: concurrent SQLite idempotency leases require a bounded busy
  timeout in addition to `BEGIN IMMEDIATE`; pool construction now applies the
  configured acquire timeout as SQLite's busy timeout instead of failing
  immediately under legitimate write contention.
- 2026-07-25: security review found that accepting an arbitrary prebuilt
  webhook HTTP client could not prove redirects were disabled, while normal
  DNS resolution left a rebinding path to private or link-local addresses.
  Webhook construction is now asynchronous, resolves and pins only public DNS
  addresses, builds a bounded no-redirect client internally, and exposes
  loopback HTTP only under the test boundary.
- 2026-07-25: IAM review found that provider-neutral capabilities alone cannot
  distinguish memory, webhook, or AWS implementations. The explicit
  `aws-adapters` marker plugin now records selected AWS provider capabilities
  and non-fixed-cost resource intents. IAM generation keys only off those
  markers and fails closed on missing exact ARNs, including static-site S3
  prefix and CloudFront invalidation permissions.
- 2026-07-25: the first combined provider-port test run failed because the new
  identity administration test composed an empty selection even though
  official plugins are intentionally opt-in. Identity and static-site
  injection tests now enable their exact plugin IDs before asserting graph
  capabilities; the combined test is rerun below.
- 2026-07-25: the single PostgreSQL umbrella test did not prove the two
  concurrency invariants or the caller-owned transaction seam, and the SQLite
  lease test did not actually exercise stale-lease rejection or write
  contention. Focused regressions now prove rollback removes the outbox row,
  concurrent outbox claims are disjoint, concurrent idempotency has exactly one
  owner, subject-wide session revocation works, a replaced SQLite lease cannot
  complete, and two file-backed SQLite writers serialize successfully.
- 2026-07-25: final source review found that S3 endpoint overrides, SQS queue
  URLs, and returned static-site URLs used ad hoc string checks. They now use
  the exact resolved `http` 1.4.2 URI parser, reject userinfo/query ambiguity,
  allow plaintext only for loopback emulators, validate hosts, and keep S3
  overrides at the endpoint root. Context7 was attempted first as required but
  returned `Monthly quota exceeded`; `chub` had no Rust `http` documentation,
  so the implementation was verified against the locally resolved crate source
  and compiler.
- 2026-07-25: the first SQS URL regression fixture constructed a generated AWS
  client without a behavior version and panicked before reaching Minco
  validation. The fixture now calls `behavior_version_latest()`. A direct
  invocation of the ignored Rustack test also failed because
  `AWS_ENDPOINT_URL` was intentionally absent; the supported
  `scripts/dev/rustack-smoke.sh` wrapper was rerun and passed, then its unique
  container and network were confirmed absent.
- 2026-07-25: teardown review found that the real-AWS harness set ownership
  booleans before create calls. A same-name race or provider response failure
  could therefore authorize cleanup without a confirmed create result. Resource
  names now include an unpredictable per-run nonce, the exact non-secret names
  are saved in `resources.json`, and every ownership boolean is set only after
  its create command succeeds. A lost response now fails toward a recoverable
  tagged resource instead of destructive cleanup of an unconfirmed target.
- 2026-07-25: direct use of the Cognito adapter could bypass the validation
  performed by `IdentityAdministrationService`, unlike the memory adapter.
  `InviteIdentity::validate` and public managed-username validation now define
  one provider-neutral boundary used by the service, memory adapter, and
  Cognito adapter. Static publication also now uses the AWS SDK's retryable
  file-backed byte stream instead of loading each complete asset into memory;
  documentation makes the per-prefix serialization requirement explicit.
- 2026-07-25: the bounded provider diagnostic used byte-index truncation at
  2,048 bytes and then appended an ellipsis. A multibyte provider message could
  panic at a non-character boundary, and ASCII output could exceed its stated
  bound. Truncation now reserves the ellipsis, backs up to a UTF-8 boundary, and
  has a long-Unicode regression.
- 2026-07-25: `PresignedObjectRequest` derived `Debug`, which exposed signed URL
  query parameters, authorization/header values, POST signatures, and temporary
  session tokens to accidental logs. Its custom debug output now redacts the URL
  and all values while retaining only method, expiry, and header/form-field
  names; serialization remains functional and a regression checks representative
  secrets are absent.
- 2026-07-25: the existing `PostgresPoolConfig` also derived `Debug` over its
  password-bearing URL. Its custom debug output now retains only bounded pool
  settings and a redacted URL marker, with a credential-regression test.
- 2026-07-25: package inventory review found that the PostgreSQL and SQLite
  crates' explicit include lists omitted the new SQL embedded by `include_str!`.
  Both manifests now include only `migrations/plugins/**` in addition to their
  prior source/license files. The new AWS crate also carries both workspace
  license texts in its package inventory.
- 2026-07-25: `cargo package -p minco-aws-adapters --allow-dirty --no-verify`
  could not prepare the upload because crates.io has `minco-core` 0.1.x but not
  the workspace's required 0.2.0. This is the existing M8 dependency-order
  release gate, not a package pass. `cargo package --list` does pass the local
  inventories for the AWS/PostgreSQL/SQLite crates; the AWS smoke renderer is
  included to avoid Cargo's omitted-example warning.
- 2026-07-25: direct idempotency-store calls enforced only a positive timeout,
  while `IdempotencyService` also enforced the documented 24-hour maximum.
  Provider-neutral `validate_claim_timeout` now defines the contract used by
  the service, memory store, PostgreSQL store, and SQLite store, preventing
  oversized duration arithmetic and adapter drift.
- 2026-07-25: the single-task review found three final boundary defects. An
  SQS `.fifo` URL was accepted when FIFO mode was false, which would omit the
  mandatory group and deduplication values and fail at AWS; queue-name suffix
  and mode must now agree in both directions. Managed Route 53 output emitted
  only an `A` alias even when the CloudFront distribution enabled IPv6; it now
  emits a matching `AAAA` alias only for the dual-stack profile. Finally, a
  positive but unrepresentable session TTL could panic through `DateTime`
  addition; session issuance now uses checked expiry arithmetic and returns
  `InvalidSession` instead. The first post-fix local SAM lint rendered only the
  no-domain profile, so the reusable smoke renderer now accepts optional
  custom-domain, certificate, and hosted-zone inputs; the exact dual-stack
  template can be linted without creating a distribution or touching AWS.
- 2026-07-25: continued port-boundary review found two validation paths that
  otherwise deferred predictable failures to concrete providers. Identity
  administration accepted whitespace, multiple `@` delimiters, empty DNS
  labels, and invalid domain-label boundaries; provider-neutral validation now
  rejects them before Cognito. Static-site configuration accepted `.` path
  components that the AWS publisher necessarily rejected later; configuration
  and publisher now agree on normal relative components.
- 2026-07-25: signed-webhook review found that its custom debug output still
  printed the configured endpoint, so a path or query capability could leak
  even though the HMAC secret was redacted. Request errors could repeat that URL,
  and only the body rather than the complete serialized notification was size
  bounded. Debug now redacts the whole URL, transport errors are deliberately
  generic, and title/link/body plus the exact serialized payload are bounded
  before network I/O.
- 2026-07-25: persistence review found that `migrate_plugin_storage` executed
  one idempotent SQL blob without recording a version or checksum. That was
  insufficient for production evolution even though first-install tests passed.
  PostgreSQL and SQLite now use embedded SQLx migrators with the independent
  `_minco_plugin_storage_migrations` history table, and behavioral tests assert
  the applied migration record.
- 2026-07-25: the first compiler pass for the embedded migrators failed because
  the SQLx adapter crates enabled migration runtime support but not SQLx's
  `macros` feature. Both manifests now opt into that exact feature, matching the
  already-embedded Feedback migrator; the database proofs below were rerun
  instead of treating dependency presence in `Cargo.lock` as activation.
- 2026-07-25: feature-isolation checks found that the bounded provider-error
  formatter was compiled but unused for non-S3 feature sets. The function and
  its regressions now share the exact `s3` feature boundary; each individual
  adapter feature is checked with warnings denied below.
- 2026-07-25: the final whitespace check initially invoked `git diff --check`
  in the task's JJ-only workspace, where there is intentionally no colocated
  Git working tree. It produced only Git's not-a-repository usage error and did
  not validate the patch. The permanent repository-compatible form streams
  `jj diff --git` into reverse `git apply --check --whitespace=error-all`;
  that exact check passed.

## Local conformance evidence

- `cargo check -p minco-aws-adapters --all-features` — passed after the recorded
  SES ownership fix.
- `cargo test -p minco-sqlx-sqlite plugin_adapters` — 3 passed, covering
  persistent session resolution/revocation, idempotency replay, and ordered
  append-only audit storage.
- `MINCO_TEST_POSTGRES_URL=<local Docker PostgreSQL> cargo test -p
  minco-sqlx-postgres
  plugin_adapters -- --nocapture` — 4 passed against PostgreSQL 18, including
  caller-transaction rollback and concurrent claim/lease invariants.
- `cargo test -p minco-sqlx-sqlite plugin_adapters -- --nocapture` — 4 passed;
  the file-backed concurrent idempotency case also passed five consecutive
  focused runs with exact temporary-file cleanup.
- `cargo test -p minco-plugin-identity -p minco-plugin-static-site -p
  minco-aws-adapters --all-features` — 21 unit tests passed after correcting the
  opt-in plugin test selection.
- `./scripts/dev/rustack-smoke.sh` — passed against the pinned Rustack image:
  S3/SQS/SSM/STS CLI seams, Minco SSM adapter, Minco S3 server/direct
  upload/download/delete adapters, and Minco SQS event publication. The unique
  final Compose project `minco-rustack-smoke-82927` and all emulator resources
  were removed by the exit trap; an explicit post-run query found no containers
  and confirmed its network absent.

## Final exact-source verification

- `cargo test --workspace --all-features` — passed on the final source,
  including the provider-neutral validation, redaction, checked-time,
  migration-history, and adapter regressions.
- `MINCO_TEST_POSTGRES_URL=<local PostgreSQL 18> cargo test -p
  minco-sqlx-postgres plugin_adapters -- --nocapture` — 4 passed; the embedded
  plugin migrator recorded its dedicated history row, caller rollback removed
  the outbox row, and concurrent claims remained disjoint.
- `cargo test -p minco-sqlx-sqlite plugin_adapters -- --nocapture` — 4 passed,
  including embedded migration history and file-backed concurrent lease
  serialization.
- `cargo clippy` with warnings denied passed for every target and feature of
  each modified crate. Separate warnings-denied checks passed with
  `minco-aws-adapters` in no-default mode and with each of `s3`, `sqs`, `ses`,
  `cognito`, `webhook`, `static-site`, and `full` selected alone.
- `rustfmt --edition 2024 --check` passed for only the Rust files modified or
  created by this task. No repository-wide formatting command was run.
- `./scripts/test/e2e.sh` — Orders E2E passed; `cargo minco deploy plan`
  returned an empty diagnostics array.
- `bash -n` and `shellcheck -x` passed for both changed shell scripts.
- AWS SAM CLI `1.164.0` locally linted the exact generated private
  S3/CloudFront OAC template with a custom domain; structural assertions found
  both the conditional `A` and `AAAA` Route 53 aliases. This check did not
  contact AWS or create a distribution.
- `cargo package --list --allow-dirty` passed for the AWS, PostgreSQL, and
  SQLite crates and confirmed their source, examples/tests, plugin migrations,
  and dual-license files are included. Preparing the AWS crate for upload
  remains correctly blocked by the M8 publication order because crates.io does
  not yet contain the workspace's `minco-core` 0.2.0.
- `cargo audit` found no vulnerable dependency; `cargo deny check advisories
  licenses bans sources` passed all four policies with only the repository's
  existing unmatched-license and duplicate-version warnings.
- `jj diff --git | gitleaks stdin --redact` scanned the final patch and found no
  leaks. The reverse Git-apply whitespace check passed, and
  `jj log -r 'conflicts()'` returned no conflicts.
- The single-task review found and resolved every issue recorded above. No
  remaining correctness or security defect was identified within M6-T04's
  owned paths. Live SES delivery is explicitly unproven because the account has
  no verified sender, and no cost-bearing CloudFront distribution was created;
  those are separate release rehearsals, not silent passes.

The first standalone `shellcheck` invocation reported only two informational
items: its default mode did not follow the already-annotated sourced helper,
and it interpreted JMESPath boolean backticks as intended shell expansion. SES
identity filtering now uses `jq` over the journaled AWS response instead of
ambiguous JMESPath quoting; final lint uses `shellcheck -x` so the repository
helper is analyzed.

## Real-AWS attempts and cleanup

- Run `20260725t035258z-adapters` stopped at SQS creation because AWS CLI v2
  parsed separate shorthand map entries for colon-containing tag keys as
  unknown options. The permanent fix passes one JSON map generated by `jq` for
  both SQS and Cognito tags. The exit trap removed the already-created tagged
  S3 bucket and verified bucket, queue, Cognito pool, bootstrap user, adapter
  role, and local credential files were all absent in
  `target/minco/aws/20260725t035258z-adapters/cleanup.json`. This run is a
  recorded failed attempt, not provider conformance evidence.
- Run `20260725t035356z-adapters` created and validated the generated IAM policy
  (zero Access Analyzer findings) and private static-site CloudFormation
  template, then established the isolated non-root role. The compiled provider
  test stopped before its first adapter call because `MINCO_AWS_RUN_ID` was not
  exported. The first automatic teardown also exposed a more serious harness
  issue: once the non-root `AWS_CONFIG_FILE` was exported, the root wrapper
  changed only `AWS_PROFILE`, so teardown could not load the real root profile.
  A journaled recovery first verified exact run tags on the bucket, queue,
  Cognito pool, bootstrap user, and adapter role, deleted only those resources,
  and recorded all five absence proofs as true in
  `target/minco/aws/20260725t035356z-adapters/recovery-cleanup.json`.
  The permanent fix exports the run ID and isolates root/source/deploy wrappers
  in subshells that unset inherited credential and config variables. An initial
  recovery-audit command was also rerun under Bash after Zsh lacked
  `BASH_SOURCE`; it failed locally before any cloud call.
- Run `20260725t035708z-adapters` reached the first non-root provider call and
  the S3 adapter returned only `dispatch failure`. Automatic cleanup passed all
  six absence checks, proving the profile-isolation fix. The adapter now
  preserves a bounded, control-character-free standard error source chain
  instead of discarding connector diagnostics, and the harness saves the
  redaction-safe test output with backtraces disabled for the next diagnosis.
- Run `20260725t035908z-adapters` used the improved error chain and identified
  the underlying defect before any S3 request left the SDK: the workspace had
  disabled `aws-config`'s `credentials-process` Cargo feature, so Rust SDK
  clients could not load the same isolated non-root profile that AWS CLI
  successfully used. Cleanup again passed all six absence checks. The workspace
  now enables that exact feature; this closes a production authentication gap
  for normal process-backed AWS profiles rather than bypassing it with exported
  credentials.
- Run `20260725t040042z-adapters` authenticated and exercised S3 server and
  signed-POST operations, then found that a missing-key `HeadObject` is reported
  as access denied when the role lacks `s3:ListBucket`. Because the object-store
  contract must distinguish absence from authorization failure, IAM derivation
  now grants only prefix-bounded bucket listing in addition to exact-prefix
  object actions. Its automatic cleanup passed all six absence checks.
- Run `20260725t040256z-adapters` passed every compiled provider conformance
  assertion for S3, SQS, Cognito, and static publication. SES was skipped
  because the account has no pre-existing verified email sender; a journaled
  post-run count confirmed zero verified email identities and zero verified
  domain identities, and the harness did not create or mutate an SES identity.
  Cleanup deleted every resource, but
  its single immediate `HeadBucket` sample still observed the just-deleted
  bucket and therefore conservatively recorded `bucket_absent: false`; a
  journaled follow-up returned `404`. S3 absence verification now uses the same
  bounded retry pattern as SQS, and the immutable original cleanup result is
  retained beside a separate recovery proof.
- Run `20260725t040913z-adapters` passed S3 server-side storage, signed POST
  including enforced size rejection, signed GET, SQS publication, Cognito
  create/get/disable/delete, and static-site S3 publication through the compiled
  adapters. Access Analyzer reported zero errors, security warnings, or
  warnings; CloudFormation accepted the private S3/CloudFront OAC template.
  The corrected cleanup verifier passed on its first S3 and SQS checks, with all
  six absence booleans true. No distribution or SES identity was created, and
  SES delivery remains unexercised because the account has no verified sender.
- Run `20260725t041822z-adapters` repeated the full compiled provider suite after
  the ownership-hardening change. Its unpredictable exact resource names were
  saved in `resources.json`; provider assertions, IAM and template validation
  passed, and all six cleanup/absence booleans are true. This is the final
  real-AWS evidence for this task.
