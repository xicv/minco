# Connect an application to Minco Ticketing

Minco Ticketing should expose one support system through several thin entry
surfaces. The recommended product shape is:

```text
                       +-----------------------------+
                       | Canonical support portal    |
                       | support.peopleplanner.app   |
                       +--------------+--------------+
                                      |
            +-------------------------+-------------------------+
            |                         |                         |
     floating launcher          browser extension         direct portal
     in PeoplePlanner            or side panel             navigation
            |                         |                         |
            +-------------------------+-------------------------+
                                      |
                               Ticketing API
                                      |
                         ticket store / objects / audit
```

The portal is the canonical requester UI. The launcher and extension open that
portal with a short-lived support handoff; they do not implement independent
ticket workflows.

## Choose the entry surface

| Situation | Recommended surface | Reason |
| --- | --- | --- |
| Website or web application you control | Floating or inline launcher plus portal | Lowest user friction and richest safe page context |
| Mobile application | Native support action plus portal/API | Uses native navigation and the same handoff contract |
| User bookmarks, ticket history, knowledge content | Dedicated portal | First-party navigation, accessibility and direct links |
| Internal staff working across third-party sites | Optional browser extension | Adds user-confirmed context where the site cannot embed code |
| Server, automation or another backend framework | Private API/BFF integration | Keeps service credentials and identity off the browser |
| Public unauthenticated website | Portal or explicitly anonymous launcher | Requires separate abuse, verification and privacy policy |

For PeoplePlanner, use both the launcher and the dedicated portal. The launcher
is the normal in-product entry. `support.peopleplanner.app` is the stable direct
location and fallback. A Chrome extension is a later optional tool for staff who
need support capture outside PeoplePlanner.

## Browser launcher

The reference implementation is under `examples/ticketing-entry` and uses a
Web Component:

```html
<meta name="csrf-token" content="host-framework-token" />
<script
  type="module"
  src="https://support.peopleplanner.app/_minco/ticketing/support-entry.js"
></script>
<minco-support-launcher
  portal="https://support.peopleplanner.app/"
  project="peopleplanner"
  handoff-endpoint="/api/support/handoff"
  label="Support"
  position="bottom-right"
></minco-support-launcher>
```

The browser posts only bounded context to the same-origin host endpoint. The host
endpoint derives trusted identity from its existing authenticated session and
calls the private Ticketing integration operation.

A host with a custom client or CSRF mechanism can provide callbacks instead:

```js
window.MincoSupport = {
  async issueHandoff(request) {
    return peoplePlannerApi.post('/api/support/handoff', request);
  },
  getContext() {
    return {
      route_name: window.currentRouteName,
      release_id: window.applicationReleaseId,
      request_id: window.currentRequestId,
      resource_references: [
        {
          system: 'peopleplanner',
          resource_type: 'shift',
          resource_id: window.currentOpaqueShiftId,
        },
      ],
    };
  },
};
```

Do not put a Minco service token, requester role, tenant ID or authorization
claim in the page. The backend owns those values.

### Floating versus inline

The same element supports:

```html
<minco-support-launcher mode="floating" ...></minco-support-launcher>
<minco-support-launcher mode="inline" ...></minco-support-launcher>
```

Floating mode is suitable for application-wide support. Inline mode is suitable
for a Help menu, error state or settings page. Both open the same portal.

### Modal versus tab

The default `target="modal"` embeds the portal. Use `target="tab"` when the host
has a restrictive Content Security Policy, when browser storage does not work in
an embedded context, or when the portal needs a larger workflow:

```html
<minco-support-launcher target="tab" ...></minco-support-launcher>
```

The modal itself offers a new-tab fallback if the portal does not confirm
readiness. Keep that fallback: iframe support is not universal enough to be a
hard dependency.

## Handoff contract

The browser-facing request is closed and bounded:

```json
{
  "project_id": "peopleplanner",
  "surface": "widget",
  "return_url": "https://app.peopleplanner.example/orders/opaque-id",
  "context": {
    "page_url": "https://app.peopleplanner.example/orders/opaque-id",
    "page_title": "Order",
    "route_name": "orders.show",
    "release_id": "release-id",
    "request_id": "request-id",
    "locale": "en-AU",
    "timezone": "Australia/Adelaide",
    "viewport": "1440x900",
    "resource_references": [
      {
        "system": "peopleplanner",
        "resource_type": "order",
        "resource_id": "opaque-order-id"
      }
    ]
  }
}
```

The host backend adds trusted requester and authorization information when it
calls Ticketing privately. The browser response is:

```json
{
  "launch_url": "https://support.peopleplanner.app/start#handoff=opaque-one-time-value",
  "expires_at": "2026-08-20T06:35:00Z"
}
```

