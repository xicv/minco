---
id: M14-T63
title: Explicit plan binding for the inbound mail wake chain
milestone: M14
status: active
priority: high
area: plan
depends_on: [M14-T62]
operations: []
owned_paths:
  - crates/minco-plan/src/inbound_mail.rs
  - crates/minco-plan/src/lib.rs
  - crates/minco-plan/src/model.rs
  - crates/minco-plan/src/sam.rs
  - crates/minco-plan/src/cost.rs
  - crates/minco-plan/tests/render_inbound_mail.rs
  - docs/DECISIONS.md
  - docs/adrs/0065-inbound-mail-plan-binding.md
  - tasks/M14/M14-T63-inbound-mail-plan-binding.md
  - verification/1.9-performance-baseline.json
  - verification/source-manifest.json
  - verification/operational-evidence-validation.json
  - verification/static-validation.json
  - verification/publish-validation.json
  - verification/deep-review.json
checks:
  - cargo test -p minco-plan --locked
  - cargo clippy -p minco-plan --all-targets --all-features --locked -- -D warnings
  - cargo minco check --with-cargo
---

# M14-T63 - Explicit plan binding for the inbound mail wake chain

Stage D2 slice 3b part 3. The live seam (M14-T62) proved the chain; this
task renders the provider side explicitly in the Plan with IAM, cost
assumptions and first-class wake sources. No provider contact.

## Goal

- `minco_plan::inbound_mail` sidecar: explicit `InboundMailTopology` →
  `apply` (synthesize wake queue + worker SQS trigger, idempotent;
  bindings recorded on `DeploymentPlan.inbound_mail`), `validate`
  (stable `MINCO-MAIL-001..013`, fail-closed on unknown/mis-roled
  workers, shared queues, duplicate ids, unbounded fields),
  `estimate_inbound_mail_cost` (explicit per-binding assumptions, no
  invented prices), `render_sam_with_inbound_mail` (raw bucket with
  prefix-filtered `ObjectCreated:*` notification and lifecycle retention,
  SES-only `s3:PutObject` bucket policy, S3→SQS queue policy conditioned
  on the bucket ARN, SES receipt rule set/rule with scanning disabled).
- Worker policy gains read-only `s3:GetObject` on the raw bucket; SES
  stays the only writer.
- Structural tests cover synthesis idempotence, every rejection code,
  cost assumptions, the full rendered chain, and unchanged output for
  disabled topologies.

## Non-goals

- Provider contact or deployment; application-config TOML surface for
  bindings (the sidecar API is the composition surface, mirroring
  durable work); DLQ defaults for wake queues (redelivery is the retry
  authority per ADR-0060).

## Evidence

Run 2026-08-25 in the `minco-task-m14-t63` workspace:

- `cargo test -p minco-plan` — ok (6 new tests in
  `render_inbound_mail`: synthesis idempotence, sound/broken validation,
  duplicate ids, invalid fields, cost assumptions, full-chain rendering,
  disabled-topology byte-identity).
- `cargo clippy -p minco-plan --all-targets --all-features --locked --
  -D warnings` — clean; `cargo fmt --all -- --check` clean.
- `sam validate --lint` NOT RUN: the `sam` CLI is unavailable in this
  workspace (`command -v sam` empty). The rendered template is written
  to a temp artifact by the rendering test for external validation; this
  is recorded as a gap, not converted to a pass.
- Evidence chain: static/publish validation, source manifest stable,
  baseline re-bound, operational evidence PASS, deep review rerun.
- `cargo minco check --with-cargo` — result recorded at finish.
