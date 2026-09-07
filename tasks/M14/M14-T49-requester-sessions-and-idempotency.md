---
id: M14-T49
title: Add durable requester portal sessions and shared HTTP idempotency
milestone: M14
status: active
priority: high
area: plugins/ticketing/requester
depends_on: [M14-T48]
operations:
  - createTicketingRequesterSession
  - endTicketingRequesterSession
owned_paths:
  - docs/DECISIONS.md
  - docs/adrs/0051-requester-portal-sessions-and-idempotency.md
  - plugins/minco-plugin-ticketing/**
  - tasks/M14/M14-T49-requester-sessions-and-idempotency.md
  - verification/1.9-performance-baseline.json
  - verification/source-manifest.json
  - verification/operational-evidence-validation.json
  - verification/static-validation.json
  - verification/publish-validation.json
  - verification/deep-review.json
  - docs/reference/generated/plugins.md
checks:
  - cargo test -p minco-plugin-ticketing --all-features --locked
  - cargo clippy -p minco-plugin-ticketing --all-targets --all-features --locked -- -D warnings
  - cargo minco plugin validate
  - cargo package -p minco-plugin-ticketing --locked
  - cargo minco check --with-cargo
---

# M14-T49 - Add durable requester portal sessions and shared HTTP idempotency

Stage B2 of the Ticketing sequence (ADR-0051). Reuse the existing Minco
1.12 `minco-plugin-sessions` (`SessionService`, `CsrfService`, digest-only
token storage, SQLite/Postgres adapters and migrations) and
`minco-plugin-idempotency` (`IdempotencyService`, begin/replay/conflict,
SQLite/Postgres adapters) instead of building another session or
idempotency system. Ticketing adds only a thin, optional HTTP shim.

## Goal

- `POST /_minco/ticketing/requester/sessions` atomically consumes a one-time
  handoff (existing sensitive header) and issues a durable requester portal
  session bound to the requester subject, project and portal origin, with
  granted permissions recorded in session attributes; response sets a
  host-only `Secure; HttpOnly; SameSite=Lax` cookie scoped to the ticketing
  path and returns a session-bound CSRF token. Identical exchange replays
  the original response; a different exchange of the same handoff fails
  closed.
- `POST /_minco/ticketing/requester/logout` revokes the session.
- Requester operations accept the session cookie as an alternative identity
  source (injected principals remain authoritative for API/BFF callers);
  session-sourced mutations require the `x-minco-csrf` header bound to the
  session.
- Requester mutations opt into shared HTTP idempotency via `Idempotency-Key`
  (same key + fingerprint replays the stored response; different fingerprint
  conflicts) whenever the idempotency plugin is registered.
- Without the sessions plugin the new operations fail closed with a truthful
  problem; the base plugin is unchanged.

## Acceptance

- Tokens are stored digest-only; no log, Debug or response ever contains a
  bearer beyond the one-time exchange response.
- Expired/revoked/unknown cookies are unauthenticated.
- Session identity cannot exceed the permissions granted by the handoff.
- Memory and SQLite behaviors match; OpenAPI/descriptor/manifest parity
  holds (27 → 29 operations).

## Non-goals

- A login screen, password or OIDC flow (application-owned).
- Append-only message persistence (B3), activity dispatch (B4).
- Any Jobs dependency.

## Evidence

Run 2026-08-24 in the `minco-task-m14-t49` workspace:

- `cargo test -p minco-plugin-ticketing --all-features --locked` — ok,
  **49 passed** (was 44; +5 session/idempotency proofs including real
  temporary SQLite).
- Feature matrix — `--no-default-features` (20), sqlite-only (28),
  `--features full` (49) — all ok.
- `cargo clippy -p minco-plugin-ticketing --all-targets --all-features
  --locked -- -D warnings` — clean; `rustfmt --check` over changed files —
  clean.
- `cargo minco plugin validate` — `[]`; `cargo package
  -p minco-plugin-ticketing --locked` — verified;
  `cargo minco contract sync --check` — passes;
  `scripts/docs/generate-reference.sh` — current.
- `cargo minco check --with-cargo` — result recorded below (the previous
  host-level mDNS blocker resolved; the gate passed end-to-end for
  M14-T48's tree on 2026-08-24).

Behavioral proofs (memory + SQLite): exchange consumes the handoff once and
sets `Secure; HttpOnly; SameSite=Lax; Path=/_minco/ticketing`; identical
replay returns the original grant (200); a different portal origin with the
same handoff digest conflicts (409); the cookie authenticates the
requester's own-ticket list without any injected principal; session replies
and logout require the session-bound `X-Minco-CSRF` token (403 without);
logout revokes and the cookie becomes 401; without the sessions/CSRF/
idempotency services the exchange fails closed with 503 and a truthful
problem; session handoff consumption and ticket handoff consumption are
independent one-time claims; bearer and CSRF values never appear in Debug
(redacted impls) and the cookie is set exactly once.
