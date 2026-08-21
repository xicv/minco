# Changelog

All notable changes to Minco will be documented here. The project follows
Semantic Versioning once public releases begin.

## [Unreleased]

## [1.11.0] - 2026-08-21

This additive lock-step minor makes reviewed OpenAPI request assertions an
executable, bounded Rust delivery boundary. It preserves application-owned
business authorization and invariants, adds no provider resource, and keeps
contracts without the opt-in profile unchanged.

### Added

- Added opt-in OpenAPI-derived request validation with bounded direct generated
  checks, single-pass typed Axum extractors, separate coarse authorization
  policies, exact identity scope mapping and Orders place/update integration.

### Fixed

- Hardened request IDs before tracing and final Problem reflection, and replaced
  ambiguous timeout/body-limit response inference with explicit Minco-owned
  streamed-limit and timeout boundaries that preserve application responses.

### Compatibility and cost

- Preserved published public struct fields, constructors, serialized Problems
  and non-opted generated DTO behavior. Added no AWS resource, wake source,
  fixed compute, schedule, hosted service, deployment or provider operation.
- Advanced all 36 publishable packages and official plugin descriptors in
  lock-step to the additive `1.11.0` candidate line, with a frozen manual and
  version-matched nine-skill Codex/Claude bundle.

## [1.10.0] - 2026-08-20

This additive release introduces a portal-first Ticketing support boundary
without changing existing Feedback public names or enabling a runtime,
database, provider, schedule, or fixed compute by default. All 36 exact package
versions are published; future trusted publishing, deployment, and production
remain separate evidence states.

### Added

- Added the public `minco-interaction` crate for bounded support context,
  attachments, transcription, static transitions, and explicitly best-effort
  post-commit activity helpers.
- Added the beta `minco-plugin-ticketing` crate with project-scoped tickets,
  requester-safe projections, atomic one-time handoffs, external-message
  idempotency, explicit SQLite persistence, OpenAPI, and static composition.
- Added a packaged framework-neutral support launcher with modal and tab modes,
  fragment-only handoffs, exact message shapes, keyboard focus containment,
  mobile full-screen layout, reduced motion, and real Chromium/Firefox tests.

### Changed

- Moved provider-neutral transcription primitives out of Feedback while
  preserving every existing Feedback public name and Cargo feature through
  compatibility re-exports.
- Published the 36-package `1.10.0` family with `minco-interaction` and
  `minco-plugin-ticketing` crossing their first-publication ownership boundary.

### Compatibility and evidence

- Kept trusted requester identity and permissions server-derived, browser
  context bounded and untrusted, internal notes private, and first handoff
  consumption atomic with the authoritative ticket/session result.
- Added no portal hosting, mailbox polling, provider request, deployment,
  publication, production mutation, scheduler, NAT Gateway, fixed compute, or
  provisioned concurrency; current evidence is local and provider-free.

## [1.9.0] - 2026-08-19

This additive lock-step minor adds AWS-native ingress traffic control and a
hardened dynamic response-compression boundary. It changes no existing public
API or serialized contract and adds no package, resource or always-on
topology. It remains unpublished until exact-source qualification, merge,
tagging and registry publication complete as separate evidence states.

### Added

- Added an opt-in API Gateway HTTP traffic policy that maps one default
  request-rate/burst target plus canonical operation-ID overrides to
  `DefaultRouteSettings` and `RouteSettings` on both the `$default` and
  candidate stages, resolving against the reviewed deployment plan and failing
  closed on unknown operations, duplicate routes, repeated operation IDs,
  invalid budgets and ambiguous rendered stage markers.
- Added a hardened negotiated response-compression boundary: fastest-level
  gzip for known-size responses of at least 1 KiB with Tower HTTP's
  content-type exclusions composed, an explicit per-response
  `DisableResponseCompression` opt-out for BREACH-sensitive representations,
  and proven `lambda_http` binary-body transport for compressed responses.

## [1.8.0] - 2026-08-14

This additive lock-step minor makes the object-storage plugin ready for
cost-aware browser and mobile transfer control flows. It remains unpublished
until exact-source qualification, merge, tagging and registry publication
complete as separate evidence states.

### Added

- Added an opt-in authenticated object-transfer HTTP control plane for direct
  single or multipart upload, immutable conditional update, private full/range
  download, abort and authorized conditional cache metadata.
- Added provider-neutral bounded streaming, strong-validator range resume and
  checksummed multipart contracts with a production-targeted private S3 byte
  plane and explicit non-S3 conformance boundary.
- Added quarantined completion, application-selected inspection verdicts and a
  structural transfer-cost projection covering storage, incomplete parts,
  requests, egress, acceleration and optional edge dimensions without embedding
  changing provider prices.

### Fixed

- Bounded the complete 10,000-part manifest to a 3 MiB JSON control-plane body
  and each provider part `ETag` to 64 bytes so valid S3 completion fits the
  golden synchronous Lambda and API Gateway ingress limits.
- Implemented authorized GET `If-None-Match` weak comparison for strong/weak
  lists and `*`, ignored malformed candidates, and rejected invalid
  application-supplied response entity tags before emitting headers.
- Revalidated trusted single-upload state against its generated key, exact
  policy, byte limit, checksum, upload identity and attributes before spending
  a provider metadata request.
