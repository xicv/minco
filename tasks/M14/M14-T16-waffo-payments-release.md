---
id: M14-T16
title: Release the Waffo Pancake payments integration
milestone: M14
status: complete
priority: critical
area: plugins/payments/release
depends_on: [M14-T15]
operations: []
owned_paths:
  - Cargo.toml
  - Cargo.lock
  - CHANGELOG.md
  - README.md
  - CODEX_HANDOFF.md
  - PUBLISHING.md
  - REVIEW_STATUS.md
  - VERIFICATION.md
  - quality.toml
  - crates/**/Cargo.toml
  - extensions/**/Cargo.toml
  - plugins/**/Cargo.toml
  - examples/**/Cargo.toml
  - examples/plugins/third-party-minimal/Cargo.lock
  - proofs/realtime-appsync/aws-handler/Cargo.lock
  - crates/minco/Cargo.toml
  - crates/minco/src/lib.rs
  - crates/minco-cli/assets/agent/**
  - crates/minco-cli/src/agent_cmd.rs
  - crates/minco-cli/src/delivery_evidence.rs
  - crates/minco-cli/src/handover_cmd.rs
  - crates/minco-cli/tests/agent_skills.rs
  - crates/minco-cli/tests/agent_cli.rs
  - crates/minco-cli/tests/plugin_cli.rs
  - plugins/minco-plugin-payments-waffo/**
  - plugins/catalog.toml
  - extensions/**/minco-plugin.json
  - plugins/**/minco-plugin.json
  - infra/aws/generated/plan.json
  - infra/aws/generated/template.yaml
  - docs/adrs/0039-waffo-payment-boundary.md
  - docs/adoption/1.2.2-to-1.3.0.md
  - docs/adoption/incremental-adoption.md
  - docs/development/publishing.md
  - docs/development/using-minco-crate.md
  - docs/reference/compatibility.md
  - docs/reference/generated/**
  - docs/reference/supported-matrix.md
  - docs/vision/minco-framework-definition.md
  - docs/DECISIONS.md
  - docs-site/**
  - roadmap/**
  - scripts/test/agent_workflows.py
  - scripts/test/candidate_qualification.py
  - scripts/test/hosted_ci_policy.py
  - scripts/test/operational_evidence.py
  - scripts/test/repository_truth.py
  - scripts/source_manifest.py
  - scripts/validate_operational_evidence.py
  - scripts/validate_static.py
  - tasks/M14/M14-T16-waffo-payments-release.md
  - verification/**
checks:
  - cargo test -p minco-plugin-payments-waffo --all-targets --all-features --locked
  - cargo clippy -p minco-plugin-payments-waffo --all-targets --all-features --locked -- -D warnings
  - cargo test -p cargo-minco --test plugin_cli --locked
  - cargo test -p cargo-minco --test agent_skills --locked
  - uv run --locked python scripts/test/agent_workflows.py --check-output verification/agent-workflows.json
  - uv run --locked python scripts/test/hosted_ci_policy.py
  - uv run --locked python scripts/test/repository_truth.py
  - uv run --locked python scripts/validate_static.py
  - uv run --locked python scripts/validate_publish.py
  - scripts/docs/generate-reference.sh --check
  - scripts/docs/check-snippets.sh
  - scripts/docs/check-links.sh
  - scripts/docs/test-browser.sh
  - scripts/ci/local-release.sh
  - uv run --locked python scripts/source_manifest.py --check
---

# M14-T16 - Release the Waffo Pancake payments integration

## Goal

Recover draft PR #125 onto current `main`, harden and release one opt-in,
statically composed Waffo Pancake integration as the lock-step Minco `1.3.0`
family. The release must keep payment-provider authority explicit, preserve
application-owned billing state, update the portable agent guidance and frozen
documentation product, and separate offline qualification from live-provider
proof.

## Acceptance

- `minco-plugin-payments-waffo` supplies signed actions, typed hosted checkout,
  read-only GraphQL, raw-body webhook verification, strict configuration,
  stable JSON CLI automation, deterministic fakes and a complete descriptor;
- production and test provider environments cannot be confused, secrets remain
  opaque until an explicit operation, production writes require the persisted
  guard, and no hidden retry, poller, scheduler or always-on control plane is
  introduced;
- session bearer tokens are not persisted through generic idempotency records,
  signed requests never follow redirects, and token-bearing checkout URLs fail
  closed on unsafe scheme, credentials or malformed origin data;
- the custom test-origin seam remains explicit, trusted-operator-only and
  unavailable to production credentials;
- the prohibited task-specific workflow from the interrupted branch is absent;
  the exact three-workflow ADR-0038 allowlist remains enforced;
- facade selection, plugin catalog, distribution metadata, package archives,
  generated references and the complete lock-step family agree on `1.3.0`;
- the Waffo-focused skill is version-matched, projected for Codex and Claude,
  and covered by the cumulative release-feature gate together with every
  existing skill;
- the versioned `1.3.0` manual, changelog, upgrade guide and release inventory
  explain the provider, security, compatibility, cost, recovery and evidence
  boundaries;
- exact-tree local qualification and clean-Linux compatibility pass before
  merge, tag or publication; and
- tag, GitHub release, registry publication, docs.rs, Pages and live-provider
  proof are verified as separate states.

## Non-goals

- generic billing, subscription, entitlement, invoice or transaction domain
  models;
- a provider-agnostic payment facade frozen from one implementation;
- live Waffo payment mutations, customer data, provider credentials or a claim
  of production readiness from offline tests;
- AWS deployment or new AWS resources; or
- temporary, task-specific or branch-only GitHub workflows.

## Recovery

PR #125 head `962d093f520fd894cbf1a4be344c8c7899bd9e08` was preserved locally before
recovery. It was based on `80c0bc71dc21252e853e960745ed984a1a4fe9f5`,
conflicted with published `main` `6dda1a87771d5c99a6dd4f35c27f08f4a802192c`,
reused an allocated task identifier, retained stale `1.2.0` release evidence,
and added a workflow prohibited by ADR-0038. This isolated JJ workspace starts
from current `main`; only the reviewed payment implementation is recovered,
then current release, documentation and evidence truth are regenerated.

## Evidence

Provider review used the official Waffo Pancake Go SDK `v0.9.0` at exact
revision `799135cbe07c45819da0ab4bf777c64fcc956220`. Exact merged release source
`e1fbb066e9332a2b6355b11a6f4b1c28806cc3e5`, tree
`cddd64160b6d3aeff80dd11af18e2f11541a36aa` and source-tree digest
`d92a7b8e625aebdecb7ef155db97ed613d210c62a2c10e467ad8f9056d67d0a1`
passed the complete local macOS release gate and exact-main clean-Linux run
[`31451883403`](https://github.com/xicv/minco/actions/runs/31451883403).

Immutable tag `v1.3.0` resolves to that exact source. The authenticated local
publisher uploaded the dependency-ordered family; crates.io rate-limited the
final two packages after 32 accepted uploads until its explicit retry time, so
recovery republished only the missing `minco` and `cargo-minco` complement.
Independent validation then found all 34 exact versions present and non-yanked.
The [`v1.3.0` GitHub release](https://github.com/xicv/minco/releases/tag/v1.3.0)
is published from the same tag. All 34 exact 1.3.0 docs.rs rustdoc routes
subsequently returned HTTP 200.

Post-publication truth PR
[`#146`](https://github.com/xicv/minco/pull/146) passed exact-head clean-Linux
run [`31457619990`](https://github.com/xicv/minco/actions/runs/31457619990)
at `88f57393691297397a4673a0974c82387d0523e9`, then merged with exact reviewed
tree `3de7375ec5fdc5ec16ea240a4a142c33ff0a6c17` in merged-main commit
`f46304d4c59061a1d4c118681eac45de748aadd4`. Merged-main Pages run
[`31457889688`](https://github.com/xicv/minco/actions/runs/31457889688) built,
checked and deployed the stable site. Live checks returned HTTP 200 with the
expected content for the root, frozen `/1.3.0/` manual, versions, Waffo
payments, local development, files/static sites, events/notifications/mail,
plugins and AI-agent guide routes. These attainable lanes close M14-T16.

No live Waffo request or payment is part of this release; unavailable sandbox
evidence remains `NOT RUN` and no AWS application deployment or production SLO
is claimed.
