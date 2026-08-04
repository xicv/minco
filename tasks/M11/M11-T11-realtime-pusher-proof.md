---
id: M11-T11
title: Implement minimal subscriber-only realtime with AppSync Events
milestone: M11
status: complete
priority: high
area: plugins/realtime/deployment
depends_on: [M11-T10]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - CHANGELOG.md
  - CODEX_HANDOFF.md
  - PUBLISHING.md
  - README.md
  - VERIFICATION.md
  - crates/minco/Cargo.toml
  - crates/minco/src/lib.rs
  - crates/minco-cli/Cargo.toml
  - crates/minco-cli/src/main.rs
  - crates/minco-cli/tests/plugin_cli.rs
  - crates/minco-plan/**
  - docs/DECISIONS.md
  - docs/adrs/0031-subscriber-only-realtime.md
  - docs/adoption/0.6.0-to-0.7.0.md
  - docs/adoption/incremental-adoption.md
  - docs/development/publishing.md
  - docs/development/using-minco-crate.md
  - docs/how-to/realtime.md
  - docs/reference/generated/**
  - proofs/realtime-pusher/**
  - docs/research/realtime-pusher-proof-2026-08.md
  - docs/vision/minco-framework-definition.md
  - docs-site/release.json
  - docs-site/tests/docs.spec.mts
  - examples/plugins/third-party-minimal/**
  - examples/orders/api/src/generated.rs
  - extensions/minco-aws-adapters/**
  - extensions/*/minco-plugin.json
  - infra/aws/generated/plan.json
  - plugins/catalog.toml
  - plugins/*/minco-plugin.json
  - plugins/minco-plugin-realtime/**
  - crates/minco-test/tests/plugin_conformance.rs
  - roadmap/tasks.mmd
  - scripts/source_manifest.py
  - scripts/test/repository_truth.py
  - tasks/M11/M11-T11-realtime-pusher-proof.md
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/publish-validation.json
  - verification/repository-truth.toml
  - verification/rust-dependency-hygiene.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - cargo test -p minco-plugin-realtime --all-features --locked
  - cargo test -p minco-aws-adapters --lib --features appsync-events --locked
  - cargo test -p minco-plan --locked
  - npm test --prefix plugins/minco-plugin-realtime
  - cargo minco plugin validate --json
  - proofs/realtime-pusher/scripts/test-local.sh
  - proofs/realtime-pusher/scripts/check-aws-template.sh
  - proofs/realtime-pusher/scripts/check-appsync-plan.sh
  - proofs/realtime-pusher/scripts/test-live-authority.sh
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/validate_publish.py
  - ./scripts/quality.sh
---

## Goal

Implement the minimal realtime contract justified by the transport proof:
backend-only event publication, subscriber-only browser delivery while the UI
is active, and application-owned HTTP resynchronization after every initial or
re-established subscription. Use a provider-neutral plugin with memory and AWS
AppSync Events implementations, and make AWS infrastructure, IAM, cost and
endpoint output explicit in Plan IR and SAM.

## Acceptance

- ADR-0031 distinguishes ephemeral realtime notification from durable domain
  events and records delivery, authentication, resync, cost and failure rules;
- `minco-plugin-realtime` exposes one use-case-shaped publisher, validates
  provider-portable channels and bounded JSON envelopes, and includes a real
  deterministic memory implementation;
- `minco-aws-adapters` publishes one bounded event per use-case call to the
  AppSync Events HTTP data plane with SigV4, exact endpoint validation, bounded
  responses, transient failure classification and no secret-bearing errors;
- the realtime plugin declares request-driven AppSync resource intent, the AWS
  provider marker supplies the explicit AppSync capability, and the rendered
  plan grants exact `appsync:EventPublish` IAM rather than wildcard resources;
- optional Plan IR realtime intent renders `AWS::AppSync::Api` and
  `AWS::AppSync::ChannelNamespace`, requires OIDC/JWT browser authentication,
  uses IAM-only publication, and emits HTTP/realtime endpoints plus explicit
  connection-minute and 5 KiB operation cost dimensions;
- the browser facade can only subscribe, opens while visible, disconnects after
  the configured hidden grace period, reconnects with bounded jitter, buffers
  live events until the application HTTP resync callback completes, and never
  stores or logs bearer tokens;
- plugin metadata, facade features, package contents, documentation and public
  conformance stay deterministic and archive-visible;
- the prior Pusher/API Gateway proof remains as decision evidence, not as the
  selected production implementation;
- provider deployment, browser runtime and cleanup remain separate live gates.

## Non-goals

- client-side publication, presence, chat history, offline/device push,
  guaranteed delivery or using realtime as the authoritative data store;
- API-key authorization, bearer-token persistence, application heartbeat
  messages or a Lambda/DynamoDB connection registry;
- changing the durable `minco-plugin-events` outbox semantics;
- deploying to AWS, enabling production traffic, publishing crates, or adding a
  NAT Gateway, fixed compute, schedule or provisioned concurrency;
- treating local tests, SAM validation or compilation as live AWS runtime proof.
- tagging or describing the unpublished `0.7.0` source candidate as a registry
  release; the published baseline remains the 28-package `0.6.0` family.

## Evidence

Active. The user authorized the bounded proof on 2026-08-04 after the dated
research comparison of API Gateway WebSockets, SSE, AppSync Events and native
Rust/fixed-compute alternatives.

The user selected the subscriber-only AppSync Events branch on 2026-08-04.
Implementation follows ADR-0031 through red-green vertical slices. The frontend
will be receive-only and active-visibility scoped; missed state is reconciled
through the application's authoritative HTTP API after subscription readiness.

- Research: the dated report records current primary sources, exact dependency
  releases and `ap-southeast-2` AWS Price List results. When keeping
  `pusher-js` is optional, it recommends AppSync Events behind a small Minco
  frontend facade; Pusher compatibility remains the portability/ecosystem
  branch. SSE is rejected as the general pub/sub transport.
- Native runtime: `proofs/realtime-pusher/scripts/test-local.sh` passed on
  2026-08-04. The pinned browser distribution of `pusher-js` 8.6.0 passed eight
  public-behavior Playwright cases against Axum: connect, public subscribe,
  typed event, valid and invalid private authorization, application ping/pong,
  sender exclusion, and reconnect/resubscribe. The one live-AWS test was
  explicitly skipped because no provider endpoint was supplied.
- Rust/AWS boundary: six post-connect state-machine tests and three deployable
  handler boundary tests passed; both Rust packages passed formatting, locked
  tests and Clippy with warnings denied.
- Deployable artifact: `cargo lambda build --release --arm64 --output-format
  zip` passed on the pinned toolchain and produced SHA-256
  `42b7da037abbc5f7a5b1300ba9d76c8040c8f26e0afcab48c8a3d6f43d57e205`.
  The cross-linker emitted the non-fatal warning `ignoring deprecated linker
  optimization setting '1'`; this is recorded as a warning, not promoted to a
  clean build claim.
- AWS structure: the bounded template passed the repository policy checker and
  `sam validate --lint`. It declares one WebSocket API, one arm64 Lambda,
  on-demand DynamoDB with TTL, exact S3 object versioning, bounded async retry,
  narrow callback IAM, one-day Lambda logs, detailed API metrics, explicit route
  throttles and delete policies; it declares no API access-log dependency on
  account-global role state and no NAT, schedule, provisioned concurrency or
  fixed compute.
- Dependency audit: `npm audit --audit-level=high` and `cargo audit` against
  both independent proof lockfiles reported zero vulnerabilities. The AWS
  packages disable legacy default TLS features and use the same current
  `default-https-client` boundary as the main workspace.
- Live authority: the pre-contact authority regression passed for every
  required account, Region, profile, role, stack, source, duration, spend and
  cleanup field, including fail-closed root and non-role caller handling. No
  provider-capable run was performed, no resources were created and exact
  non-root role authority was not supplied. Provider deployment, browser
  runtime and cleanup remain unproved and must be reported separately after a
  future approved run.

The selected AppSync implementation is locally complete as of 2026-08-04:

- the provider-neutral plugin has seven passing Rust tests covering portable
  channels, bounded envelopes, memory publication, composition, packaged
  browser source and public conformance;
- the AWS adapter has seven passing focused tests covering SigV4 request shape,
  exact endpoint identity, provider-size bounds, exact namespace IAM and HTTP
  200 partial-failure rejection without response-body disclosure;
- the dependency-free browser facade has nine passing Node protocol tests for
  receive-only auth, matching regional endpoints, resync buffering, visibility
  teardown, jittered reconnect, AWS keepalive handling, bounded buffers and
  bounded WebSocket messages;
- `minco-plan` has 63 passing unit/integration tests, including fail-before-SAM
  policy validation, OIDC claim/channel authorization code, exact publish IAM,
  endpoint outputs and explicit connection/5-KiB operation pricing dimensions;
- the selected plan fixture rendered through `cargo minco` and passed `sam
  validate --lint`; plugin distribution validation returned an empty finding
  list, and the realtime/plugin, AWS adapter, Plan and all-feature CLI Clippy
  scopes passed with warnings denied.

The selected AppSync implementation and release boundary are locally qualified
as of 2026-08-04 against exact parent
`2ac0c7b4463bcb9d06dafd79688e8852539fdec3`:

- review added fail-before-transport plan validation, a 15-second browser
  connection-acknowledgement deadline and a bounded 10-second AWS publication
  request timeout; focused regression suites now report 8 realtime-plugin, 8
  AppSync-adapter and 10 browser-protocol tests passing;
- the provider-neutral plugin, exact SigV4 HTTP publisher, OIDC/IAM-only SAM,
  exact `appsync:EventPublish` policy, endpoint outputs and explicit
  connection-minute/5-KiB operation cost dimensions pass the task's locked
  Rust, Node, Plan and plugin-validation checks;
- the prior Pusher/API Gateway proof still passes its local Rust and Playwright
  suite, AWS template policy and pre-contact live-authority regression; its
  live-AWS browser case remains explicitly skipped because no provider endpoint
  was supplied;
- `minco-plugin-realtime` is the first-publication package in the coordinated
  29-package unpublished `0.7.0` candidate. Static and publish validators pass
  with zero findings while retaining the immutable 28-package published
  `0.6.0` baseline;
- `./scripts/quality.sh` passed end to end on the pinned toolchain, including
  repository/static/publish truth, generated reference, Rust and JavaScript
  tests, desktop/mobile browser checks, all-feature Clippy with warnings denied,
  generated PostgreSQL/SQLite applications, rustdoc, dependency/advisory audits,
  gitleaks and the final deterministic source-manifest check.

The final qualified source-tree digest is recorded in
`verification/source-manifest.json` and bound by the adoption report. No crate
was tagged or published, no AWS provider was contacted, no resource was created
or changed, and live provider/browser, cleanup and production readiness remain
separate unproved gates.

The first exact-head hosted essential run, GitHub Actions run `30893275719`,
failed at the final source-manifest check because the local ignored AppSync Plan
render was present in the committed digest but absent from the clean runner.
The manifest now excludes that exact reproducible output directory and a
regression test covers the clean/local boundary. This failure was treated as a
merge blocker, not as qualified evidence.
