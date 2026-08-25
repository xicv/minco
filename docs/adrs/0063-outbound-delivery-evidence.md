# ADR 0063: Outbound ticketing email records evidence and reconciles before retry

## Status

Accepted.

## Context

`ticketing.deliver-public-notification` sent email through the
fire-and-forget notifications port: the job succeeded when the port
accepted the send, and nothing distinguished provider acceptance from
delivery. Stage D2 requires the opposite discipline: provider acceptance
is not delivery; bounce, complaint and delay feedback must become durable
evidence; ambiguous transport results must recover through
reconciliation; and no outcome may trigger a blind retry that could
duplicate outbound mail.

The notifications plugin already ships the observable mail path
(ADR-0007-era M14-T07 work): `MailService` returns a `MailReceipt`
(provider, provider message id, attempt), `MailError` classifies
`Ambiguous`, and `MailRetryAdvice::ReconcileBeforeRetry` states the
correct ambiguity policy. Ticketing adopted none of it.

## Decision

1. Email-channel public replies are submitted through `MailService`, and
   every decision-grade fact is recorded as append-only
   `ticketing_delivery_evidence` (migration 0005): `accepted`
   (never a delivery claim), `ambiguous` (with the mail error kind),
   `permanent_failure`, and `feedback` (bounce/complaint/delay).
2. Before any send the handler reconciles: an existing `accepted` row for
   the exact (project, ticket, message) suppresses the resend. Job
   redelivery and ambiguous retries therefore cannot duplicate mail —
   the store is the reconciliation authority; a provider-side
   reconciliation query is out of scope until there is a real provider.
3. `MailRetryAdvice` maps one-to-one onto job outcomes:
   `SafeAfterBackoff` → retryable with no evidence row (transient
   attempts stay observable in the mail observer), `ReconcileBeforeRetry`
   → an `ambiguous` evidence row plus a retryable failure with the
   distinct code `ticketing.notification_ambiguous`,
   `Never` → a `permanent_failure` row plus a permanent failure.
4. Bounce/complaint/delay feedback enters through the authorized use case
   `TicketingService::record_delivery_feedback` (`ticketing.ingest`
   principal only), which validates bounded provider identifiers and
   requires an existing public-reply target; orphan feedback fails closed
   without persisting. The provider-side wiring (SES feedback) arrives
   with the receiving-rule binding task.
5. An email-channel requester with no configured mail service is a
   permanent configuration failure (`ticketing.notification_mail_unconfigured`);
   the channel is never silently downgraded to in-app.
6. In-app notifications keep the fire-and-forget port unchanged.

## Consequences

- The jobs bridge composition root now selects the mail path via
  `NotificationsPlugin::mail_service()`; base ticketing without the jobs
  feature is unchanged, and the sqlite natural key makes redelivered
  acceptance appends idempotent.
- Operator evidence answers "what did the provider tell us" (acceptance,
  ambiguity, feedback) and never asserts delivery.
- The SES receiving-rule/S3/SQS plan binding and the Rustack live seam
  proof remain open (slice 3b part 2).
