---
title: Ticketing Support Entry
description: Add a portal-first, project-scoped support flow with atomic one-time handoffs.
---

# Ticketing Support Entry

The beta Ticketing plugin provides a project-scoped support domain and one
portal-first entry contract for embedded launchers, dedicated support pages,
browser extensions, native clients, and server integrations. The launcher is a
thin adapter into an application-owned portal; it is not a second ticket client.

## Compose the plugin explicitly

Enable the Ticketing facade feature only in an application that also supplies
the plugin's health, identity, object-storage, notification, audit, and event
dependencies. Inspect its archive-visible contract before composition:

```bash
cargo minco plugin explain ticketing --json
cargo minco plugin validate
```

The plugin is disabled by default. Its memory store is deterministic and for
tests only. The SQLite profile uses its own database and explicit migration;
Lambda startup does not migrate it.

## Keep identity and browser context separate

The browser sends only bounded, redacted context through a same-origin BFF or
an explicit host callback. The BFF derives the authenticated subject, project,
permissions, and return-location policy server side. Browser attributes never
assert trusted identity, tenancy, or authorization.

Page title and selected text are host opt-in. Screenshot, voice, and file
capture remain explicit user actions. Page URLs omit user information, query,
and fragment data.

## Exchange one handoff atomically

The integration endpoint issues a short-lived, high-entropy handoff. The
launcher accepts it only in a `#handoff=` fragment at the exact portal origin.
Ticketing stores only its digest and consumes it in the same transaction that
creates the first authoritative ticket and requester-session result.

An identical retry returns that result even after the issue-time TTL passes. A
different replay, wrong project, wrong portal, expired unused handoff, or
unknown digest fails closed. Requester projections exclude internal notes,
provider payloads, object keys, and attachment digests.

## Evidence boundary

Node and Playwright tests exercise the packaged launcher, including keyboard
focus, mobile layout, reduced motion, zoom, popup fallback, strict messages,
and iframe readiness. Rust tests exercise authorization, state transitions,
optimistic revisions, atomic SQLite handoff exchange, external-message
idempotency, requester projections, descriptor conformance, and OpenAPI
inventory.

This is local source evidence. It does not prove a branded portal, browser-store
publication, live mailbox/provider behavior, deployment, or production use.