The portal exchanges the fragment value immediately, clears it from browser
history and creates the first ticket or requester session through one atomic
store operation. Store only the handoff digest. A repeated consume returns the
same safe idempotent result only when the original operation completed; a
concurrent second consumer must not create another session or ticket.

Recommended defaults:

- handoff TTL: 60 to 180 seconds;
- exact portal origin;
- exact host/return-origin allowlist;
- one project scope per handoff;
- no wildcard embed origins;
- no requester identity from browser JSON;
- no query-string bearer value;
- no automatic retry after an ambiguous state-changing provider outcome.

## PeoplePlanner Laravel BFF

The browser route should be ordinary PeoplePlanner application code, for example:

```text
POST /api/support/handoff
```

Its use case is:

1. require the current PeoplePlanner session;
2. apply CSRF and ordinary request limits;
3. authorize support access with the current Spatie permission policy;
4. validate the closed browser context;
5. replace browser requester/tenant claims with server-derived values;
6. map PeoplePlanner records to opaque resource references;
7. call Minco Ticketing over loopback, a Unix socket or a private authenticated
   endpoint;
8. verify that the returned launch URL uses the configured portal origin; and
9. return only `launch_url` and `expires_at`.

Propagate the PeoplePlanner request ID to Minco. Do not share the PeoplePlanner
SQLite file with Rust. The Ticketing service owns its database and audit/outbox
intent; PeoplePlanner owns its domain records.

The sidecar can run on the existing EC2 host when capacity is measured. Bind it
to loopback or a Unix socket and supervise it explicitly. This reuses paid
capacity but is not literally free: object storage, email, logs, backups and
transcription still have usage costs.

## Dedicated portal

A custom support domain should provide:

- requester ticket creation and conversation history;
- organization-level visibility only for explicitly authorized managers;
- knowledge search and suggested articles;
- attachment upload through Minco's verified direct-object path;
- accessible keyboard and mobile layouts;
- explicit privacy/retention information;
- first-party login or one-time handoff exchange;
- a clear return link to the source application; and
- the same Ticketing API used by the widget.

Do not create one portal database per brand. Brand, domain and presentation are
configuration; ticket scope remains project/tenant data.

## Browser extension

Use an extension only when it adds value the site cannot provide. Good examples:

- user-confirmed screenshot capture;
- user-confirmed selected-text capture;
- lookup of an existing support ticket from an external system;
- adding an opaque external resource reference; or
- a side-panel portal for internal support staff.

The extension should:

- package all Manifest V3 extension logic locally;
- use the hosted portal as UI rather than download executable JavaScript;
- request `activeTab` instead of broad site access where possible;
- request screenshot or clipboard permissions only for an explicit feature;
- start interactive identity only from a user gesture;
- display exactly what page text or image will be submitted;
- use a dedicated authenticated BFF/handoff endpoint;
- validate the portal origin before navigation; and
- never become a hidden background page scraper.

An extension whose only behavior is opening the portal should not be shipped;
the browser bookmark or in-application launcher is simpler.

## Screenshots, voice and files

The launcher does not capture rich media automatically. After the portal opens:

1. the user chooses screenshot, voice or file;
2. the shared interaction policy validates kind, exact content type, count and
   size;
3. the application authorizes a direct object upload;
4. the browser sends bytes directly to private object storage;
5. the completion operation verifies key, size, SHA-256 and provider metadata;
6. the Ticketing domain attaches the verified object reference; and
7. optional transcription runs through the shared provider-neutral service.

A content type and checksum bind expected bytes; they do not prove that an
untrusted document is safe. Add quarantine, decoding or malware inspection when
the product threat model requires it.

## `postMessage` protocol

The embedded portal may send only:

```json
{ "type": "minco.support.ready" }
{ "type": "minco.support.close" }
{ "type": "minco.support.resize", "height": 640 }
```

The parent verifies the exact portal origin, exact iframe window and closed
message schema. Height is clamped. Do not use `*` as `targetOrigin` and do not
accept navigation, arbitrary URL or script commands from the iframe.

## Cost and wake sources

The support-entry browser module adds no server process. A static portal can use
S3/CloudFront. The API can use Lambda or an explicitly owned sidecar. Ticket
email ingress can use SES/S3/SQS or Microsoft Graph webhooks/delta recovery, but
those are separate selected adapters.

Declare every worker, queue, DLQ, subscription-renewal schedule and recovery
poller. The base Ticketing plugin must not silently install an always-on worker
or schedule.

## Qualification

Before promoting the example into the published plugin asset, prove:

```bash
cd examples/ticketing-entry
npm test
npm run check
```

Then add browser tests for keyboard use, modal/tab fallback, mobile viewport,
strict `postMessage`, exact-origin launch validation, CSP failure, blocked iframe
storage and no horizontal overflow. The plugin's Rust handoff and persistence
implementation requires its own unit, concurrency, SQLite and Axum contract
qualification.
