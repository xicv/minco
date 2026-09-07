---
id: M14-T47
title: Qualify the ticketing agent console in real browsers
milestone: M14
status: active
priority: high
area: plugins/ticketing/agent-console
depends_on: [M14-T45]
operations: []
owned_paths:
  - examples/ticketing-agent-console/**
  - tasks/M14/M14-T47-agent-console-browser-qualification.md
checks:
  - cd examples/ticketing-agent-console && npm ci && npm run check && npx playwright test --project=chromium
---

# M14-T47 - Qualify the ticketing agent console in real browsers

## Goal

Give the Stage A agent console real-browser qualification, following the
established `examples/ticketing-entry` harness pattern: serve the exact
plugin-shipped assets (`plugins/minco-plugin-ticketing/assets/agent-console.*`)
and fulfill the agent API transport from deterministic fixtures, so the UI
journeys, not the server, are what is under test. Server behavior is already
proven by the plugin's Axum contract tests (M14-T45).

## Coverage

- bootstrap renders brand and disables controls the principal lacks
- views switch and load the correct filtered list queries
- cursor pagination loads the next page and appends rows
- current-page search filters rows without any request
- selection loads the detail view with messages and metadata
- public reply and internal note submit exact bodies and refresh the list
- management submits the exact atomic payload with If-Match
- stale If-Match (412) shows the conflict message and reloads the ticket
- create posts the exact create payload and opens the new detail
- keyboard operation: rows are focusable and Enter selects
- loading, empty and forbidden states render truthfully
- dark scheme renders without remote resources (no network beyond fixtures)

## Evidence

Run 2026-08-23 in the `minco-task-m14-t47` workspace:

- `cd examples/ticketing-agent-console && npm ci` — installed from the
  committed `package-lock.json` (@playwright/test 1.62.1, no runtime deps).
- `npm run check` (`node --check` over the plugin-shipped console script) — ok.
- `npx playwright install chromium firefox` + `npx playwright test
  --project=chromium` — **16/16 passed**; `--project=firefox` —
  **16/16 passed**.

The harness serves the exact plugin assets
(`plugins/minco-plugin-ticketing/assets/agent-console.*`) and fulfills the
agent API transport from deterministic fixtures, mirroring the established
`examples/ticketing-entry` pattern; server behavior is proven by the
plugin's Axum contract tests.

**Real defect found and fixed**: the shipped console read snake_case
`page.has_more`/`page.next_cursor` while `ResourceCollection` serializes
camelCase `hasMore`/`nextCursor`, so "Next page" never appeared and the
cursor was lost — pagination was broken in the browser despite passing all
HTTP contract tests. Fixed in `agent-console.js`; the conflict-recovery
message on 412 also no longer flashes away during reload. `cargo test -p
minco-plugin-ticketing --all-features --locked` re-passes (38/38).

Coverage (both engines): bootstrap brand rendering, capability-based control
disabling, exact view filter queries (Active status set, Mine assignee
filter), cursor pagination replacement, truthful current-page search with
zero extra requests, detail rendering incl. internal-note styling,
keyboard-only selection, exact reply/note/management/create payloads with
If-Match, 412 conflict recovery and reload, empty/forbidden/unauthenticated
states, and a no-remote-resources + dark-scheme pass.

Not covered by this harness (recorded honestly): 200% zoom measurements,
screen-reader output, reduced-motion and mobile safe-area checks, and
embedded Web Component / BFF integration modes.
