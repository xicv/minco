# Minco Ticketing support-entry reference

This dependency-free browser reference proves one entry contract across four
product surfaces:

1. a hosted support portal;
2. a floating or inline launcher in an application;
3. a packaged browser-extension side panel or tab; and
4. a headless backend-for-frontend or native-client API.

The portal is the canonical support UI. The launcher does not implement a second
ticket client: it obtains a short-lived launch URL from the host application's
same-origin backend and opens that portal in a cross-origin iframe or trusted
new tab.

## Run the checks

```bash
npm test
npm run check
```

The test suite uses only Node's built-in test runner.

## Browser integration

```html
<meta name="csrf-token" content="host-framework-token" />
<script type="module" src="https://support.example.test/_minco/ticketing/support-entry.js"></script>
<minco-support-launcher
  portal="https://support.example.test/"
  project="example"
  handoff-endpoint="/api/support/handoff"
  label="Get support"
></minco-support-launcher>
```

The same-origin `handoff-endpoint` is optional. A host application with a
special authentication or CSRF boundary can instead supply a callback:

```js
window.MincoSupport = {
  async issueHandoff(request) {
    const response = await hostApplicationClient.post('/api/support/handoff', request);
    return response.data;
  },
  async getContext() {
    return {
      route_name: 'orders.show',
      release_id: window.applicationReleaseId,
      resource_references: [
        { system: 'example', resource_type: 'order', resource_id: 'opaque-order-id' },
      ],
    };
  },
};
```

The host backend, not browser JavaScript, derives the trusted requester identity
and calls the private Minco Ticketing integration operation. Its browser response
contains only a short-lived launch URL on the exact configured portal origin.

## Privacy and security boundary

- Page URLs are reduced to scheme, authority and path by default.
- Query strings, fragments and URL user information are never forwarded.
- Selected text is accepted only through an explicit host/user action.
- Screenshots, voice and files are not captured automatically.
- The launch URL must use the exact configured portal origin.
- The iframe sets `referrerPolicy="no-referrer"` and validates both
  `postMessage` origin and source.
- Only ready, close and bounded resize messages are accepted.
- The iframe has a constrained sandbox and a new-tab fallback.
- The browser endpoint must be same-origin with the host application.
- A one-time handoff belongs in the URL fragment, not a query string.
- The browser never receives a Minco service credential.

A Chrome extension must package its own Manifest V3 launcher code. It can use
this JSON contract and hosted portal, but must not download and execute this
script remotely as extension logic. Request only the permissions needed for
features the user deliberately invokes.
