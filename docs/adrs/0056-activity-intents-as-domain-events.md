# ADR 0056: Activity intents dispatch as domain events

## Status

Accepted.

## Context

Two open review findings share one root: `ticketing_activity_intents`
rows are written transactionally with every mutation (with a
`published_at` column) but nothing ever publishes them, and the ticketing
descriptor requires `events.publish`/`events.outbox` capabilities at
install while the plugin never touches the events service. The semantic
separation (ADR-0048 era) says domain events are facts with zero-or-many
consumers — exactly what these intents are.

## Decision

1. Activity intents are the local, transactionally-committed record of
   ticketing facts. A new explicit bounded pass,
   `TicketingService::dispatch_pending_activity(project, limit)`,
   publishes each unpublished intent as a `DomainEvent` through the events
   service publisher — type = intent kind (for example
   `ticketing.requester_replied`), aggregate type `ticketing.ticket`,
   aggregate id = ticket id, correlation = the intent's correlation id,
   payload = the bounded intent payload (identifiers, revision, status;
   never bodies or internal data) — and then marks the intent published.
2. Dispatch is never scheduled implicitly: applications invoke it
   request-assisted, from an explicit worker profile, or as an operator
   command, exactly as the events plugin prescribes.
3. Delivery is at-least-once (publish, then mark): a crash between the two
   steps replays one event on recovery. Event consumers are already
   required to be idempotent; a duplicate fact is harmless, a lost one is
   not. A publish failure stops the pass and leaves the remaining intents
   pending for the next explicit pass.
4. The events service moves from a required-but-unused dependency to a
   used one: plugin install passes the resolved `EventServices` into the
   ticketing service.

## Consequences

- The `published_at` column becomes real lifecycle state; intents stop
  accumulating silently.
- Realtime subscribers and audit consumers can subscribe to ticket facts
  through the standard events surface without any ticketing-specific
  coupling.
- The events outbox is deliberately not used as a second relay: the intent
  row already commits with the mutation, which is the durability guarantee
  the outbox would provide.

## Alternatives considered

- **Dropping the events requirement** — rejected: ticket facts are exactly
  domain events, and the capability is real once dispatched.
- **Routing dispatch through the events outbox** — rejected for now: it
  would relay the same durability the intent row already provides and add
  a second store transaction per fact.
