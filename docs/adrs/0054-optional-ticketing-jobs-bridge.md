# ADR 0054: Optional Ticketing-to-Jobs bridge with transactional enqueue

## Status

Accepted.

## Context

Ticketing mutations now need deferred side effects (notify the requester
when an agent replies publicly). Minco 1.12 ships the full durable job
lifecycle (ADR-0048); the continuation contract forbids Ticketing from
building a second queue/lease/retry system, forbids hidden workers, and
requires that a job accompanying a ticket mutation commit atomically with
it — `commit ticket` followed by `submit_durable` leaves a crash window.

## Decision

1. The bridge is an optional Cargo feature `jobs` on
   `minco-plugin-ticketing`, never in the default set. With the feature
   off the crate has no dependency on the jobs plugin and its behavior is
   unchanged. One command ships in this stage:
   `ticketing.deliver-public-notification` v1. The `transcribe-audio` and
   `classify-ticket` names and policies are reserved for the media/AI
   stages; no handler is registered for them now, because a handler that
   can only fail is dead surface.
2. The command payload carries bounded identifiers only
   (`project_id`, `ticket_id`, `message_id`). Envelope policy: dedupe
   `notification:{ticket}:{message}` (same key plus same semantic
   fingerprint returns the existing job; a conflicting fingerprint fails
   closed in the jobs store), overlap key `ticket:{ticket}` so one
   ticket's notifications serialize, partition equal to the project
   routing reference, bounded exponential retry, a one-hour deadline so a
   stale acknowledgement never sends, and causation equal to the
   triggering correlation ID.
3. The handler is real: it loads the ticket and message through the
   ticketing store, projects the public message, and delivers through the
   notifications plugin's `NotificationService` port with the requester's
   own addressing; a missing ticket or message is a permanent failure
   (`ticketing.notification_target_missing`), and a send failure is
   retryable (`ticketing.notification_send_failed`) under the bounded
   policy and deadline — the handler never fabricates success. Handlers
   are registered statically: the composition root calls the bridge's
   registration function on its own `JobHandlerRegistry` before building
   `JobsServices`; no runtime scanning, no plugin retro-fit.
4. Pattern A atomicity: store requests may carry bounded job records; the
   `sqlite` profile enqueues them through a ticketing-owned
   `TicketingJobEnqueue` port inside the same SQL transaction as the
   mutation (the composition adapts the released `SqliteJobStore`
   `enqueue_in` to the port — adapters implement ports owned by the
   application layer). The memory test profile records the records for
   deterministic inspection. Records present without a configured sink
   fail closed; nothing is dropped silently. Ticketing never calls
   `submit_durable` for a job it claims is atomic with a mutation.
5. No topology: the bridge adds no queue, worker, schedule or provider
   resource; the jobs plugin owns all of that, and only when the
   application selects it. No `ticketing.jobs` capability is declared in
   the descriptor or the distribution manifest: plugin conformance
   requires the two capability lists to match exactly, and a static
   manifest cannot express feature-conditional capabilities — claiming the
   bridge unconditionally would be untruthful in default builds. The
   Cargo feature and the `notify_requester_on_public_reply` configuration
   are the truthful opt-in surface.

## Consequences

- Application composition opts in explicitly: enable the feature,
  register the jobs plugin, adapt the shared-pool enqueue, register the
  handlers, then build the worker profile the jobs ADR prescribes.
- The notification job and the ticket reply commit or roll back together
  on SQLite; a crash between reply and notification cannot lose the job.
- Later stages add commands by registering new typed handlers — the seam
  and policies are already proven by two executed paths (durable submit
  and transactional enqueue).

## Alternatives considered

- **A ticketing-owned outbox projected into Jobs later** — the correct
  Pattern B, but unnecessary while one SQL database backs both stores;
  revisited if a custom store cannot share the transaction.
- **`submit_durable` after commit** — rejected: the crash window is the
  defect.
- **A companion plugin crate** — rejected: no second implementation to
  prove the seam yet, and packaging cost is real.