- Added table-scoped `dynamodb:PutItem` to generated DynamoDB audit IAM because
  AWS authorizes a `Put` member inside `TransactWriteItems` as its dependent
  item operation. The audit adapter still has no standalone `PutItem` path.

### Changed

- Advanced all 34 publishable packages and official plugin descriptors in
  lock-step to the additive `1.8.0` candidate line and froze matching browser,
  mobile, plugin and adoption guidance.

### Compatibility and evidence

- Kept existing buffering storage and signed single-upload APIs compatible;
  transfer services and HTTP lifecycle composition remain explicit additive
  capabilities, and a non-S3 `ObjectStore` alone does not claim resumable HTTP.
- Kept authorization, quotas, durable sessions, logical object pointers,
  retention and content safety in application use cases; checksum and metadata
  integrity never become an antivirus or safe-inline claim.
- Added no default CDN, Transfer Acceleration, scanner, scheduler, fixed compute,
  NAT Gateway, provisioned concurrency or large-body Lambda relay. Current
  prices, live providers, deployment and production remain separate evidence.

Release sections from `1.2.0` onward retain exact candidate-source wording
because their digests are bound into the portable agent bundle; tag, registry,
documentation and provider evidence stay separate in `VERIFICATION.md`.

## [1.7.0] - 2026-08-13

This additive lock-step minor releases Apple Container as the preferred fresh
local-service runtime on qualified Apple silicon hosts. It remains unpublished
until exact-source qualification, merge, tagging and registry publication
complete as separate evidence states.

### Changed

- Changed fresh `MINCO_CONTAINER_RUNTIME=auto` selection to prefer a ready,
  qualified Apple Container `1.2.x` runtime before a ready Docker Compose
  runtime for Minco-owned PostgreSQL and Rustack services.
- Advanced all 34 publishable packages and official plugin descriptors in
  lock-step to the additive `1.7.0` candidate line with no new package or
  first-publication boundary.
- Updated the frozen 1.7.0 manual, adoption guidance and all nine packaged
  Codex/Claude skills for the Apple-first local-development boundary.

### Compatibility and evidence

- Existing lifecycle receipts and exact owned resources still select their
  recorded runtime; the new preference applies only when no receipt or resource
  exists, so the release does not silently migrate persistent data.
- Explicit `docker` and `apple` selections remain authoritative and fail
  closed. Docker and the project-owned Compose customization boundary remain
  supported when Apple Container is unavailable or unsuitable.
- Production runtime, Plan IR, AWS behavior and cloud cost are unchanged. The
  release adds no deployment, provider contact, scheduler, fixed compute
  resource or automatic volume migration/deletion.

## [1.6.0] - 2026-08-13

This additive lock-step minor packages durable, schema-agnostic action auditing
across the reference Orders application and its SQLite, PostgreSQL and DynamoDB
profiles. It remains unpublished until exact-source qualification, merge,
tagging and registry publication complete as separate evidence states.

### Added

- Added bounded append-only V2 audit records with semantic action, actor,
  resource, correlation, operation and revision identity; privacy-aware field
  changes; opaque cursors; and explicit related-resource projections.
- Added physically separate PostgreSQL and SQLite ledgers with a transactional
  source journal plus a bounded, lease-safe, idempotent relay. Storage health,
  segment state and archive progress are explicit rather than hidden rotation.
- Added an Orders DynamoDB audit table committed with the operational mutation
  through one conditional `TransactWriteItems` call, including deterministic
  idempotency, race-safe revisions, hashed resource keys, bounded relationship
  fanout, encryption, point-in-time recovery and retained deletion policy.
- Added semantic `order.created`, `order.updated` and `order.deleted` actions
  plus permission-gated, cursor-bounded order history that remains queryable
  after a soft delete.

### Changed

- Advanced all 34 publishable packages and official plugin descriptors in
  lock-step to the additive `1.6.0` candidate line with no new package or
  first-publication boundary.
- Updated all nine packaged Codex/Claude skills, the candidate manual and
  adoption guidance for audit transaction, privacy, query and lifecycle
  boundaries.

### Compatibility and evidence

- Kept existing public Rust interfaces and Plan constructors compatible; the
  V2 audit model and derived DynamoDB audit plan are additive after repairing
  the candidate's initial exhaustive-public-structure SemVer blocker.
- Added no global DynamoDB default, automatic deletion, implicit archive job,
  provider contact, deployment, tag or publication. SQL profiles still require
  a distinct audit database or file, while the Orders DynamoDB profile is the
  low-idle AWS audit option.
- Kept retained growth and incomplete pricing visible: DynamoDB request,
  storage and point-in-time-recovery cost depend on Region and workload;
  PostgreSQL normally partitions by time; SQLite may explicitly seal bounded
  segments; stable logical cursors span those physical lifecycle choices.

## [1.5.0] - 2026-08-12

This additive lock-step minor packages the measured-assurance,
golden-topology cost-regression and typed side-effect-fake improvements already
merged after 1.4.0. It remains unpublished until exact-source qualification,
merge, tagging and registry publication complete as separate evidence states.

### Added

- Added provider-free, failure-scriptable typed fakes for SQS message handling,
  domain-event publication, object storage, feedback persistence and rich-mail
  submission. The fakes implement their owning public ports, capture ordered
  attempts and keep private payloads out of diagnostics.
- Added a deterministic golden-topology cost-regression baseline over seven
  reviewed Orders configurations without inventing provider prices or a
  production budget.
