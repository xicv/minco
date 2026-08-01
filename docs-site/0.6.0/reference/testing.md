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

## Hosted Profiles

The manual `essential` GitHub Actions profile adds clean Linux compiler and
repository-truth evidence for an exact commit. The explicit `release` profile
repeats the larger release matrix, native ARM64 artifact builds, Plan/SAM, and
optional Rustack/E2E checks.

Neither profile contacts real AWS unless a separately designed job and
authorization says so.

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

