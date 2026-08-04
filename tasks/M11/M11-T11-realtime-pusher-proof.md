---
id: M11-T11
title: Prove a Rust and AWS Pusher-compatible realtime transport
milestone: M11
status: active
priority: high
area: plugins/realtime/research
depends_on: [M11-T10]
operations: []
owned_paths:
  - proofs/realtime-pusher/**
  - docs/research/realtime-pusher-proof-2026-08.md
  - roadmap/tasks.mmd
  - tasks/M11/M11-T11-realtime-pusher-proof.md
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - proofs/realtime-pusher/scripts/test-local.sh
  - proofs/realtime-pusher/scripts/check-aws-template.sh
  - proofs/realtime-pusher/scripts/test-live-authority.sh
  - uv run --locked python scripts/validate_static.py
  - ./scripts/quality.sh
---

## Goal

Prove, before adding a public Minco extension point, whether unmodified
`pusher-js` can use a bounded Pusher Protocol v7 subset implemented in Rust and
whether API Gateway WebSocket `$connect` timing can deliver the required first
protocol frame reliably without provisioned compute.

## Acceptance

- exact proof dependencies and protocol assumptions are pinned and recorded;
- an unmodified browser build of `pusher-js` opens `/app/<key>`, receives
  `pusher:connection_established` and reaches the connected state against a
  native Axum server;
- public and private subscription, invalid private authorization, application
  ping/pong, sender exclusion and reconnect/resubscribe behavior are exercised
  through browser-visible public behavior;
- the AWS handler boundary completes `$connect` before attempting a management
  callback, checks connection visibility and treats pre-establishment or stale
  connections as retryable/terminal outcomes explicitly;
- a bounded CloudFormation proof declares exact WebSocket routes, Lambda
  functions, DynamoDB state, IAM, retention, quotas and cleanup behavior;
- any live AWS run records exact account, Region, role, stack, source and
  artifact identity, then reports deployment, browser runtime and cleanup as
  separate evidence;
- the proof ends with a clear go/no-go recommendation and identifies which
  contracts require an ADR and two production implementations.

## Non-goals

- shipping or publishing a Minco realtime plugin;
- changing a public Rust API, Plan IR schema or the default application graph;
- claiming full Pusher, Laravel Echo, presence, encrypted-channel, cache-channel,
  client-event, XHR fallback or SockJS compatibility;
- treating local simulation, template validation or compilation as live AWS
  runtime proof;
- retaining an AWS proof stack, enabling production traffic, adding a NAT
  Gateway, fixed compute, schedules or provisioned concurrency.

## Evidence

Active. The user authorized the bounded proof on 2026-08-04 after the dated
research comparison of API Gateway WebSockets, SSE, AppSync Events and native
Rust/fixed-compute alternatives.

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
