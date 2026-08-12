---
title: Testing and Evidence
description: Test boundaries, authoritative local quality, hosted CI, provider checks, and exact-release evidence.
---

# Testing and Evidence

Minco proves behavior at the nearest meaningful public boundary, then keeps
larger operational claims separate.

## Test Boundaries

| Boundary | What to prove |
|---|---|
| Domain | invariants, value validation, and state transitions with pure tests |
| Application | authorization, validation, and fail-before-persistence with fake owned ports |
| Adapter | real engine transactions, concurrency, idempotency, and rollback |
| HTTP | Axum `oneshot` status, media type, headers, request IDs, and bodies |
| Plugin/core | dependency graph, typed injection, selection, ordering, and provenance |
| Deployment | deterministic Plan/SAM, IAM, wake, cost, and performance structure |
| Release | exact source, artifact digest, manifest, receipt, and registry identity |

## Local Commands

```bash
./scripts/test/unit.sh
./scripts/test/feature.sh
./scripts/docs/build.sh
./scripts/docs/check-links.sh
./scripts/docs/check-snippets.sh
./scripts/docs/test-browser.sh
./scripts/quality.sh
```

The complete local quality runner is authoritative. It includes static truth,
formatting, Clippy, all workspace targets, generated applications, browser
checks, package policy, dependency hygiene, advisory review, secret scanning,
Rustdoc, and deterministic evidence freshness.

Agent release freshness is part of that deterministic boundary:

```bash
cargo test -p cargo-minco --test agent_skills --locked
uv run --locked python scripts/test/agent_workflows.py \
  --check-output verification/agent-workflows.json
```

The first command binds release notes, versioned documentation and all nine
packaged skills. The second reproduces Codex/Claude projection and scenario
evidence byte-for-byte without invoking a model or contacting a provider.

The 1.5 candidate also exposes official provider-free fakes for SQS handling,
domain-event publication, object storage, feedback persistence and rich mail.
Use them only through their owning public ports. Their ordered, redacted
attempt records and one-shot failure scripts prove application behavior; they
do not qualify a production adapter or live provider.

The slower measured assurance lane pins nextest, llvm-cov, cargo-mutants and
cargo-semver-checks, and the golden-topology cost baseline covers seven
reviewed Orders configurations. Both are exact-source local evidence.
Application-specific model evaluation, measured human-review effort, hosted
performance and current live-provider proof remain `NOT RUN` or absent for the
1.5 candidate.

## Hosted Profiles

The manual `essential` GitHub Actions profile adds bounded clean-Linux compiler
and repository-truth evidence for an exact commit. Full qualification remains
local through `scripts/ci/local-release.sh`; Pages and exact-tag crates.io OIDC
publication are the only other GitHub workflow responsibilities.

The retained manual profile does not contact real AWS. Live-provider evidence
requires its own designed command and authorization.

## Evidence Vocabulary

<table class="evidence-table">
  <thead>
    <tr><th>State</th><th>Meaning</th></tr>
  </thead>
  <tbody>
    <tr><td>passed</td><td>The named command ran successfully against the identified source or target.</td></tr>
    <tr><td>failed</td><td>The command ran and did not satisfy its contract.</td></tr>
    <tr><td>not run</td><td>The boundary was deliberately not executed.</td></tr>
    <tr><td>not assessed</td><td>The current test cannot make that claim.</td></tr>
    <tr><td>ignored</td><td>The test is compiled but requires an explicit environment or provider.</td></tr>
  </tbody>
</table>

Static validation is not compiler verification. A package dry run is not
registry publication. Hosted verification is not promotion. Promotion is not
ongoing production health.

## What to Record in a Task

Record the exact command, result, source SHA, tool versions, artifact digest,
and any skipped boundary. Never convert a missing tool, ignored provider test,
or dry run into a pass.