- Added pinned, measured local assurance for selected-crate coverage, bounded
  mutation resistance, nextest/Cargo parity and public Rust SemVer comparison
  against immutable `v1.4.0`.

### Changed

- Advanced all 34 publishable packages and official plugin descriptors in
  lock-step to the additive `1.5.0` candidate line with no new package or
  first-publication boundary, while Cargo refreshed only compatible transitive
  patch locks for `futures`, `num-integer`, `rustls-webpki` and `whoami`.
- Updated all nine packaged Codex/Claude skills and the frozen 1.5.0 manual to
  teach the typed-fake, cost and measured-assurance evidence boundaries.

### Compatibility and evidence

- Kept model-driven application evaluation and human-review effort explicitly
  `NOT RUN`; deterministic skill projection is not an agent outcome score.
- Kept Plan IR, CLI names, production adapter selection, provider topology and
  existing behavior compatible; the new fake types are additive public Rust
  interfaces selected only by tests.
- Added no live provider contact, deployment, poller, schedule, fixed compute,
  tag, publication or always-on control plane. Hosted performance and current
  live-provider evidence remain unavailable.

Release sections from `1.2.0` onward retain exact candidate-source wording
because their digests are bound into the portable agent bundle; tag, registry,
documentation and provider evidence stay separate in `VERIFICATION.md`.

## [1.4.0] - 2026-08-11

This additive lock-step maintenance minor releases the documentation and
ecosystem work completed after 1.3.0. It remains unpublished until exact-source
qualification, merge, tagging and registry publication complete as separate
evidence states.

### Fixed

- Released the rebalanced public homepage system diagram with contained labels,
  aligned operating-model cards and browser geometry regressions for desktop and
  mobile layouts.

### Changed

- Refreshed the reproducible development and proof ecosystem against the
  2026-08-11 stable package set: Rust remains current at 1.97.1, while uv,
  Node LTS, Playwright, Cargo dependencies and immutable action pins advance.
- Migrated Minco-owned digest, HMAC and Base64 dependencies to `sha2` 0.11,
  `hmac` 0.13 and `base64` 0.23 without changing their external byte contracts.
- Advanced all 34 publishable packages, official plugin descriptors, frozen
  documentation and nine packaged Codex/Claude skills to the `1.4.0` line.

### Compatibility and evidence

- Kept the published `1.3.0` public Rust API, serialized contracts, CLI,
  compile-time plugin selection and provider topology compatible and unchanged.
- Added no AWS resource, live provider contact, deployment, poller, schedule,
  fixed compute or always-on control plane. Performance remains `NOT RUN`; local
  and hosted checks do not become production-SLO or provider evidence.

## [1.3.0] - 2026-08-11

This additive lock-step minor candidate introduces one provider-specific Waffo
Pancake payment integration and advances the complete family to 34 packages. It
remains unpublished until exact-source qualification, merge, tagging and
registry publication complete as separate evidence states.

### Added

- Added the opt-in `minco-plugin-payments-waffo` beta with signed typed actions,
  hosted guest and authenticated checkout, read-only GraphQL, raw-body webhook
  verification, deterministic offline fakes and stable JSON CLI automation.
- Added a version-matched `minco-waffo-payments` skill to the packaged Codex and
  Claude bundle, with cumulative release-feature validation and a byte-identical
  package-local copy.

### Changed

- Advanced all 34 publishable packages and official plugin compatibility
  descriptors in lock-step to the additive `1.3.0` minor line; the payment
  plugin is statically selected, opt-in and absent from default features.

### Safety, cost and evidence

- Kept the Waffo payment boundary provider-specific and application-owned:
  signed requests do not redirect, session bearer tokens avoid generic
  persistence, checkout destinations require clean HTTPS URLs, production
  custom origins fail closed, and provider hints remain untrusted data.
- Added no polling, schedule, queue, database, fixed compute, AWS resource or
  always-on control plane. Offline conformance, registry publication, Waffo
  sandbox behavior, deployment and production readiness remain separate;
  live-provider evidence is `NOT RUN` for this candidate.

## [1.2.2] - 2026-08-10

This compatible lock-step patch hardens the public Signal documentation
presentation while preserving the complete 33-package feature and API boundary.
It remains unpublished until exact-source qualification, tagging and registry
publication complete as separate evidence states.

### Fixed

- Corrected the public homepage system diagram so contract, application,
  runtime and evidence labels remain inside their intended nodes.
- Removed inherited ordered-list markers and sibling spacing from the operating
  model cards, restoring one aligned desktop row while retaining intentional
  mobile stacking.

### Changed

- Bound the `1.2.2` documentation presentation contract into cumulative agent
  release coverage and added computed-style and desktop geometry regressions.

### Compatibility

- Kept public Rust APIs, serialized contracts, plugin capabilities, CLI
  behavior and deployment topology unchanged; official descriptors and the
  complete crate family advance together as the SemVer-compatible `1.2.2`
  patch.

### Safety and evidence

- Added no AWS resource, production deployment, hosted agent runtime, dynamic
  skill download or always-on control plane; browser, package, registry, docs.rs
  and provider/runtime evidence remain independent claims.

## [1.2.1] - 2026-08-10

This compatible lock-step patch keeps the complete 33-package family together
while making packaged AI skill freshness an ordinary, fail-closed release
contract. It remains unpublished until exact-source qualification, tagging and
registry publication complete as separate evidence states.

