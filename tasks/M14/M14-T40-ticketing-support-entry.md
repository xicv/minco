---
id: M14-T40
title: Add shared Ticketing interaction and multi-surface support entry
milestone: M14
status: active
priority: high
area: plugins/ticketing/feedback/integration
depends_on: [M14-T39]
operations:
  - getTicketingSupportEntry
  - getTicketingBootstrap
  - issueTicketingHandoff
  - consumeTicketingHandoff
  - createTicketFromHandoff
owned_paths:
  - crates/minco-interaction/**
  - plugins/minco-plugin-feedback/Cargo.toml
  - plugins/minco-plugin-feedback/src/lib.rs
  - plugins/minco-plugin-feedback/src/model.rs
  - plugins/minco-plugin-feedback/src/service.rs
  - plugins/minco-plugin-feedback/src/transcription.rs
  - plugins/minco-plugin-feedback/src/plugin.rs
  - plugins/minco-plugin-feedback/README.md
  - plugins/minco-plugin-feedback/minco-plugin.json
  - plugins/minco-plugin-ticketing/**
  - examples/ticketing-entry/**
  - docs/adrs/0046-multi-surface-ticketing-entry.md
  - docs/how-to/ticketing-entry-surfaces.md
  - docs/DECISIONS.md
  - tasks/M14/M14-T40-ticketing-support-entry.md
  - roadmap/tasks.mmd
  - Cargo.toml
  - Cargo.lock
  - crates/minco/Cargo.toml
  - crates/minco/src/lib.rs
checks:
  - cd examples/ticketing-entry && npm test
  - cd examples/ticketing-entry && npm run check
  - python3 -m json.tool examples/ticketing-entry/handoff-contract.schema.json
  - cargo check -p minco-interaction -p minco-plugin-feedback -p minco-plugin-ticketing --all-targets --all-features --locked
  - cargo test -p minco-interaction -p minco-plugin-feedback -p minco-plugin-ticketing --all-targets --all-features --locked
  - cargo clippy -p minco-interaction -p minco-plugin-feedback -p minco-plugin-ticketing --all-targets --all-features --locked -- -D warnings
  - cargo minco plugin validate
  - git diff --check origin/main...HEAD
---

# M14-T40 - Add shared Ticketing interaction and multi-surface support entry

## Goal

Add a project-agnostic Ticketing plugin and extract genuinely shared Feedback
interaction mechanics without moving product-specific workflow into Minco core.
Expose one portal-first support-entry contract that can be used by an embedded
floating/inline launcher, a dedicated support domain, an optional packaged
browser extension, native clients and application backends.

The first committed slice proves the browser contract under
`examples/ticketing-entry`. The task remains active until the shared Rust crate,
Ticketing domain, atomic handoff store, durable SQLite profile, HTTP contract,
Feedback compatibility refactor and local qualification are complete.

## Product decision

The canonical requester UI is a hosted support portal. The launcher and browser
extension are thin adapters into that portal, not separate ticket clients.
PeoplePlanner uses a same-origin Laravel BFF to derive trusted requester identity
and obtain a one-time launch URL from Minco Ticketing. A direct portal remains
available as the universal fallback.

## Acceptance

### Shared interaction crate

- create optional `minco-interaction`, not a `minco-core` module;
- move provider-neutral transcription ports/adapters out of Feedback while
  retaining every existing Feedback public name and Cargo feature through
  re-exports;
- add shared attachment kind/metadata/policy and bounded server-side storage
  orchestration that delegates to `minco-plugin-object-storage`;
- use the existing verified direct-upload lifecycle for browser object bytes;
- add a tiny static transition helper without a runtime workflow registry;
- share only genuinely identical semantic activity construction and explicitly
  describe any post-commit recorder as best effort rather than transactional;
- no Axum, SQLx, Lambda or AWS SDK dependency in domain-only modules; and
- no direct S3 SDK dependency in Feedback, Ticketing or `minco-interaction`.

### Ticketing domain

- add a separate `minco-plugin-ticketing` bounded context;
- implement ticket, requester, public reply, internal note, assignment, queue,
  priority, status, timer category, attachments, source references and external
  message identity;
- statuses are New, Open, Pending Requester, Pending Internal, On Hold,
  Resolved and Closed;
- Pending Internal retains an open organizational clock;
- internal notes never appear in requester projections;
- optimistic revisions prevent lost updates;
- provider/mail ingestion is idempotent by provider, mailbox scope, external
  identity and content digest;
- same identity/same digest returns the existing result; same identity/different
  digest fails closed; and
- Feedback links through a generic source reference rather than merging the two
  domains.

### Support-entry and portal handoff

- define surfaces for widget, portal, extension, API and mobile;
- accept only bounded redacted context and opaque resource references;
- browser input cannot assert trusted requester identity, tenant or permission;
- issue high-entropy one-time handoffs only through a private
  `ticketing.integrate` operation;
- restrict project, portal origin, embed origins and return URLs through exact
  configuration;
- handoff TTL is bounded and short;
- store only handoff digests;
- place the bearer handoff in the portal URL fragment, never a query string;
- consume atomically with the first ticket/session mutation;
- concurrent consumption creates at most one authoritative result;
- serve a browser-safe bootstrap and the reviewed launcher asset;
- keep the portal frontend replaceable and application-owned;
- support modal and new-tab modes and retain a trusted fallback; and
- add no fixed compute, portal hosting resource, extension or hidden schedule by
  default.

### Browser launcher

- framework-neutral ES module and Web Component;
- floating and inline modes;
- same-origin BFF endpoint or explicit host callback;
- page URL strips query, fragment and user information;
- selected text is included only when explicitly supplied;
- no automatic screenshot, microphone or file capture;
- exact portal-origin launch validation;
- iframe `no-referrer`, constrained sandbox and mobile full-screen layout;
- `postMessage` verifies exact origin and source and accepts only ready, close
  and bounded resize;
- new-tab fallback;
- no service credential or trusted identity in browser configuration;
- dependency-free Node tests and browser acceptance tests; and
- Chrome extension guidance requires packaged Manifest V3 code and least
  permissions.

### Persistence and HTTP

- memory adapter is deterministic and test-only;
- SQLite adapter uses its own database file and transactions;
- handoff consume/ticket creation and external-message deduplication are atomic;
- OpenAPI 3.1 is the source of truth for every operation;
- Problem Details, request IDs, ETag/If-Match and cursor pagination follow Minco
  conventions;
- HTTP handlers extract/map, call one use case and map the response;
- private integration operations require explicit permission; and
- browser-facing operations never expose internal notes or provider payloads.

### PeoplePlanner boundary

- document Laravel BFF and sidecar integration without editing PeoplePlanner;
- browser/mobile clients continue to authenticate to PeoplePlanner;
- Spatie permissions and PeoplePlanner references are mapped server side;
- the Rust service binds privately and owns a separate Ticketing database;
- no cross-database foreign keys or shared SQLite file;
- direct uploads use private object-storage capabilities; and
- no production credentials, deployment or mailbox configuration in this task.

## Non-goals

- a complete branded PeoplePlanner portal frontend;
- a Chrome Web Store publication;
- browser scraping or automatic background capture;
- anonymous public support enabled by default;
- Microsoft Graph or SES production credentials and tenant/domain setup;
- subject-only email threading;
- a generic workflow engine, ORM or global storage facade;
- automatic malware scanning without a selected provider/workflow;
- exactly-once email or event delivery claims;
- an always-on worker, hidden poller or implicit schedule;
- editing or dispatching GitHub Actions; or
- unrelated formatting changes.

## Research boundary

Review current primary documentation and source for Chatwoot, Frappe Helpdesk,
Papercups, FreeScout, Zammad, Chrome Manifest V3/Identity/Web Store policy,
`postMessage`, SES receiving/S3 notifications and Microsoft Graph shared-mailbox
notifications/delta synchronization. Record source dates and retain only
patterns compatible with Minco's typed static composition, privacy boundary and
minimal-cost topology.

## Evidence currently present

The browser reference has dependency-free tests for URL redaction, secure portal
configuration, same-origin handoff endpoints, closed context shape, bounded
resource references, exact-origin launch URLs, strict `postMessage` origin/source
checks and the committed JSON schema. This evidence does not qualify the Rust
plugin or a production portal.

No GitHub workflow, AWS operation, mailbox configuration, deployment,
publication, tag, release or production mutation is authorized by this task.
