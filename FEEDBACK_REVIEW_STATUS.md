# Minco core, plugin-system and Feedback review status

Date: 2026-07-24
Workspace version: `0.2.0`
Candidate bookmark: `agent/feedback-plugin-and-core-audit`

## Conclusion

The Minco core is architecturally strong enough to host the reusable
framework-level capabilities exercised by GarmentIQ and CGSP. The plugin kernel
now provides a stable provider-neutral center:

- statically linked and explicitly selected plugins;
- semantic plugin/core compatibility and versioned capabilities;
- dependency auto-enablement with fail-closed explicit disabling;
- whole-graph validation before service construction;
- strict configuration schemas, defaults and unknown-field rejection;
- typed singleton services and ordered typed multi-contributions;
- deterministic install and finalize phases;
- exact HTTP operation ownership;
- explicit migrations, resources, wake sources, health, cost, stability and
  data-sensitivity metadata;
- application-provided adapters selected only by the composition root.

The review did not introduce dynamic libraries, runtime package scanning,
string-key service location, hidden schedulers or provider dependencies in
`minco-core`.

## Official plugin coverage

Stable defaults:

- health;
- observability;
- idempotency.

Opt-in beta plugins:

- sessions and CSRF primitives;
- identity, scopes, claims and permissions;
- object storage;
- events and explicit outbox ports;
- notifications;
- append-only audit;
- Feedback;
- static-site deployment intent.

The repository catalog also includes SQLx PostgreSQL, SQLx SQLite and native AWS
Lambda extensions. See `docs/architecture/capability-audit.md` for the complete
GarmentIQ/CGSP coverage matrix.

## Feedback vertical slice

`minco-plugin-feedback` includes:

- a framework-independent, Shadow DOM widget and four FAB positions;
- browser-consented screen capture, screenshots, files and microphone audio;
- optional OpenAI-compatible and audited command transcription;
- client/developer threads, private internal notes and clarification states;
- memory, PostgreSQL, SQLite and application-provided stores;
- hashed client tokens and optimistic concurrency revisions;
- object-storage metadata rather than database BLOBs;
- notification, event/outbox and audit integration;
- protected developer routes and `cargo minco feedback` commands;
- deterministic JSON and Markdown AI context;
- a 13-operation OpenAPI contract and committed migrations.

Feedback persistence is authoritative. Post-commit notification, audit or event
delivery failures become warnings and do not erase accepted feedback.
Independent stores are not described as transactionally atomic.

## Hardening completed

Compiler and runtime verification found and fixed:

- stale generated Orders contract bindings;
- optional plugin configuration treating typed `None` as JSON `null`;
- SQLx migration macro features and PostgreSQL `SELECT 1` integer decoding;
- PostgreSQL 18's changed data-volume mount;
- digest-based constant-time project/developer token comparison;
- transcription provider-detail leakage;
- prompt-injection boundaries for client text, transcripts and context;
- modal focus trapping and reduced-motion behavior;
- artifact cleanup when authoritative Feedback persistence fails;
- root-anchored Cargo package includes so nested browser dependencies cannot
  leak into the Feedback crate;
- lock-step `0.2.0` candidate versioning after the immutable `0.1.0` release,
  as required for the public pre-1.0 API changes;
- official-plugin core compatibility requirements derived from the lock-step
  package version instead of a stale hard-coded `^0.1`;
- macOS Bash compatibility in the package-list script;
- current cargo-lambda ZIP output selection;
- `cargo minco explain` tracing for plugin-owned operations;
- static/source scanners traversing generated Node dependencies;
- Clippy errors across all targets and features without lint suppression.

## Verified

The exact evidence is in `VERIFICATION.md`. In summary:

- full Rust format/check/Clippy/test/doc gates pass on Rust `1.97.1`;
- all Feedback feature combinations pass;
- SQLite and real PostgreSQL persistence contracts pass;
- Orders generated-app and TCP E2E checks pass;
- Chromium/Firefox widget runs pass `38/38`, repeated `114/114`;
- cargo-deny, gitleaks and npm audit pass;
- plugin validation, Plan IR and Orders/Feedback operation traces pass;
- native ARM64 Lambda ZIP packaging passes;
- SAM linting plus read-only CloudFormation and IAM Access Analyzer validation
  pass;
- clean-tree package listing and crates.io publication dry run pass;
- no crate was uploaded.

No real AWS deployment or provider-adapter conformance was attempted. The
repository-wide Codex Security Deep Scan is deferred because the external scan
service terminated two defensive runs before returning an acceptable discovery
manifest; the deterministic local deep-review remains green.

## Explicit provider gaps

These remain planned under `M6-T04`:

1. Production S3 object storage and direct-access signing.
2. SQS publication and transaction-integrated PostgreSQL outbox recovery.
3. SES and signed-webhook notification delivery.
4. Product-selected durable session, idempotency and audit adapters.
5. Cognito administrative invitation and user management.
6. S3/CloudFront static-site rendering.
7. IAM derivation and bounded real-AWS conformance for selected adapters.

These are implementation gaps, not reasons to add another core abstraction.
