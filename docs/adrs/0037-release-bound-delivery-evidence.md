# ADR-0037: Release-bound delivery evidence

- Status: Accepted
- Date: 2026-08-07
- Owners: Minco maintainers

## Context

Fast review deployments create commercial value only when a client-visible
outcome can be traced to the exact software that produced it. Minco could seal
and deploy immutable releases and collect feedback, but it did not close the
authority chain from a successful deployment through feedback-derived work and
a reproducible client handover. Performance and provider observations could
also be misread as current when they belonged to another tree or runner.

The required chain is:

```text
exact source -> release manifest -> successful deployment receipt
  -> server-bound feedback -> digest-approved task + immutable receipt
  -> verified implementation -> performance/provider evidence
  -> deterministic client handover
```

Client text, labels, attachment names and browser context are untrusted.
Release/deployment files, repository source authority and plan digests become
authoritative only after independent verification.

## Decision

### Exact release and feedback binding

The Feedback server stamps one `FeedbackReleaseBinding`, overwriting client
release/environment labels. Task conversion rejects a missing, malformed,
duplicate or conflicting marker. It verifies both input files independently,
requires terminal success, and compares the supplied release manifest's path,
byte length and SHA-256 with the `FileDigest` in the deployment receipt.

Clarification is durable state, not punctuation inference. Moving to
`needs_clarification` binds the latest client-visible developer message ID; a
client reply records the resolving message ID. `ready_for_development` rejects
any open clarification.

### Plan then apply

`cargo minco feedback task` and `cargo minco handover` are read-only by
default. Their canonical payloads bind every authoritative input and output
path. Apply requires the exact lowercase SHA-256 of the current payload; any
changed input invalidates approval.

Task conversion computes direct pre-mutation tree authority rather than
trusting the checked-in source manifest. It excludes only the exact planned
task (plus canonical generated-evidence exclusions), allowing an immediate
byte-identical rerun without broadly excluding repository tasks.

Handover requires the checked-in manifest to match direct full-tree authority:
file set, per-file digests/sizes, aggregate digest, version and closed
exclusions. The validator emits a deterministic operational-validation receipt
that binds its current source digest and every policy, ledger, baseline and
provider qualification receipt it accepted. Handover requires this strict PASS
receipt, rechecks each bound byte, verifies the receipt seal and directly
validates the evidence invariants needed for its truth classifications. The
published CLI never executes repository-controlled Python during read-only
planning; the canonical Python validator remains a developer/CI generator and
checker. This tranche accepts the committed `NOT RUN`, `stale`, `deferred` and
missing-provider states. It deliberately rejects positive performance `PASS`,
provider `current` and capability `supported` claims until the compiled
handover verifier covers the complete hosted provenance, sealed provider
receipt, freshness, artifacts, cleanup, implementation and test policy. An
unkeyed self-resealed receipt can never create a positive handover claim. A
stale, malformed or contradictory receipt fails planning.
Generated baseline, operational-validation, static-validation, deep-review,
publish-validation, handover, feedback and sealed provider receipts are
excluded narrowly and separately bound to avoid self-reference or a local
quality-run digest cycle. Policies, validators, ledgers, ADRs and research stay
inside source authority.

Default handover files use `verification/handover.json` and `.md`; explicit
alternatives must stay under the source-excluded `verification/handover/`
directory. No other arbitrary verification path is excluded.

### Transactional create-only publication

Task/receipt and JSON/Markdown pairs use fd-relative publication on macOS and
Linux. Path components are opened no-follow; missing parents are created from
retained directory handles. Both files are staged create-new and synced before
installation. No-replace rename and root/parent/file identity checks prevent
escape and replacement. A later failure rolls back only the inode created by
this operation through the retained parent handle. Rollback ambiguity is
reported, never hidden with a path-based delete.

Exact existing bytes are idempotent; different bytes, symlinks and unsupported
platforms fail closed.

### Untrusted data and secrets

