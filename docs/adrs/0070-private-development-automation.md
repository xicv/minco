# ADR 0070: Private development automation as reviewed proposals

## Status

Accepted.

## Context

Stage F asks for private AI/development automation over Jobs, with the
continuation prompt's hard constraints: profiles (off/assist/supervised/
autonomous), automation kept private from requester DTOs, human review
(always/risk_based/disabled) where disabled still requires trusted
verification, a default authority-exclusion list (merge, release,
publish, deploy, production mutation, secret management, workflow
dispatch), and model output as proposal/result — never authority.

## Decision

1. **Automation is opt-in and profile-gated** (`AutomationConfig` on the
   ticketing config, default `off`): both the HTTP trigger and the job
   handler fail closed (`Configuration` error / permanent job failure
   `ticketing.automation_disabled`) when the profile is off, so a queued
   command can never run for an application that has not opted in.
   Review `disabled` is unconfigurable while any profile is enabled —
   trusted deterministic verification does not ship, so nothing may skip
   review.
2. **The durable command is `ticketing.run-development-automation`**
   (ADR-0054 envelope discipline: dedupe per ticket+requester, overlap
   per ticket, project partition, bounded retry, one-hour deadline). The
   handler assembles a deterministic proposal from ticket context — a
   local model with no external calls; a real model arrives later behind
   the same command, still producing proposals.
3. **Authority is structurally excluded**: requested actions are checked
   against the fixed exclusion list before anything persists
   (`validate_automation_actions`, fail closed with
   `ticketing.automation_action_excluded`), and every stored record is a
   proposal with an explicit human decision transition (`awaiting_review
   → accepted | rejected`, exactly once). Accepting authorizes the
   proposal record only — executing accepted proposals is later,
   explicit work with its own evidence.
4. **Automation is agent-only** (`/agent/...` routes, `agent.read` for
   listing, `manage` for triggering and deciding); proposal state never
   crosses into requester projections or public schemas. SQLite
   migration 0010 stores proposals with a ticket foreign key.

## Consequences

- The clarification loop (durable clarification drafts, checkpointed
  resume) remains open Stage F work, as do real model adapters and
  execution of accepted proposals.
- The exclusion list is code, not configuration: widening it is a
  reviewed decision, never a runtime toggle.
- Profiles `supervised` and `autonomous` differ in proposal scope today;
  if autonomy ever means execution, it must ship with trusted
  verification evidence first.