### Added

- Added cumulative, release-bound AI skill freshness metadata that maps every
  top-level release note to stable product features, version-matched
  documentation and the packaged skills that must teach them.

### Changed

- Updated all eight packaged skills for the complete 1.2 browser/native,
  verified-upload, rich-mail, owned-local-service, Signal documentation,
  topology-aware cost and release-bound evidence boundary.
- Made the deterministic Codex/Claude workflow receipt support exact check mode
  and added its freshness, bundle coverage and mutation tests to ordinary local
  and hosted release gates.

### Compatibility

- Kept skill names, trigger semantics, projection paths and mutation authority
  unchanged; the complete public crate and plugin family advances together as
  the SemVer-compatible `1.2.1` patch.

### Safety and evidence

- Added no hosted agent runtime, dynamic skill download or always-on control
  plane. Local and clean-Linux checks remain distinct from model quality,
  provider, deployment, runtime and production evidence.

## [1.2.0] - 2026-08-10

The complete 33-package family is published from immutable tag `v1.2.0` at
`48df3cc0ebb8990061b60d9383ced63532941079`. Exact source, hosted qualification,
tag identity, crates.io publication, the GitHub release, docs.rs, stable
documentation and live AWS application deployment remain separate evidence
states; this crate release did not deploy an application or establish a
production SLO.

### Added

- Added frontend-neutral browser and native-client response metadata,
  conditional-request CORS support, bearer challenges, bounded retry guidance,
  lifecycle signals, and one mobile/API compatibility guide without creating a
  second business API.
- Added a verified direct-object-upload lifecycle with authorization-first
  issuance, UUIDv7 object keys, bounded media/size/SHA-256 policy, expiring
  provider capabilities, provider-metadata completion checks, exact S3 POST
  signing, and explicit cleanup and content-safety boundaries.
- Added exact release/deployment binding for feedback, deterministic
  `cargo minco feedback task` conversion and create-only task receipts.
- Added deterministic `cargo minco handover` planning and digest-approved JSON
  and Markdown handover packets.
- Added machine-readable performance, provider-freshness and AWS/Rust
  capability ledgers with fail-closed validation.
- Added an explicit `mail.send` contract with validated To/CC/BCC/reply-to,
  text and HTML alternatives, bounded attachments and inline content, safe
  headers and tags, acceptance receipts, deterministic capture, and
  privacy-safe submission and delivery observation.
- Added a loopback-only Mailpit SMTP transport and a bounded, pinned local inbox
  for macOS and other Docker-compatible development environments.
- Added an Amazon SES v2 rich-mail transport with one SDK submission attempt,
  bounded timeouts, fixed sender identity, raw MIME, Minco correlation tags,
  configuration-set support, and policy-gated direct/SNS/EventBridge delivery
  event normalization.
- Added owned, loopback-only PostgreSQL and Rustack lifecycle support for
  `cargo minco dev` through Docker Compose and qualified Apple Container 1.2.x,
  including fail-closed identity checks, durable non-secret receipts, recovery,
  port-conflict refusal, and persistent-data preservation.
- Added a complete versioned 1.2.0 manual and the Signal documentation product:
  an original connected-runtime visual system, task-first discovery,
  architecture and troubleshooting guides, production blueprint, feature and
  plugin catalogs, exercised examples, responsive navigation, dark mode,
  accessible focus states, and reduced-motion support.

### Changed

- Made deployment Plan validation and runtime cost evidence depend on the exact
  runtime/ingress topology rather than always assuming API Gateway plus Lambda.
- Added p99 to bounded candidate load evidence and made current evidence checks
  part of both local and hosted-essential quality gates.
- Reserved GitHub Actions for Pages, exact-tag crates.io OIDC publication, and
  one bounded manual clean-Linux compatibility check; complete quality,
  package, runtime, Rustack, recovery, load and E2E qualification remains local
  and authoritative.
- Published the lock-step 33-package workspace and version-matched agent bundle
  as `1.2.0`.

### Compatibility

- Existing generic notification APIs, `NotificationsPlugin::new`,
  `NotificationsPlugin::memory`, `SesNotificationSink`, and
  `aws.ses.email-notifications` remain available. The new `mail.send` and
  `aws.ses.mail-delivery` capabilities are additive and opt-in.
- The release is additive at the CLI and feedback-model boundaries.
  Unsupported Lambda Function URL combinations now fail during Plan validation
  instead of reaching provider rendering, which aligns behavior with the
  existing declared-but-unsupported assurance status.
- Existing object-storage and notification APIs remain available; managed
  uploads and rich mail are additive opt-ins. Official plugin descriptor core
  compatibility advances in lock-step to `^1.2.0` without changing descriptor
  or capability schema versions.

### Safety and cost

- Ambiguous mail-submission outcomes never retry or fail over automatically,
  provider acceptance remains distinct from final mailbox delivery, and direct
  SES introduces no queue, worker, schedule, database, NAT gateway, provisioned
  concurrency, dedicated IP, or other fixed-capacity service.
- SNS and EventBridge wrappers require an exact trust policy and a successful
  caller-supplied verifier; direct trusted normalization rejects wrappers. SES
  topics are provider-safely encoded, merged tags are capped at 50, and delivery
  deduplication is explicitly bounded and in-process.
