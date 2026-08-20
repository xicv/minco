# ADR 0046: Portal-first Ticketing with one multi-surface support-entry contract

## Status

Proposed

## Context

A support system must be reachable from products that Minco owns, products built
with another framework, native clients, internal tooling, and pages where the
product team cannot add application code. The obvious entry surfaces are not
mutually exclusive:

- a floating button inside the product;
- a dedicated hosted portal such as `support.peopleplanner.app`;
- an optional browser extension or side panel; and
- a headless API used through an application backend-for-frontend.

Building an independent ticket client for each surface would duplicate
conversation UI, attachment policy, requester authentication, accessibility,
localization, analytics and support workflow. Making the floating widget the
only interface would make ticket history, knowledge discovery, bookmarking,
mobile use and cross-product support awkward. Making a browser extension the
primary interface would add installation, permission, review and update costs to
every user, including users already inside an application Minco can integrate
directly.

The existing Feedback plugin proves a useful small Web Component with Shadow
DOM, screenshot/file/voice capture and a conversation loop. Ticketing is a
different bounded context, but its browser entry should reuse the same future
interaction primitives rather than copy the complete Feedback widget.

Open-source support products converge on a similar separation:

- Chatwoot exposes a hosted help center and a programmable web widget, and lets
  the widget transition between knowledge content and a conversation;
- Frappe Helpdesk exposes portal, email and agent channels while keeping one
  ticket model and supports requester-versus-organization visibility;
- Papercups demonstrates a framework-neutral launcher with programmatic
  open/close and customer metadata; and
- FreeScout and Zammad keep shared-inbox and channel semantics behind one agent
  workflow rather than implementing a different helpdesk per entry point.

Browser-extension platforms impose an additional boundary. Manifest V3
extension logic must be packaged with the extension, permissions should be
minimal, interactive identity flows must follow a user action, and an extension
that merely opens a website has weak product and store-review value. A Minco
extension therefore needs an actual user-invoked capability such as confirmed
page-context capture, selected-text capture, screenshot capture or ticket lookup;
it must not be the default installation path.

## Decision

Minco Ticketing is **portal first, not portal only**.

One canonical hosted portal owns requester-facing ticket UI, conversation
history, knowledge content, attachment presentation, accessibility and
localization. Every other surface is a thin entry adapter into that portal and
the same Ticketing API.

### Supported surfaces

1. **Hosted portal** — the universal baseline. It works as a normal first-party
   website, can have a custom domain, supports direct navigation and remains
   usable when embedding is blocked.
2. **Floating or inline launcher** — the recommended default for applications
   the product owner controls. It requests a short-lived support handoff from
   the host application's same-origin backend and opens the canonical portal in
   a constrained cross-origin iframe. It falls back to a trusted portal tab.
3. **Browser extension** — an optional packaged Manifest V3 client for internal
   users or pages that cannot embed the launcher. It uses the same handoff JSON
   and portal URL but packages its own extension logic and asks only for
   permissions needed by explicit user actions.
4. **Headless/native integration** — a backend-for-frontend or native client
   calls authenticated Ticketing operations and uses direct object-upload
   capabilities. Browsers do not receive service credentials.

### Shared support-entry contract

The shared `minco-interaction` boundary will own provider-neutral support-entry
values alongside attachment, transcription and workflow primitives. The first
contract includes:

- `SupportSurface` (`widget`, `portal`, `extension`, `api`, `mobile`);
- bounded `SupportContext` with a redacted page URL, route/release/request IDs,
  locale, timezone, viewport, explicitly supplied selected text and opaque
  external resource references;
- browser-safe `SupportBootstrap` describing project, portal origin, branding,
  enabled capture capabilities, limits and privacy notice; and
- a short-lived, high-entropy, one-time `SupportHandoff` issued only by a
  trusted integration operation.

A handoff contains trusted requester identity and claims derived by the host
backend, project scope, allowed return location, surface, bounded context,
creation/expiry time and one-time consumption state. Browser input cannot
assert a trusted requester subject or permissions.

Handoff consumption must be atomic with the first ticket/session mutation that
uses it. A resolve-then-revoke sequence is not sufficient because concurrent
consumers could both succeed. Persistence adapters own a use-case-shaped atomic
operation or transaction.

The browser receives a launch URL on the exact configured portal origin. The
one-time bearer value belongs in the URL fragment, not the query string, so it
is absent from HTTP request targets, normal server access logs and referrer
headers. The portal immediately exchanges and clears it. Handoffs have a short
expiry, cannot be renewed by the browser and are stored only as digests.

