---
title: Events, Notifications, and Mail
description: Publish domain facts and deliver rich, low-cost mail through explicit ports, outboxes, provider receipts, and privacy-safe lifecycle events.
---

# Events, Notifications, and Mail

The events plugin describes domain events and transactional-outbox ports. The
notifications plugin keeps the existing generic notification API and adds a
first-class mail contract. The audit plugin records append-only business history
independently of operational logs.

## Enable the capabilities

```bash
cargo minco plugin enable events --dry-run --json
cargo minco plugin enable notifications --dry-run --json
cargo minco plugin enable audit --dry-run --json
```

Compile-time features and explicit constructors still decide which adapters are
present. Enabling metadata alone does not choose SES, SQS, Mailpit, a webhook
client, or a database table.

## Send a rich message

Use `MailMessage` for email-specific behavior instead of squeezing mail into the
generic `Notification` shape.

```rust
use minco_plugin_notifications::{MailAddress, MailAttachment, MailMessage};

let message = MailMessage::builder("invoice.ready", "Your invoice is ready")
    .to(MailAddress::named("person@example.com", "Example Person")?)
    .cc(MailAddress::new("accounts@example.com")?)
    .reply_to(MailAddress::new("support@example.com")?)
    .text("Your invoice is attached.")
    .html("<p>Your invoice is <strong>attached</strong>.</p>")
    .attachment(MailAttachment::attachment(
        "invoice.pdf",
        "application/pdf",
        invoice_bytes,
    )?)
    .header("List-Unsubscribe-Post", "List-Unsubscribe=One-Click")
    .tag("message_type", "transactional")
    .build()?;

let receipt = mail.send(message).await?;
```

The envelope validates recipients, duplicate addresses, reply-to, subject,
bodies, attachment sizes, inline content IDs, safe custom headers, tags, and
metadata before any transport runs. BCC recipients are never rendered into the
local MIME headers.

The application message UUID and stable topic are the correlation authority.
The receipt adds the selected transport, provider message ID, acceptance time,
and attempt number.

## Compose SES explicitly

```rust
use minco_aws_adapters::ses::{SesMailTransport, SesMailTransportConfig};
use minco_plugin_notifications::{
    MailAddress, MailService, TracingMailObserver,
};
use std::sync::Arc;

let mut config =
    SesMailTransportConfig::new(MailAddress::new("no-reply@example.com")?)?;
config.configuration_set = Some("application-mail".into());
config.default_tags.insert("environment".into(), "production".into());

let transport = Arc::new(SesMailTransport::new(ses_client, config)?);
let mail = MailService::single(transport, Arc::new(TracingMailObserver))?;
```

SES requests support To, CC, BCC, reply-to, text and HTML alternatives,
attachments, inline content IDs, custom headers, message tags, configuration
sets, multi-region endpoint IDs, tenants, and sending-identity ARNs. Minco adds
reserved `minco_message_id` and `minco_topic` tags so a provider event can be
joined to application state without logging a recipient.

## Treat retries as a correctness decision

Mail delivery is not a normal idempotent HTTP call. A provider can accept a
message while the caller receives a timeout or malformed response. Retrying that
outcome can send a duplicate.

`MailErrorKind` therefore separates:

- invalid message, configuration, authentication, and rejection: never retry;
- explicit throttling or unavailability: safe to retry after backoff or use a
  configured fallback;
- ambiguous outcome: reconcile the application UUID and provider events before
  retrying.

The SES adapter disables transparent AWS SDK retries for `SendEmail` so this
classification remains observable. `MailService` stops immediately after an
ambiguous outcome and only advances to a fallback transport after an explicitly
retry-safe failure.

Use a stable application message UUID with Minco's idempotency plugin when a
command may be submitted repeatedly. Use the events plugin's transactional
outbox when mail intent must be committed atomically with business state. The
outbox prevents concurrent application workers from executing the same intent,
but the provider boundary still requires receipt and event reconciliation.

## Choose the lowest-cost delivery shape

Start with the smallest architecture that meets the requirement:

| Requirement | Delivery shape |
| --- | --- |
| Ordinary transactional mail | Direct SES API call, shared IP, structured logs |
| Request latency must not include delivery | Request-assisted outbox dispatch or an explicit worker |
| Burst absorption or durable delayed delivery | Existing outbox plus SQS/Lambda worker and DLQ |
| Delivery/bounce/complaint state | SES configuration-set events into an existing event destination |
| Open/click analytics | Enable only with explicit product and privacy need |
| Dedicated IP or advanced deliverability tooling | Add only after measured volume/reputation need |

Do not provision a queue, periodic poller, dedicated IP, or per-message custom
metric merely because mail is enabled. Queues and schedules create additional
operations and failure modes. If a queue is justified, reuse the existing Minco
outbox and SQS worker instead of creating a mail-only scheduling subsystem.

Large attachments should normally live in object storage with a short-lived
application link. Besides reducing provider payload cost, this keeps serialized
outbox or SQS records below their transport boundaries.

## Observe the complete lifecycle

`MailObserver` receives privacy-safe lifecycle events:

```text
prepared -> attempting -> accepted -> delivered
                         \-> delayed / bounce / complaint / reject
```

Optional rendering, open, click, and subscription events use the same schema.
The tracing observer records only the message UUID, stable topic, transport,
event kind, attempt, failure class, and provider message ID. It excludes
recipient addresses, display names, subject, bodies, attachment names, and
metadata values.

SES events can arrive directly or inside SNS/EventBridge envelopes:

```rust
use minco_aws_adapters::ses::parse_ses_event;

let event = parse_ses_event(request_body)?;
mail.observe_provider_event(event).await?;
```

The parser requires the Minco correlation tags, normalizes the provider event,
and discards recipient details. Keep topic values bounded and stable; never put
customer IDs, email addresses, or arbitrary request values into metric
dimensions.

Operational lifecycle events explain delivery. Audit history explains the
business action that requested delivery. Neither substitutes for the other.

## Preview mail locally in a browser

Start the pinned Mailpit container:

```bash
docker compose -f compose.mail.yml up -d
```

Configure `MailpitTransport::default()` or SMTP `127.0.0.1:1025`, then open
`http://127.0.0.1:8025`. The local adapter accepts loopback addresses only and
supports the same rich message shape as the SES adapter.

```rust
use minco_plugin_notifications::{MailService, MailpitTransport, TracingMailObserver};
use std::sync::Arc;

let mail = MailService::single(
    Arc::new(MailpitTransport::default()),
    Arc::new(TracingMailObserver),
)?;
```

Stop it with `docker compose -f compose.mail.yml down`. Add `-v` only when the
captured inbox should be deleted.

## Test without a container

`MemoryMailTransport` is the default deterministic fake for unit and application
tests. It captures the complete message and provides focused assertions without
network, AWS credentials, or sleeps.

```rust
let transport = Arc::new(MemoryMailTransport::default());
let observer = Arc::new(MemoryMailObserver::default());
let mail = MailService::single(transport.clone(), observer)?;

mail.send(message).await?;
transport.assert_sent_count(1).await;
transport.assert_sent_to("person@example.com").await;
```

Keep bounded real-AWS tests separate. Use SES mailbox simulator addresses to
exercise delivery, bounce, complaint, suppression, and out-of-office behavior
without sending to real recipients. A real-provider smoke must name the account,
Region, verified identity, configuration set, exact fixture, and cleanup path.
