# ADR 0071: Clarification is a durable loop with resume checkpoints

## Status

Accepted.

## Context

Stage F's clarification requirement: when requirements are missing or
contradictory, automation (or an agent) must not guess. The continuation
prompt defines a first-class loop — durable clarification draft →
policy/human send decision → requester reply → resume from checkpoint.

## Decision

1. **A clarification is durable ticket state** (`Clarification`,
   migration 0011): a reason (`missing_requirement` /
   `contradictory_requirement`), 1..8 bounded unique questions, and an
   opaque bounded **checkpoint** token the creating context defines.
   The state machine is `draft → sent → answered` with `withdrawn` for
   unsent drafts; every transition is domain-validated and
   exactly-once (send only from draft, answer only from sent, one
   answer per question in order).
2. **Drafting never reaches the requester**: drafts are agent-only
   (`ticketing.manage` to draft). The **send decision is human**
   (`send_clarification`, `ticketing.manage`) — automation may draft
   questions, but only this decision asks the requester. A requester
   attempting to answer a draft sees not-found, not a state leak.
3. **The requester answers once, own-ticket only**: the reply use case
   authorizes `ticketing.read`, verifies the ticket's requester matches
   the authenticated subject, requires one answer per question, and
   transitions `sent → answered`. The requester projection carries
   questions and the requester's own answers only — never the
   checkpoint, creator, or internal reason vocabulary beyond what the
   questions themselves state.
4. **Resume is explicit**: an answered clarification exposes its
   checkpoint on the agent listing (`agent.read`); agents or
   automation re-read it to resume from exactly where work paused.
   Nothing auto-resumes: resuming is a new, explicit decision
   consistent with ADR-0070's authority discipline.
5. Five operations ship: agent create/list/send plus requester
   list/reply, aligned across contract, descriptor and distribution;
   the generated boundary enforces the bounded shapes at extraction.

## Consequences

- The clarification loop composes with ADR-0070 automation: an
  automation proposal that hits missing or contradictory requirements
  can persist a checkpoint and draft questions; the human sends; the
  requester answers; the agent resumes with full context.
- Checkpoints are opaque tokens: their interpretation belongs to the
  creating context, so the loop stays engine-neutral.
- Withdrawal is draft-only and terminal; a sent clarification can
  never be silently withdrawn from a requester who has already seen
  the questions.
