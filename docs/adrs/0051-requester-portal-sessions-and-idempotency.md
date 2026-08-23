# ADR 0051: Requester portal sessions and shared HTTP idempotency by reuse

## Status

Accepted.

## Context

ADR-0050 gave requesters a closed public projection and own-ticket
operations, but every requester call still requires a host-injected
principal — there is no durable requester session, so a portal cannot keep
a requester signed in across requests, and requester mutations have no
shared idempotency, so a retried reply duplicates.

Minco 1.12 already ships both halves as transport-agnostic services:
`minco-plugin-sessions` (opaque high-entropy tokens stored digest-only,
TTL/revocation, HMAC-signed CSRF tokens bound to a session, SQLite and
Postgres adapters with committed migrations) and
`minco-plugin-idempotency` (atomic begin/replay/conflict with bounded
response snapshots and the same adapters). Neither owns cookie policy or
HTTP wiring; that is deliberately application-owned.

## Decision

1. Ticketing reuses both services as-is and adds only an optional HTTP
   shim inside its own `HttpModule`. At install, the plugin resolves
   `SessionService`, `CsrfService` and `IdempotencyService` from the
   service registry when present; all three remain optional and the base
   plugin is unchanged without them.
2. `POST /_minco/ticketing/requester/sessions` consumes the one-time
   handoff from the existing sensitive header — validating project and
   portal origin, marking the handoff consumed with the exchange
   fingerprint for idempotent replay — and issues a session whose
   attributes bind `ticketing.project`, `ticketing.portal_origin` and the
   handoff-granted `ticketing.permissions`. The response sets
   `minco_ticketing_session=<token>` with `Secure; HttpOnly;
   SameSite=Lax; Path=/_minco/ticketing` (host-only) and returns a
   session-bound CSRF token in the body; the bearer appears exactly once.
3. `POST /_minco/ticketing/requester/logout` revokes the session.
4. Requester identity resolution order: a host-injected `Principal` wins
   (API/BFF callers keep their authority); otherwise a valid session cookie
   resolves to an identity whose permissions are exactly the
   handoff-granted set. Expired, revoked or unknown cookies are
   unauthenticated. Session-sourced mutations must present `x-minco-csrf`
   bound to the session; injected principals are not CSRF-checked.
5. Requester mutations opt into shared idempotency through the standard
   `Idempotency-Key` header (ADR-0026 semantics): same key plus same
   request fingerprint replays the stored response; a different fingerprint
   conflicts; an in-flight claim waits or rejects. The session exchange is
   keyed internally by the handoff digest so a browser retry cannot mint a
   second session.
6. Sessions configuration is explicit: `requester_session_ttl_seconds`
   (default 3600, at most 86400). Without the sessions plugin the two new
   operations fail closed with a truthful problem and no capability is
   claimed beyond what is enforced.

## Consequences

- Ticketing depends on `minco-plugin-sessions` and
  `minco-plugin-idempotency` as optional service lookups, not declared
  plugin dependencies; applications opt in by registering those plugins
  (memory store for tests, SQL adapters for durable profiles — their
  migrations already ship in the sqlx extensions).
- No new ticketing-owned session, token, CSRF or idempotency store exists;
  the SQLite ticketing schema is unchanged in this task.
- Browser portals now hold one HttpOnly session cookie plus an in-memory
  CSRF token; no credential reaches script or URLs.

## Alternatives considered

- **A ticketing-owned session table** — rejected: duplicates the sessions
  plugin and its audit surface; the Jobs precedent (ADR-0048) is to reuse.
- **Making the sessions plugin mandatory** — rejected: violates the
  optional-capability boundary established by ADR-0049 and the portal-first
  handoff flow that works without cookies.