### Launcher security

The launcher:

- strips query, fragment and URL user information from page context by default;
- never captures selected text, screenshot, audio or files without an explicit
  user or host-application action;
- uses a same-origin BFF endpoint or an explicit host callback for handoff
  issuance;
- accepts only launch URLs on the exact configured portal origin;
- embeds with `referrerPolicy="no-referrer"` and a constrained sandbox;
- validates both `postMessage` origin and `source` and accepts a closed message
  vocabulary;
- exposes a trusted new-tab fallback for browser storage, CSP, frame-ancestor or
  accessibility incompatibility; and
- sends no secret diagnostic data through browser events or ordinary logs.

The browser reference is a dependency-free ES module and Web Component. It is
not a second ticket client and contains no Ticketing business rules.

### PeoplePlanner integration

PeoplePlanner remains the browser/mobile backend-for-frontend:

```text
PeoplePlanner browser or mobile client
                |
                v
PeoplePlanner Laravel BFF
- existing session and CSRF boundary
- Spatie permission mapping
- PeoplePlanner-specific resource references
                |
      private authenticated call
                v
Minco Ticketing service
- one-time handoff issuance
- ticket domain and persistence
- attachments, audit and events
                |
                v
support.peopleplanner.app
```

The launcher calls a same-origin PeoplePlanner endpoint. Laravel derives the
current requester and calls Ticketing privately. It returns only the bounded
launch response. The browser never calls the private integration operation and
never receives its credential.

PeoplePlanner and the Minco service do not share a database file or cross-database
foreign keys. PeoplePlanner references are opaque scoped values.

### Portal deployment

The portal can be a separate static frontend backed by the Minco Ticketing API,
or an application-owned frontend using the same contract. A custom domain is a
presentation and trust boundary, not a separate ticket database. Multiple
brands or products can select portal configuration statically without cloning
the Ticketing domain.

The base plugin adds no portal hosting resource, fixed compute, schedule,
provisioned concurrency, NAT Gateway or browser extension. The application
selects static-site hosting, API runtime, database and any queue/worker explicitly.

## Consequences

- Every product gets one canonical requester experience and one ticket history.
- Owned applications can add in-context support with a very small integration.
- A dedicated portal remains available when iframe embedding, third-party
  storage or application JavaScript is unavailable.
- Browser extensions become a targeted integration for real extension-only
  value, not a mandatory distribution channel.
- Native clients and third-party frameworks use the same handoff and API rather
  than importing Rust code.
- Context capture is useful but privacy-bounded and requires explicit action for
  rich content.
- The portal origin, embed origin, return URL and project scope become explicit
  reviewed configuration.
- Handoff persistence and atomic consumption add a small durable contract but
  no always-on process.
- A portal may need token-backed or partition-aware session behavior inside a
  third-party iframe; the new-tab fallback remains part of the supported
  contract rather than an error afterthought.

## Alternatives rejected

### Widget-only Ticketing

A widget is excellent for entry and context, but poor as the sole location for
history, organization-wide ticket views, knowledge discovery, accessibility
fallback and direct links.

### Portal-only Ticketing

A portal alone loses useful page, route, release, request and resource context
and asks users to leave the product for every support interaction.

### Browser extension as the default

Installation and permission prompts are unjustified on websites the owner can
integrate directly. Store policy and Manifest V3 also make a remote-script or
link-only extension the wrong baseline.

### Copy the Feedback widget into Ticketing

This would duplicate media, redaction, upload, transcription and browser-shell
code. Shared interaction primitives should be extracted once while Feedback and
Ticketing retain separate domain models.

### Put trusted identity in widget attributes

DOM attributes and browser JavaScript are untrusted and visible. Trusted
requester identity must come from the authenticated host backend or the portal's
own identity provider.

### Long-lived signed URLs

Long-lived bearer URLs leak through history, screenshots, support messages and
browser synchronization. A short-lived opaque, one-time handoff has a smaller
blast radius and supports revocation by consumption.

## Compatibility

This decision is additive. The existing Feedback widget and routes remain
unchanged until their shared interaction pieces are moved with public aliases
and regression fixtures. The browser reference under `examples/ticketing-entry`
is not yet a published Minco API; it proves the contract before the Ticketing
plugin freezes it.

## Safety

The contract carries no service credential, raw authentication token, URL query,
fragment, automatic selected text or automatic media capture. Origins and
return locations are exact allowlists. Handoff values are high entropy, short
lived, stored as digests and consumed atomically. Provider-specific identity,
email and extension authentication remain adapter and application concerns.
