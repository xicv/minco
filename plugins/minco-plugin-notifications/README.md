# minco-plugin-notifications

Provider-neutral notification delivery plus an explicit rich outbound-mail
contract for Minco applications.

The existing `Notification`, `NotificationSink`, and `NotificationService` APIs
remain available for email-shaped notices, webhooks, in-app alerts, and
developer feedback. New email-specific work should use `MailMessage` and
`MailService` for CC/BCC, reply-to, text and HTML alternatives, attachments,
inline content, safe headers, provider tags, acceptance receipts, deterministic
capture, fallback policy, and submission observation.

## Compose a message

```rust
use minco_plugin_notifications::{MailAddress, MailAttachment, MailMessage};

let message = MailMessage::builder("invoice.ready", "Your invoice is ready")
    .to(MailAddress::named("person@example.com", "Example Person")?)
    .cc(MailAddress::new("accounts@example.com")?)
    .bcc(MailAddress::new("audit@example.com")?)
    .reply_to(MailAddress::new("support@example.com")?)
    .text("Your invoice is attached.")
    .html("<p>Your invoice is <strong>attached</strong>.</p>")
    .attachment(MailAttachment::attachment(
        "invoice.pdf",
        "application/pdf",
        invoice_bytes,
    )?)
    .tag("message_type", "transactional")
    .build()?;
```

Validation occurs before transport I/O. The message is bounded to 50 envelope
recipients, one text and/or HTML body, 32 attachments, 25 MiB of raw attachment
bytes, safe application headers, bounded tags and metadata, and a rendered MIME
message below Minco's provider boundary. BCC recipients stay in the transport
envelope and are never rendered into MIME headers.

Unicode display names and subjects are encoded and folded. Custom application
header values are printable ASCII, Minco/SES control headers are reserved, and
every physical header line is limited to 998 bytes. Tests parse the result with
an independent RFC 5322/MIME parser as well as checking the transmitted bytes.

Attachment bytes are runtime values and deliberately do not implement Serde.
Durable mail intent should use an application-owned schema containing object
references, digests, lengths, and media types instead of serializing raw bytes
into an outbox or queue record.

Rendering is intentionally in-memory and may transiently retain raw attachment
bytes, Base64 output, and the final MIME buffer together. Use an
access-controlled object-storage link for large files and size the runtime from
measured peak memory rather than treating the 25 MiB raw limit as a recommended
payload size.

## Send and observe

```rust
use minco_plugin_notifications::{
    MailService, TracingMailObserver,
};
use std::sync::Arc;

let mail = MailService::single(transport, Arc::new(TracingMailObserver))?;
let receipt = mail.send(message).await?;
```

`MailReceipt` means that the selected transport accepted the submission and
returned a provider identifier. It does not mean that the recipient mailbox has
received the message.

Submission observation includes prepared, attempting, failed, and accepted
states. The tracing observer emits the Minco message UUID, stable topic,
transport, attempt, coarse error class, and duration. It excludes recipients,
display names, subject, body, attachments, metadata values, and provider
message IDs.

Observers have independent bounded execution windows and run concurrently, so
a slow observer cannot prevent a later observer from receiving the same event
or hold the submission path indefinitely. Invalid acceptance receipts emit an
ambiguous failed-attempt observation before returning an error.

## Failure and fallback semantics

`MailErrorKind` separates invalid configuration, authentication, rejection,
throttling, unavailability, protocol violations, and ambiguous outcomes.
`MailService` advances to a fallback only after an explicitly retry-safe
throttled or unavailable result. It stops immediately after an ambiguous result
because the first provider may already have accepted the message.

A stable message UUID can be combined with Minco's idempotency and transactional
outbox capabilities when business state and mail intent must be committed
together. No exactly-once email-delivery guarantee is claimed.

## Deterministic tests

```rust
use minco_plugin_notifications::{
    MailService, MemoryMailObserver, MemoryMailTransport,
};
use std::sync::Arc;

let transport = Arc::new(MemoryMailTransport::default());
let observer = Arc::new(MemoryMailObserver::default());
let mail = MailService::single(transport.clone(), observer.clone())?;

mail.send(message).await?;
transport.assert_sent_count(1).await;
transport.assert_sent_to("person@example.com").await;
assert_eq!(observer.events().await.len(), 3);
```

`MemoryMailTransport` captures the complete message without network access,
credentials, sleeps, or provider state.

Use `FakeMailTransport` when the test must select a failure class. It records
typed attempts, consumes queued failures once and drives the real
`MailService` retry/fallback rules. Its `Debug` output omits recipients,
subjects, bodies, attachments and metadata values. A fake acceptance remains
submission-policy evidence, not proof of mailbox delivery.

The package distribution describes the plain notifications constructor and
therefore advertises only `notifications.send`. A runtime
`NotificationsPlugin` descriptor adds `mail.send` only when constructed with an
explicit `MailService`; graph tests reject one-sided rich-mail selection.

## Browser inbox with Mailpit

From the Minco repository root:

```bash
docker compose -f compose.mail.yml up -d --wait
plugins/minco-plugin-notifications/scripts/mailpit-ready.sh
plugins/minco-plugin-notifications/scripts/mailpit-smoke.sh
```

Use `MailpitTransport::default()` to submit to `127.0.0.1:1025`, then open
`http://127.0.0.1:8025`. The adapter refuses non-loopback plaintext SMTP
endpoints, implements command timeouts and multiline responses, dot-stuffs DATA,
and treats connection loss after DATA as an ambiguous result. The pinned
container uses its native health command, explicit CPU/memory/PID limits, and no
automatic restart policy. The host smoke verifies rich SMTP capture through the
Mailpit API. Mailpit reconstructs BCC from envelope metadata in its raw-message
API, so the byte-exact SMTP test remains the authority that Minco's transmitted
MIME omitted the BCC header. Mailpit's CSS/font preview control does not block
remote images or tracking pixels; do not open untrusted HTML without separate
browser/network isolation.

```bash
docker compose -f compose.mail.yml down
```

Add `-v` only when the captured local inbox should also be deleted.

## Final delivery evidence

`MailDeliveryEvent` represents submission, delivery, permanent/transient/
undetermined bounce, complaint, reject, delay, rendering failure, and optional
open/click/subscription evidence.
`MemoryMailDeliveryEventSink` provides deterministic source-event
deduplication. `TracingMailDeliveryEventSink` also deduplicates and emits a
digest of the source event ID plus bounded privacy-safe fields. The memory sink
is test-scoped; the tracing sink caps its in-process window at 4,096 source IDs.
Neither persists across restarts, so durable replay protection is
application-owned. The tracing sink deliberately omits raw source IDs, provider
message IDs, and customer-provided values.

Provider adapters normalize their own event envelopes before passing events to
these sinks.
