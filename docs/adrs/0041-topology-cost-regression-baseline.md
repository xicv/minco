# ADR 0041: Guard golden-topology cost projections as reviewed evidence

## Status

Accepted.

## Context

Plan IR makes fixed resources, request dimensions, schedules, queue mappings,
worker connection pressure and missing regional rates inspectable before
deployment. Unit tests protect individual decisions, but Minco did not retain a
portfolio-level view showing whether the complete golden application topology
changed across a framework update. A locally valid code change could therefore
add a cost class, remove a required regional rate or make an incomplete profile
appear complete without one review surface showing the cross-profile effect.

## Decision

Minco retains a canonical cost projection for seven reviewed Orders
configurations: local SQLite, Neon Free, Neon Launch, Aurora Serverless v2,
provisioned RDS, self-hosted PostgreSQL and DynamoDB on-demand.

The generator invokes the existing `cargo minco cost --json` command without a
shell. Each record binds the exact configuration bytes, the complete
machine-relevant projection and its SHA-256. The only excluded CLI field is its
human explanatory note; database limitations and dated pricing notes remain in
the baseline because they affect the authority of the estimate. The normal
quality lane checks exact bytes and never regenerates the baseline implicitly.

The report states `provider_contact = false` and `production_budget = false`.
Missing regional rates and eligibility-dependent estimates remain visible. A
baseline change is a prompt for code and architecture review, not approval of a
provider bill.

## Security and integrity

Configuration paths are fixed repository descendants. Symlinked inputs,
observed before/after identity drift, malformed or duplicate records, duplicate
keys, non-canonical JSON, non-finite numbers, digest mismatch and unsafe output
paths fail closed with stable `COST-REGRESSION-*` diagnostics. The validator
executes only the repository-built CLI at `target/debug/cargo-minco`, with a
stripped deterministic environment and bounded timeout; it does not accept an
arbitrary command or contact AWS. This local same-user quality gate does not
claim to defend a working tree from a process that can coordinate replacements
during both reads and CLI execution.

## Compatibility

This is a repository quality contract. It changes no Plan IR schema, public
Rust API, CLI output, plugin compatibility declaration or deployment renderer.
Intentional cost-model changes regenerate one reviewed baseline in the same
change as their tests and documentation.

## Consequences

- topology-wide cost drift becomes reviewable and deterministic;
- explicit configuration prices are protected, but live provider prices are
  not fetched or inferred;
- a complete local estimate is not promoted to an AWS or production claim;
- the gate adds no deployed resource, schedule, poller or control plane; and
- later golden applications can be added only with stable IDs and reviewed,
  non-secret configurations.

## Alternatives rejected

Embedding an AWS price catalog would create freshness and account/Region
authority that this task cannot prove. Comparing only a digest would make a
reviewer unable to see which cost dimension changed. Running a hosted pricing
service would violate Minco's local-first and no-control-plane decisions.