- Raw feedback attachments are confined to inert ignored storage; evidence
  outputs use descriptor-relative no-follow creation, identity rechecks,
  digest approval, conflict detection and rollback. Read-only handover planning
  executes no repository scripts and malformed evidence fails with stable
  machine-readable findings.
- Local service ownership never adopts or deletes foreign resources. No new
  production worker, poller, schedule, NAT Gateway, provisioned concurrency,
  public bucket, CDN, or always-on Minco control plane is introduced.

## [1.1.0] - 2026-08-06

The complete 33-package family is published from immutable tag `v1.1.0` at
`4d81543f7c5adb773655f23278abfe084de9f3e0`. Exact source and merged-main
qualification, tag identity, crates.io publication, the GitHub release,
docs.rs, stable documentation and live application deployment remain separate
evidence states.

### Added

- Added eight version-matched Agent Skills for building a Minco application,
  adding an OpenAPI-first operation or static plugin, using the application
  lifecycle, diagnosing and reviewing projects, contributing to the framework,
  and preparing an explicitly requested release.
- Added deterministic `cargo minco agent plan`, digest-bound `sync`, read-only
  `doctor`, bounded project/operation/task `context`, and cross-client `eval`
  commands for Codex and Claude Code.
- Added application-mode `AGENTS.md` generation and an optional managed Claude
  `@AGENTS.md` bridge without inheriting framework-only JJ/release policy.
- Added deterministic Codex/Claude projection parity, stale-plan, path-safety,
  user-owned instruction preservation, and 16 positive/negative workflow
  scenario contracts with explicit zero-model evaluation bounds.

### Changed

- Advanced the lock-step crate family and exact internal registry requirements
  from `1.0.0` to the compatible `1.1.0` minor line.
- Made packaged agent documentation validation derive its version prefix from
  the exact `cargo-minco` package version instead of a 1.0 implementation
  constant.
- Advanced archive-visible official plugin core compatibility ranges to
  `^1.1.0`; descriptor schema and capability versions remain unchanged.

### Documentation

- Added the agent-native development guide, README workflow, CLI reference,
  `1.0.0` to `1.1.0` adoption guide, and frozen `1.1.0` manual.
- Retained source, local, hosted, review, registry, documentation, deployment,
  runtime and production evidence as independent states; skill installation or
  evaluation does not upgrade another evidence lane.

### Compatibility boundary

This is an additive post-1.0 minor release. Existing 1.0 application APIs,
plugin descriptor schema, capability versions, deployment contracts and read-only
MCP catalog remain compatible. Official plugin distributions advance their core
compatibility range in lock-step with the crate family. Projects opt in to local agent projections by
reviewing `agent plan` and supplying its exact digest to `agent sync`.

## [1.0.0] - 2026-08-05

The complete 33-package family is published from immutable tag `v1.0.0` at
`39a69e36b051724c383da75d5907a824cbd2765b`. Exact-head and exact-main hosted
release qualification passed, all 33 exact versions were independently
verified on crates.io, and the versioned documentation was promoted separately
after publication. No live AWS application resource changed as part of the
release.

### Release

- Advanced and published the complete 33-package lock-step workspace from the
  reviewed `0.7.0` source boundary to `1.0.0`, including every
  internal dependency and archive-visible Minco core compatibility range.
- Added the access-pattern-specific `minco-aws-dynamodb` package and Orders
  adapter with conditional transactions, strong point reads, bounded indexed
  list queries, exact IAM/cost intent, and disposable Rustack conformance.
- Added exact-source security, restore/rollback, bounded API/worker load,
  documentation, generated-consumer and unpacked-package qualification gates.
- Kept source, local qualification, hosted qualification, tag, registry,
  documentation, deployment and application proof as independent states;
  release publication does not imply a live AWS deployment.

### Documentation

- Froze the intended 1.0 Rust, Cargo feature, CLI, configuration, Plan,
  release/deployment receipt, plugin distribution, diagnostic and MSRV
  boundaries, with explicit post-1.0 versioning rules and evidence limits.
- Added the `0.7.0` to `1.0.0` adoption guide and completed the previously
  missing `0.6.0` to `0.7.0` Rust migrations found by a forced semver audit.
- Froze a complete versioned 1.0 documentation manual covering realtime,
  ProjectView/MCP/workbench, DynamoDB, preview environments, exact static-site
  publication, compatibility rollback and alarm-guarded canary promotion.
- Reconciled the candidate narrative with the generated 33-package inventory,
  including ProjectView, MCP and Workbench as local-only, read-only opt-ins and
  DynamoDB as an access-pattern-specific adapter.

## [0.7.0] - 2026-08-04

### Added

- Added an opt-in provider-neutral realtime publisher with deterministic memory
  delivery and a minimal AWS AppSync Events adapter. Browser delivery is
  subscriber-only, visibility-bounded and followed by application-owned HTTP
  resynchronization after every initial or re-established subscription.
- Added explicit AppSync Events Plan IR, SAM resources, exact publish IAM,
  endpoint outputs and connection-minute/5 KiB operation cost dimensions. The
  minimal profile adds no NAT Gateway, fixed compute, schedule, provisioned
  concurrency or application heartbeat.
- Added compatibility-checked rollback assessment across exact historical
  release, deployment, environment and data boundaries, plus optional
  metric-alarm-guarded Lambda alias canaries with deterministic cleanup and
  durable receipts.