Generated frontmatter uses neutral metadata. Client title/priority/body remain
inside a marked untrusted section. Export has message/byte bounds. Page URLs
must be server-redacted HTTP(S) scheme/authority/path values without query,
fragment or user information. Credential-shaped URLs, bearer/database URLs,
private keys, AWS key material and the exact operator token are rejected.

Handover contains identities, digests, classifications and limitations—never
feedback bodies, attachments, private logs, arbitrary environment-variable
values, credentials or customer content.

Source authority is traversed from retained directory descriptors. Every child
is opened no-follow and its descriptor identity and stable metadata must still
match the parent entry after reading. Raw feedback attachments are create-only
but additionally confined below `target/minco/feedback-attachments/`; they
cannot be materialized as Cargo source, configuration or repository evidence.

### Operational evidence

The performance policy declares topology, hosted runner scope, sample minimums
and per-metric absolute/relative budgets. Evidence must use finite values,
monotonic `minimum <= p50 <= p95 <= p99 <= maximum`, consistent counts/error
rate, warm/cold classification, environment fingerprint and exact source.
Zero-to-zero is finite zero; zero-to-positive is unbounded and fails.
`production_slo` and provider contact are explicitly false.
Hosted runner provenance must also repeat the canonical source-tree digest and
match the current verified manifest; a self-consistent Git revision alone is
not accepted as source authority.

The candidate record is a strict `PASS`/`NOT RUN` union. M14-T10 records
`NOT RUN`; local Mac timings are not promoted to hosted or production evidence.

Provider records include source scope, observed/reviewed time, Region, bounded
account scope, dimensions, cleanup, retained resources, evidence digests,
maximum age and limitations. Freshness uses the reviewed repository effective
date (or a reported CLI override), never wall-clock time. Historical evidence
remains visible but cannot qualify another tree. Requiring current provider
proof fails when it is absent. A `current` row cannot name the self-referential
ledger tree as its authority: it must point to an immutable create-only
qualification receipt whose sealed payload binds the exact current source
digest, profile fields, evidence artifacts and cleanup result.

AWS candidates remain declared/research/deferred/rejected until implementation,
security, cost, performance, recovery and live proof all exist. Upstream
availability is not Minco support.

### Selected-topology cost

Cost estimates represent the selected deployed topology. Local parity can
retain worker/queue/schedule structure, but it does not report AWS SQS,
Scheduler, AppSync or provisioned-concurrency charges. Lambda HTTP API retains
API Gateway and Lambda rates. Function URLs stay declared/unsupported and do
not invent API Gateway charges.

No scheduler, poller, telemetry collector or always-on Minco control plane is
introduced.

## Compatibility

CLI commands are additive. Plan IR serialization is unchanged, but invalid
runtime/ingress pairs fail earlier and local cost JSON stops projecting AWS
provider dimensions. `FeedbackThread` adds defaulted `clarifications`; old
records deserialize empty. This is JSON-compatible but is a Rust source change
for downstream exhaustive struct literals; callers must add the field or use
constructors. Automatic `?` inference is removed, URL validation is stricter,
and receipt schema 2 binds exact release bytes plus mandatory source authority.
The 1.2 family remains unpublished.

## Recovery and rollback

- Plan-only and wrong-digest paths write nothing.
- Exact reruns are byte-idempotent; conflicts require reconciliation.
- Second-install failure rolls back the first created inode.
- Evidence can be downgraded to `NOT RUN`, `stale` or `deferred`; it cannot be
  strengthened without proof.
- Rolling back packages preserves immutable receipts as audit evidence and does
  not undo deployment, promotion or migration.

## Rejected alternatives

- Trust client labels/priority or a stale checked-in source digest.
- Infer questions from punctuation or exclude all generated tasks from source.
- Apply without digest approval; overwrite outputs; follow symlinks; path-delete
  on rollback.
- Treat local timing as hosted/provider evidence or production SLO.
- Qualify a new release with historical AWS proof.
- Add a generic AWS facade, poller or control plane because a service exists.

## Consequences

The delivery loop is reproducible and incomplete proof stays visible.
Operators must explicitly refresh hosted/provider evidence. Stricter URL and
clarification behavior is documented in the 1.1-to-1.2 guide.
