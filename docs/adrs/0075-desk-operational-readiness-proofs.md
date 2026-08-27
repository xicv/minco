# ADR 0075: Desk operational readiness — load, cost, BFF boundary, database identity

## Status

Accepted.

## Context

The final Stage G evidence: bounded load behavior, the zero-compute
cost topology, the PeoplePlanner BFF integration boundary, and
separate database identity — after which Minco Desk can be described
as standalone private beta for the local profile.

## Decision

1. **Bounded load with correct pagination**: 100 sequential creates
   with interleaved listing prove the pagination bound holds while the
   corpus grows, and a full cursor walk collects exactly the corpus —
   no gaps, no duplicates. A local per-ticket latency envelope
   (< 500 ms on the composing machine) guards against accidental
   quadratic behavior; it is explicitly not a production SLO.
2. **Cost topology is zero-compute local-native**: the composition
   graph must not declare provisioned concurrency, NAT gateways or
   scheduled wakeups, and the database is a local SQLite file — the
   structural cost claim for the standalone profile.
3. **The desk is BFF-callable and rejects foreign browser origins**: a
   BFF service identity (`ticketing.agent.read`) reads the agent
   surface on behalf of its proxied users; the CORS policy is exact —
   a foreign origin's preflight is never echoed back and wildcards are
   forbidden. The browser talks to the BFF; the BFF talks to the desk;
   the desk never receives browser traffic.
4. **Database identity is separate from release identity**: two desks
   with different database URLs carry fully isolated data — the
   database identity is the configured file, not a release artifact or
   shared state. Each desk starts empty.

## Consequences

- The load proof is a regression guard, not a hosted performance
  qualification; exact-source hosted Linux evidence remains NOT RUN
  (no provider contact).
- PeoplePlanner itself is untouched (the continuation prompt forbids
  editing it in the Minco PR); the proof covers the Minco side of the
  boundary only.
- Stage G's local-profile evidence is complete; the standalone
  private-beta claim now rests on ADRs 0072–0075.