- Added bounded schema-1 ProjectView models, a local read-only stdio MCP server
  and an opt-in loopback-only Workbench with deterministic export and rendered
  desktop/mobile browser evidence.
- Added checked-in generated package, facade-feature, plugin, CLI,
  configuration, Plan and diagnostic reference derived from authoritative
  Cargo metadata, validated plugin manifests, Clap help and typed read models.

### Changed

- Added the first-publication `minco-plugin-realtime` package and advanced the
  lock-step source workspace to the unpublished `0.7.0` candidate boundary.
- Local and bounded hosted quality now fail when generated reference bytes
  drift from their authorities. Secret values and secret-reference names remain
  excluded, and reference generation performs no provider contact.

### Compatibility boundary

This is a pre-1.0 minor candidate with new public plugin, Plan, local tooling
and facade surface. The published baseline remains the immutable 28-package
`0.6.0` family; the 32-package `0.7.0` workspace has not been tagged or
published. Applications must update exact Minco dependencies and plugin
compatibility together if and when the candidate is released. Source
qualification, hosted checks, merge, tag, crates.io publication and live AWS
proof remain separate states.

## [0.6.0] - 2026-08-01

### Added

- Added strict archive-visible `minco-plugin.json` distribution records for
  official plugins, adapters and runtimes. Records expose compatibility,
  capabilities, configuration, operations, database assets, resources, IAM,
  wake sources, idle-cost intent, health, sensitivity and inert conformance
  evidence without loading plugin code.
- Added `cargo minco plugin list --json` and strengthened `plugin validate` so
  package inclusion, catalog/schema safety and overlapping statically linked
  descriptor fields fail deterministically before release.
- Added the public `minco-test` plugin conformance builder and `cargo minco
  plugin test --all --json`. Official and third-party-style packages now share
  the same bounded offline contract and stable diagnostic shapes.
- Added a standalone external-style plugin fixture using versioned public
  dependencies, archive metadata and deterministic composition tests.

### Changed

- Plugin, adapter and runtime archives now carry an explicit Minco `0.6.0`
  compatibility requirement. Metadata remains descriptive: Cargo dependency
  selection and typed constructor registration are still required.
- Plugin assurance now reports package/descriptor checks, concrete lifecycle,
  application readiness, provider/live evidence and production readiness as
  separate states instead of collapsing them into one pass.

### Documentation

- Promoted the detailed framework, resource API, plugin, testing, AWS,
  zero-idle and example documentation into a versioned `0.6.0` site while
  retaining immutable `0.5.0` pages.
- Added stable installation and tutorials, a complete plugin distribution
  reference, local search, responsive browser journeys and a candidate/stable
  switch that cannot claim registry publication early.
- Added the `0.5.0` to `0.6.0` upgrade guide with exact dependency,
  distribution-record and conformance migration steps.

### Compatibility boundary

This is a pre-1.0 minor release. Plugin distribution schema 1, the
public conformance report/builder types and new CLI output are public API
additions. Existing `0.5.0` application/resource behavior remains supported,
but plugin packages should update their exact Minco dependencies and
`core_compatibility` together. Source qualification, hosted checks, merge,
tag, crates.io publication, docs.rs and live AWS proof remain separate states.
The exact 28-package family is published from immutable tag `v0.6.0`; no AWS
resource mutation was part of the publication.

## [0.5.0] - 2026-07-31

### Added

- Added the opt-in OpenAPI-first resource convention for complete create, list,
  read, update and delete families. Resource operations carry validated
  identity/action metadata; success responses use predictable data/page
  envelopes; errors remain RFC 9457 Problem Details.
- Added bounded opaque cursor pagination, deterministic sort/filter allowlists,
  strong entity tags, required `If-Match` conditional updates/deletes and
  immutable idempotent-create replay semantics.
- Added `cargo minco make resource <name>` to select an already reviewed
  five-action contract family and atomically plan failing application/HTTP
  specifications, documentation and operation traces without generating
  domain policy, persistence or fake success.
- Completed the Orders reference resource through application-owned ports,
  memory, PostgreSQL and SQLite adapters, Axum contract tests and real-service
  HTTP lifecycle coverage.
- Added typed Plan cost classes and pricing-confidence evidence for
  zero-provisioned-compute profiles, including request/storage dimensions,
  incomplete allowances and explicit one-time cleanup policy.

### Changed

- The Orders external contract now exposes all five resource actions. Create,
  read and update return `{ "data": ... }`; list returns `data` plus bounded
  cursor-page metadata; delete returns `204 No Content`.
- Orders update and delete now require `If-Match`; missing and stale
  preconditions return `428` and `412` before persistence. Create responses
  return a stable replay snapshot even after later update or deletion.
- Optional hosted CI now defaults to a small `essential` clean-Linux
  qualification, while the authoritative full matrix remains local and a
  separately dispatched `release` profile retains browser, package,
  Plan/SAM, native ARM64, Rustack and E2E evidence.

### Documentation

- Added the `0.4.0` to `0.5.0` migration guide with the resource API wire
  format, generator boundary, application-layer responsibilities, Plan/cost
  additions and local/hosted verification profiles.
- Documented the standardized resource contract, tests and generator workflow
  without introducing a generic repository, ORM, Active Record layer or hidden
  SQL.

### Compatibility boundary

