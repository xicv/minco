---
id: M11-T12
title: Prove subscriber-only AppSync realtime in bounded live AWS
milestone: M11
status: complete
priority: critical
area: plugins/realtime/aws/verification
depends_on: [M11-T11]
operations: []
owned_paths:
  - plugins/minco-plugin-realtime/assets/realtime-client.mjs
  - plugins/minco-plugin-realtime/assets/realtime-client.test.mjs
  - proofs/realtime-appsync/**
  - docs/how-to/realtime.md
  - VERIFICATION.md
  - tasks/M11/M11-T12-live-realtime-proof.md
  - roadmap/tasks.mmd
  - verification/adoption-measurements.json
  - verification/deep-review.json
  - verification/source-manifest.json
  - verification/static-validation.json
checks:
  - proofs/realtime-appsync/scripts/test-local.sh
  - proofs/realtime-appsync/scripts/check-aws-template.sh
  - proofs/realtime-appsync/scripts/test-live-authority.sh
  - ./scripts/quality.sh
---

## Goal

Prove the selected `minco-plugin-realtime` AppSync Events path in one disposable,
approved non-production AWS boundary: exact source, generated Cognito/IAM-only
infrastructure, the real Rust publisher adapter, the packaged browser subscriber,
HTTP resynchronization before buffered event delivery, rejection of a mismatched
subscriber, and complete teardown.

## Authority gate

Before the first AWS API call, the operator must explicitly approve the exact
non-production account, Region, role/profile, stack, source revision, resource
allowlist, maximum duration/spend, and whole-run cleanup blast radius. Local
configuration discovery and qualification do not grant provider authority. An
old login or approval for another task does not authorize this proof.

## Acceptance

- authority validation, exact-source validation, local qualification and artifact
  build all complete before provider contact;
- the runner refuses pre-existing stack or artifact-bucket names and captures the
  immutable stack ID and versioned artifact identity it created;
- CloudFormation creates only one AppSync Events API/namespace, one temporary
  Cognito user pool/client, one publisher Lambda/role/log group, and the external
  versioned artifact bucket owned by the runner;
- AppSync uses the disposable Cognito user pool for connection/subscription and
  IAM only for publication, with the namespace handler binding the exact
  application channel path to Cognito `sub`;
- the real `AppSyncEventsPublisher` accepts one event through the Lambda role's
  exact namespace-scoped `appsync:EventPublish` permission;
- the packaged browser facade subscribes using an in-memory temporary ID token,
  performs HTTP resynchronization before releasing the event, reconnects and
  resynchronizes again, and rejects a token/channel mismatch without exposing
  credentials or provider bodies;
- cleanup proves the exact stack and bucket absent and no token, password, account
  ID, ARN, endpoint, username, or provider payload is committed to evidence;
- local, provider deployment, browser runtime and cleanup results remain separate.

## Non-goals

- production or persistent staging traffic;
- API-key or browser publication support;
- durable delivery, presence, history, offline push or replacing HTTP truth;
- publishing crates, creating a release/tag, modifying shared identity resources,
  or using an existing stack/bucket;
- a general AppSync load, latency, availability or cost benchmark.

## Evidence

Started on 2026-08-04 from exact `main`
`70133c22b9caadf07403df0331425dac157ae475`.

The first approved disposable run reached CloudFormation but failed closed on an
insufficiently scoped log-group permission. The exit cleanup removed the
temporary identity resources; the exact failed stack and empty versioned bucket
were then removed and independently proved absent before any retry.

The second approved run used exact source
`a629093060f53c4c8b94ec3c8cdda15ca01d52ac` and authority digest
`40ca1fb86ec17ce3677c5a390ed1ff7012f393ad346908c32a4dc67ecef20fd5`.
Local qualification and provider deployment passed, but the packaged Chromium
subscriber failed before connecting because an opaque `blob:` context exposed
Web Crypto without `crypto.randomUUID()`. The runner reported `cleanup=passed`;
separate provider queries proved the exact stack absent and the artifact bucket
absent. The temporary proof permissions were restored to their original policy
and reprovisioned, with an exact server-side policy comparison passing.

The remediation uses `crypto.randomUUID()` when available and otherwise derives
a bounded 128-bit operation identifier from `crypto.getRandomValues()`, failing
closed when Web Crypto is unavailable. A unit regression with `randomUUID`
removed and a real Chromium `about:blank` plus `blob:` import regression both
pass. `./scripts/quality.sh` passes after regenerating the qualified source
manifest.

The third approved run used exact source
`020d47ab4cb3f4fb2e5e0a7c6bee760ccb10680a` and authority digest
`640f51816066a3e641a3ffd7b5e2dfbe3bd29ee864f153b2c9fc1271c0e88b67`.
Local qualification, artifact build, provider deployment, and the opaque-context
browser regression passed. The live browser subscriber then failed safely before
its first HTTP truth resynchronization and the Playwright case reached its
unnamed 60-second timeout. The runner reported `cleanup=passed`; independent
queries proved the exact stack and bucket absent. The temporary proof permission
was restored and reprovisioned, with an exact server-side policy comparison
passing.

The proof now uses AppSync's native `AMAZON_COGNITO_USER_POOLS` mode for the
disposable Cognito pool instead of presenting that pool as a generic OIDC
provider. The client regex is exact, and `onSubscribe` compares the complete
requested path to `/orders/${sub}/orders` using the documented Cognito identity
shape. The live browser case now gives each runtime phase a safe bounded error so
provider failures cannot collapse into an unnamed whole-test timeout. Local proof
gates pass. A further provider retry remains blocked until the new exact source
and authority digest are presented and explicitly approved.

The fourth approval bound exact source
`b3824f501c490beeb1d2a644f455966fcee7ee67` and authority digest
`c9c397f37b23c167e8a413329d289cce767aad4316db810d3ffbdfe7ab39052f`.
Its first runner invocation stopped before provider contact because the full disk
prevented JJ from creating its working-copy lock. Clearing only this task
workspace's reproducible root `target/` recovered 24.9 GiB; source and proof
artifacts were unchanged and revalidated before retrying the same authority.

The provider retry proved that native Cognito authorization and the AppSync
subscription now succeed: the packaged subscriber advanced to its first HTTP
truth resynchronization. Chromium then rejected the proof page's opaque
`about:blank` to loopback HTTP fetch, and the new phase diagnostic failed safely
as `Realtime truth resync failed`. The runner reported `cleanup=passed`;
independent queries proved the exact stack and bucket absent, and the temporary
proof permission was restored and reprovisioned with an exact server-side policy
comparison passing.

A local Chromium regression reproduces the opaque-page loopback failure. The
proof harness now serves its page and `/authoritative-state` from one loopback
origin, while the separate `about:blank` plus `blob:` regression continues to
exercise the packaged client's opaque-context operation-ID fallback. Local
browser gates pass. Another provider retry remains blocked until this changed
source is fully qualified and a new exact authority digest is explicitly
approved.

The fifth approval bound exact source
`81393f521682674ccdacab6fafd3375e4ac9b452` and authority digest
`2c33094ea17181f65653b135304c15631d53fd46bd864ba73d4dd67c5c7aecb0`.
The reviewed temporary policy was recovered by its exact CloudTrail event and
matched the previously reviewed SHA-256
`39fef833a88d3d7ca10ee3c9dd6e9331ef3a589007ab6ea0d60bf93bd1dd762f`
before it was applied. Local qualification, artifact build, provider deployment,
Cognito connection, AppSync subscription, and the first HTTP truth
resynchronization all passed. The real publisher accepted the first IAM
publication, after which the array-only browser delivery parser failed closed as
`AppSync Events sent an invalid message` at the first data-frame boundary.

The runner reported `cleanup=passed`; independent provider queries proved the
exact stack absent and the artifact bucket absent. The temporary permission was
restored and reprovisioned under request
`9f647aff-bbae-4ace-a295-0c3f0e87d5f3`, and an exact server-side comparison
proved the original policy restored.

AWS documents the data message `event` as an array of stringified JSON values,
while the live failure is reproduced deterministically by the single
JSON-string delivery form used by Event API clients. The packaged subscriber now
normalizes exactly one string into a one-event batch while retaining support for
the documented string array, rejecting every other shape, and preserving the
existing message, event, and resynchronization-buffer limits. The regression is
red before the change and green after it; all realtime client tests and local
proof gates pass. Another provider retry remains blocked until this changed
source is fully qualified and a new exact authority digest is explicitly
approved.

The sixth approval bound exact source
`2d721700998ad9abf93e4ea855bf938cd1b3a27e` and authority digest
`2ac7d858975ddd9d350a39d854c3174aa6c7e341e4404018a7e0a8bcfade3124`.
The reviewed temporary policy again matched SHA-256
`39fef833a88d3d7ca10ee3c9dd6e9331ef3a589007ab6ea0d60bf93bd1dd762f`
before application, and its provisioned server-side value matched exactly.
Local qualification and the ARM64 artifact build passed. The disposable
provider deployment passed with artifact SHA-256
`2b6e81ead5d33d6ba2a3302dd88730f8e5be79f390d0f8968e9717b321895381`.

The packaged Chromium subscriber then passed the complete live journey: native
Cognito connection and exact-channel subscription, HTTP truth resynchronization
before releasing the first IAM-published event, hidden-page disconnect, visible
reconnect and second truth resynchronization, a second IAM-published event, and
generic rejection of a token/channel mismatch without credential exposure. All
three browser cases passed, including the separate loopback truth-boundary and
opaque-context regressions. Provider deployment and browser runtime were
reported separately as passed.

The runner reported `cleanup=passed`. Independent provider queries proved the
exact stack absent and artifact bucket absent. The original permission was
restored and reprovisioned under request
`a5f14a00-325b-4171-a4db-b4394b73ffb7`, and an exact server-side comparison
proved the original policy restored. The accepted bounded live proof therefore
closes M11-T12; it is verification evidence, not production enablement or
authority to recreate the disposable resources.
