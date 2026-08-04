# ADR 0030: Repository-native project view and local workbench projections

## Status

Accepted

## Context

Minco already exposes canonical OpenAPI, application and plugin graphs,
roadmap and task records, typed configuration provenance, migration and seed
plans, deployment and cost plans, immutable release evidence, and Feedback.
These sources answer different questions and intentionally carry different
assurance. They are currently inspected through separate commands and files.

Application teams need a coherent way to understand the system, navigate rich
architecture and dependency diagrams, see which features exist, distinguish
planned work from verified behavior, and consume the same explanation through
accessible or read-aloud presentation. A dashboard that reparses prose or
maintains its own completion state would drift from the repository and could
misrepresent source work as deployed or runtime-verified behavior.

The first-party CGSP adoption provides a useful generality test, but CGSP
feature names, roles, commercial decisions, release policy and UAT meaning are
application concepts. They must not become Minco framework vocabulary.

## Decision

Minco will define a versioned, bounded `ProjectView` read model during
M12-T01. The model is a deterministic projection over existing authoritative
read interfaces. It does not own project state and cannot mutate a source.
M12-T01 exposes the same model through the local read-only MCP boundary;
M12-T02 consumes it through `cargo minco workbench` and optional local static
assets. M12-T01 depends on this accepted design task so implementation cannot
start from the older, less-specific workbench brief alone.

The first schema will contain these conceptual sections:

- project identity and schema version;
- source authorities and provenance digests;
- architecture, resource, operation, milestone, task and feature nodes;
- typed edges between those nodes;
- raw source statuses and explicit project-owned semantic status mappings;
- independently reported evidence lanes;
- deterministic summaries and accessible descriptions for visual and spoken
  presentation;
- diagnostics for missing, invalid, stale, truncated or unsupported inputs.

The required evidence lanes are:

- `source`: repository implementation or documentation state;
- `local_verification`: compiler, test and local quality evidence;
- `hosted_verification`: hosted CI or separately controlled verification;
- `deployment`: infrastructure or release application evidence;
- `runtime`: observation from the running target;
- `review`: product review, acceptance or UAT evidence.

An item in one lane never upgrades another lane. Every evidence item retains
its source, exact subject identity where available, observed state and explicit
absence or freshness limit. Aggregates are labelled derived values and include
their denominator and status mapping; they are never stored as a competing
authoritative completion state.

Raw status values remain intact. A project may declare a closed mapping from
its vocabulary to display classes such as `not_started`, `active`, `blocked`,
`complete` and `unknown`. An unmapped value is `unknown` and produces a
diagnostic rather than being guessed. Minco task readiness continues to use its
own task contract and is not redefined by the presentation mapping.

The initial built-in readers use only Minco's declared canonical paths, bounded
directories and typed models. There is no discovery outside those declared
roots, arbitrary Markdown table parsing, shell execution or runtime plugin
discovery. An application-specific feature catalog or evidence adapter remains
application-owned and must produce the same bounded schema with provenance.
Minco will not freeze a general adapter API until the Minco repository and at
least one first-party application exercise it without introducing product
concepts into the framework.

The initial local MCP transport is child-process stdio and opens no listening
socket. A future network transport is a separate compatibility and security
decision rather than an implicit side effect of enabling the read model.

The planned CLI surface is:

```text
cargo minco workbench --check --json
cargo minco workbench export --format json|mermaid|static --output PATH
cargo minco workbench serve
```

`--check` validates inputs and projections without writing. `export` is the
only planned output-writing operation and requires an explicit normalized,
project-relative, non-symlink destination outside every canonical input. The
destination must not exist; export publishes one complete generated directory
atomically and never replaces or deletes unrelated content. `serve` binds to
loopback, serves only generated assets and bounded read models, and performs no
repository, database, provider or application mutation. Exact argument and
serialized-schema compatibility is frozen only by the implementing M12 tasks.

Rich diagrams and narration are projections of the same `ProjectView`.
Diagram labels, summaries and accessible descriptions are escaped untrusted
text. Read-aloud support uses accessible document structure or an explicit
client-side speech capability over displayed text. Minco core does not contact
a text-to-speech provider, generate audio, store voice data or treat narration
as separate project truth.

## Consequences

- Humans, agents, MCP clients and the local workbench share one read model.
- A visualization can explain where a statement came from and what it does not
  prove.
- Applications retain their business taxonomy and release/UAT policy.
- The workbench can evolve independently of core runtime and deployment
  crates, and its static assets add no default facade dependency.
- New readers and adapters require bounded schemas, provenance, limits and
  cross-application evidence instead of generic filesystem access.
- Project status cannot be edited from the workbench; changes continue through
  the owning roadmap, task, contract, release or review workflow.

## Compatibility

`ProjectView`, its evidence vocabulary, the MCP tools and the workbench CLI are
planned pre-1.0 compatibility surfaces. M11-T10 records only their design. The
M12 implementation tasks must version serialized output, document additive and
breaking changes, and keep applications without workbench configuration
compatible with the existing Minco CLI.

## Safety

Every reader requires an explicit canonical project root, permits only declared
project-relative inputs, rejects traversal and unsafe symlink boundaries, and
enforces per-file and total response limits. Secret values, credentials,
tokens, service instances, arbitrary attachments and customer data are not
read. Text is untrusted data and is never executed or rendered as raw HTML.

The local server binds directly to a loopback address, rejects non-loopback
`Host` values and browser origins other than its served loopback origin, and
does not enable permissive CORS. Served pages use local assets, a restrictive
Content Security Policy and `Cache-Control: no-store`; rendered repository text
is never executable markup. The server includes no hosted telemetry and
exposes no write, shell, database, deployment or provider action. Any future
write capability requires a separate ADR, explicit local grant and independent
review.