This is a pre-1.0 minor release. The standardized resource response
shape, conditional-write behavior, operation family and serialized Plan cost
surface are public compatibility changes. Existing applications opt in by
declaring complete resource metadata and migrating their contract and client
handling deliberately. The 28-package family is published at `0.5.0` from
immutable tag `v0.5.0`; registry publication does not prove live AWS deployment
or production promotion.

## [0.4.0] - 2026-07-30

### Added

- Added four publishable crates: provider-neutral `minco-config`, database
  lifecycle `minco-db`, local supervisor `minco-dev`, and guarded provider
  controller `minco-deploy-aws`. The lock-step family grows from 24 to 28
  packages.
- Added strict
  application/enabled-plugin schemas, fixed environment precedence, opaque
  `env:`/`ssm:` secret references, redacted provenance and diff, deterministic
  effective digests, typed constructor deserialization, and `cargo minco
  config check|explain|diff|schema`.
- Added Plan IR schema 2 for one HTTP API function plus explicit worker
  functions, SQS queues, DLQs, event-source mappings, partial-batch behavior
  and reviewed schedules, with deterministic local-service, IAM, cost,
  performance and SAM projections.
- Added generic API-only, standard/FIFO worker, redrive, schedule and DynamoDB
  fixtures with stable validation and migration-rejection coverage.
- Added digest-bound database migration status/plan/lock/apply/verify receipts
  and classified, preservation-aware seed plans for `reference`, `demo`, `test`
  and explicitly authorised `bootstrap` data.
- Added graph-derived `cargo minco dev` dry-run and supervision, selected
  PostgreSQL/Rustack dependencies, bounded process groups and structured
  lifecycle events.
- Added contract-aware module, operation, migration, seeder, worker, adapter,
  test and plugin generators plus app-owned stubs. Plans are deterministic,
  existing/symlinked paths fail closed, and operation specifications start
  failing rather than inventing business behavior.
- Added `cargo minco contract diff` and `cargo minco upgrade report` for bounded
  structural compatibility and redacted schema/feature inventory.
- Added immutable application packaging, deployment receipts, guarded
  CloudFormation change-set/apply phases, hosted contract/readiness/
  authentication/smoke/artifact verification and exact-artifact promotion.
- Defined zero provisioned application compute, residual cost classes/pricing
  confidence, the repository-native Verified Review Loop and an explicit AWS
  service doctrine.

### Changed

- `cargo minco explain --json` now identifies an operation's deployment
  function/trigger, `cost --json` includes runtime resource dimensions, and
  `perf --json` reports every function artifact and available SHA-256.
- The release manifest is schema 3 and binds configuration, migration and seed
  digests in addition to source, OpenAPI, Plan, template, lockfile, toolchain
  and artifact identity.
- The deployment CLI now separates plan, change-set review, apply, hosted
  verify and promote behind independent receipts and exact-digest approvals.
- The M9 application-lifecycle milestone is complete in source. Rollback/
  canary, static-site domains, review-environment cleanup and later ecosystem/
  workbench programs remain deferred.

### Fixed

- Native Lambda packaging now normalizes ZIP entry timestamps, permissions and
  order after Cargo Lambda builds. Repeated builds of byte-identical Orders and
  SQS worker binaries therefore produce the same artifact digest, while
  unexpected archive entries fail before replacement.

### Documentation

- Defined Minco's five-plane contract-to-cloud product identity, developer and
  deployment golden paths, measurable 1.0 completion boundary, explicit
  non-goals, and M9-M12 framework-completion program.
- Added the target Diátaxis information architecture without moving existing
  pages ahead of deterministic link and snippet validation.
- Corrected README drift against authoritative metadata: Feedback is stable,
  and the source inventory includes the static-site plugin, AWS adapters, and
  SQS worker runtime.
- Reconciled the adoption-measurement narrative with the authoritative current
  dependency, timing, and Lambda artifact report.
- Documented the trigger-aware Plan decision, schema 2 migration, explicit SQS
  worker constraints and unchanged minimal-idle default.
- Added a dedicated `0.3.1` to `0.4.0` guide covering the four new crates,
  schemas, configuration, database/dev/generator lifecycle, CLI and deferred
  operational boundaries.

### Compatibility boundary

The public plan types, Plan schema 2 fields, typed configuration and database
schemas, package/release model and lifecycle/deployment CLI form the Minco
`0.4.0` pre-1.0 minor boundary. API-only Plan schema 1 configurations remain
supported. Source and package qualification do not prove merge, tag, crates.io
publication, live AWS deployment or production promotion.

## [0.3.1] - 2026-07-27

### Fixed

- Enforced the Feedback text-only profile when `max_attachments = 0`: the
  widget hides screenshot, file and voice controls, and the server rejects
  multipart attachment fields without changing the JSON submission contract.
- Isolated SQLx PostgreSQL and SQLite backend features so PostgreSQL-only
  Feedback and Orders consumers no longer compile `sqlx-sqlite` or
  `libsqlite3-sys`, while SQLite-only consumers no longer compile
  `sqlx-postgres`.
- Added a complete normal/build dependency-graph regression covering Feedback,
  the official SQLx extensions, the Orders adapter/service, memory/no-default
  surfaces, and the deliberate all-backend workspace build.

### Compatibility boundary

This patch preserves the public Rust API and serialized contracts of `0.3.0`.
It tightens an existing zero-attachment configuration boundary and removes
unselected database backends from Cargo dependency graphs. The lock-step
package inventory remains 24 crates.

