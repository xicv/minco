# ADR 0053: Truthful bootstrap capabilities and explicit ticketing store selection

## Status

Accepted.

## Context

Two Stage-B review findings remain open after B3:

1. `TicketingPlugin` implements `Default` by selecting the in-memory
   store. Any construction path that relies on the default silently
   selects non-durable storage; the continuation review names accidental
   production memory as a stop condition.
2. `getTicketingBootstrap` hard-codes `screenshot_enabled`,
   `voice_enabled` and `file_enabled` as `true`, although no ticketing
   operation accepts a screenshot, voice recording or file upload. The
   plugin therefore claims capabilities it does not provide, and the
   portal-session capability introduced in Stage B2 is not reported at
   all.

## Decision

1. `Default for TicketingPlugin` is removed. The three constructors stay
   explicit: `memory()` (deterministic test/dev), `sqlite(pool)` and
   `new(store)` (custom durable). The facade's dev registration remains an
   explicit `memory()` call, visible at the composition site rather than
   implied by a trait default.
2. The bootstrap becomes truthful. The legacy capture toggles report
   `false` until real operations exist. A new additive `capabilities`
   object reports per-feature truth: `portal_sessions` is true only when
   the sessions and CSRF services are registered at install; `history` is
   true (Stage B3 pagination); files, screenshots, voice, knowledge,
   email and automation are false. The object is a ticketing-local type
   serialized next to the interaction crate's bootstrap shape (serde
   flatten): extending the published `minco-interaction` type would make
   this plugin version depend on an unpublished interaction change and
   fail package verification against the registry.
3. Truth is computed by the service from the registered portal services
   and the implemented operations — never hard-coded — so later stages
   flip capability bits by implementing features, not by editing claims.

## Consequences

- Portals can render only what is real; dead UI that implied uploads
  disappears from the surface contract.
- Compile-time break for any external `TicketingPlugin::default()` user —
  acceptable for a 0.x Beta plugin and consistent with the honesty goal.
- The wire shape gains one additive `capabilities` object; consumers that
  ignore unknown JSON fields are unaffected.

## Alternatives considered

- **Keep the defaults and document them** — rejected: documentation does
  not prevent silent non-durable selection.
- **Panic in `memory()` outside tests** — rejected: the memory store is
  the sanctioned deterministic test profile and the facade dev path.