## [0.3.0] - 2026-07-27

### Added

- Added metadata-only ownership and deterministic installation provenance for
  typed singleton services and ordered contributions, including bounded
  `cargo minco inspect --json` output and duplicate diagnostics naming the
  first and attempted owners.

### Changed

- `PluginContext` service/contribution accessors now return owner-bound
  registrar views, and `ServiceError::Duplicate` carries a structured
  first-owner/attempted-owner payload. Ordinary chained registration call sites
  remain source-compatible; explicit mutable-collection annotations must
  accept the registrar type.

### Compatibility boundary

This is a pre-1.0 minor release because the registrar return types and
`ServiceError::Duplicate` payload are public API changes. The lock-step package
inventory remains 24 crates.

## [0.2.0] - 2026-07-26

### Added

- Added a typed, deterministic HTTP header policy that merges exact
  application and installed-plugin requirements without wildcard broadening;
  Feedback-specific headers are no longer global defaults.
- Added the opt-in publishable `minco-aws-worker` crate for SQS Lambda
  partial-batch responses, bounded concurrency, FIFO fail-forward ordering,
  payload limits and optional one-pass outbox dispatch.
- Added stricter OpenAPI object, idempotency, authentication and RFC 9457 media
  policy with positive/negative fixtures, effective path/reference parameters,
  OpenAPI-correct anonymous security alternatives and explicit
  permission-scoped metadata.
- Added deterministic repository-truth validation across versions, package
  inventory, catalog/facade/descriptor metadata, generated plans and
  roadmap/task status.
- Added graph-derived local PostgreSQL/Rustack startup, standard AWS endpoint
  configuration, isolated S3/SQS/SSM/STS conformance, and safe port/database
  overrides with a pinned multi-platform Rustack 0.9.1 image. The conformance
  gate also proves the real Minco SSM SDK adapter locally and in hosted CI.

### Changed

- `HttpRuntimeConfig` now carries `HttpHeaderPolicy`, and
  `apply_standard_middleware` returns `HttpConfigurationError`.
- Plugin catalog entries now identify their `kind` and facade `feature`.
- Plan/SAM request headers are normalized and sorted deterministically.
- Generated-consumer quality checks share the repository target cache while
  retaining fresh PostgreSQL and SQLite workspaces.

## [0.1.1] - 2026-07-24

### Fixed

- Added a library documentation target to `cargo-minco` so docs.rs can build
  and publish the CLI documentation while preserving the `cargo-minco`
  executable and `cargo minco` command behavior.
- Added a docs.rs-shaped Rustdoc gate to local quality, release dry runs, and
  hosted manual CI.

## [0.1.0] - 2026-07-24

### Added

- Provider-neutral plugin kernel, typed service injection, capability graph, and
  deterministic graph validation.
- OpenAPI 3.1 validation, operation inventory, canonical digest, and checked-in
  Rust binding generation.
- Axum/Tower HTTP conventions with request IDs, exact CORS, bounded bodies,
  timeout, tracing, compression, principal extraction, and RFC 9457 errors.
- PostgreSQL and SQLite SQLx foundations with explicit migrations.
- Native ARM64 Lambda/API Gateway HTTP API adapter and JWT claim mapping.
- Deployment Plan IR, SAM renderer, structural performance rules, and cost
  profiles for Neon, self-hosted PostgreSQL, RDS, Aurora Serverless v2,
  DynamoDB on-demand, and persistent SQLite.
- Immutable release manifests.
- Orders reference vertical slice with memory, PostgreSQL, and SQLite adapters.
- JJ-first task workspaces and repository-native roadmap/task tracking.
- Local unit, feature, and e2e runners plus an opt-in manual GitHub Actions
  workflow.
- `minco update`, plugin management, deployment, cost, test, VCS, and
  inspection commands.
- Publishable `minco` facade with feature-gated contract, HTTP, plugin, SQLx,
  Lambda, planning, release, and test capabilities.
- `cargo-minco` packaging as a real Cargo subcommand (`cargo minco`).
- A real `cargo minco new` layered application generator with PostgreSQL or SQLite profiles and JJ-first initialization.
- Application-relative test, migration, operation-trace, plugin-catalog, and architecture validation commands.
- crates.io metadata, versioned path dependencies, package content allowlists,
  dual-license files, deterministic publish validation, guarded multi-package
  dry-run/upload scripts, and manual OIDC trusted-publishing workflow.
- Strengthened plugin lifecycle with core compatibility, typed multi-contributions,
  strict configuration contracts, graph-before-install validation, deterministic
  finalization, and application-provided composition dependencies.
- Official sessions, identity, object-storage, events/outbox, notifications,
  audit, static-site, and Feedback plugins.
- Feedback Web Component with configurable FAB placement, browser-authorized
  screenshots, bounded voice notes, optional transcription, threaded
  clarification, PostgreSQL/SQLite/memory storage, developer API/CLI, and
  deterministic AI handoff.

### Verification boundary

The pinned Rust compiler, Docker-backed persistence, JJ, Cargo Lambda, SAM
linting, read-only CloudFormation/IAM validation and package publication dry
runs pass. Real AWS deployment, provider-adapter conformance, crate upload and
the deferred repository-wide Codex Security Deep Scan remain outside the
verified boundary; see `VERIFICATION.md`.
