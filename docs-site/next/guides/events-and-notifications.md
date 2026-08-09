---
title: Events, Notifications, and Mail
description: Publish domain facts and deliver rich low-cost mail through explicit ports, acceptance receipts, provider events, and local capture.
---

# Events, Notifications, and Mail

The events plugin describes domain events and transactional-outbox ports. The
notifications plugin retains the generic notification API and adds an explicit
rich-mail contract. The audit plugin records append-only business history
independently of operational logs.

## Enable the capabilities

```bash
cargo minco plugin enable events --dry-run --json
cargo minco plugin enable notifications --dry-run --json
cargo minco plugin enable audit --dry-run --json
```

Compile-time features and explicit constructors still decide which adapters are
present. Enabling plugin metadata alone does not choose SES, Mailpit, SQS, a
worker, a webhook client, an event destination, or a database table.

## Publish facts, not commands in disguise

A domain event records something that happened using a stable application-owned
schema and identifier. The originating use case writes state and its outbox
entry atomically when guaranteed intent durability is required.

```text
use case transaction -> domain state + outbox record
explicit worker      -> claim -> deliver -> mark result
```

Request-assisted dispatch can reduce latency, but the durable outbox remains
the recovery authority. There is no hidden polling loop or scheduler.

## Choose notifications or rich mail

Use the existing `Notification` API for generic channel-shaped notices. Use
`MailMessage` when email semantics matter:

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

The boundary validates the Minco message UUID, stable topic, recipient count,
mailboxes, duplicate addresses, subject, body alternatives, attachments, inline
content IDs, custom headers, tags, metadata, and rendered message size before
transport I/O. BCC recipients remain in the transport envelope and are never
written into MIME headers.

Mailbox duplicate detection preserves the local part and normalizes only the
domain. This version does not claim internationalized local-part support.

Attachment bytes are runtime values and deliberately are not serializable.
Durable intent should store object references, digests, lengths, and media types
in an application-owned schema rather than placing raw attachment bytes into an
outbox or queue record.

## Send and interpret the receipt correctly

```rust
use minco_plugin_notifications::{MailService, TracingMailObserver};
use std::sync::Arc;

let mail = MailService::single(transport, Arc::new(TracingMailObserver))?;
let receipt = mail.send(message).await?;
```

A `MailReceipt` proves that the selected transport accepted the submission and
returned a valid provider identifier. It is not proof of mailbox delivery.

Submission observation emits prepared, attempting, failed, and accepted states.
The tracing observer includes only the Minco message UUID, stable topic,
transport, attempt, coarse failure class, and duration. It excludes addresses,
display names, subject, bodies, attachment values, metadata values, and provider
message IDs. Observer execution is time-bounded so a slow observer cannot hold
the send path indefinitely.

## Treat retries as a correctness decision

Mail submission is not a normally idempotent network call. A provider may accept
a message while the caller loses the response. Retrying or moving to a fallback
can then send a duplicate.

`MailErrorKind` therefore separates:

- invalid message, configuration, authentication, rejection, and protocol
  violations: do not retry automatically;
- explicit throttling or unavailability before acceptance: retry after backoff
  or use a configured fallback;
- ambiguous outcome: reconcile the stable Minco message UUID and provider events
  before deciding whether to submit again.

`MailService` never retries internally. It advances through an explicitly
configured transport list only after a retry-safe error and stops immediately
on ambiguity.

Use Minco idempotency when an application command may be submitted repeatedly.
Use an explicit transactional outbox when business state and mail intent must be
committed together. Neither mechanism creates an exactly-once email-delivery
guarantee at the provider boundary.

## Send through Amazon SES v2

```rust
use minco_aws_adapters::ses::{SesMailTransport, SesMailTransportConfig};
use minco_plugin_notifications::MailAddress;
use std::sync::Arc;

let mut config =
    SesMailTransportConfig::new(MailAddress::new("no-reply@example.com")?)?;
config.configuration_set = Some("application-mail".into());
config.default_tags.insert("environment".into(), "production".into());

let transport = Arc::new(SesMailTransport::from_sdk_config(&aws_config, config)?);
```

The recommended constructor derives the SES client from the standard AWS SDK
configuration with one total send attempt and bounded operation and attempt
timeouts. It uses the same raw MIME renderer as local SMTP, fixes the configured
sender, and supports To/CC/BCC destinations, reply-to, alternatives,
attachments, safe headers, optional configuration set, endpoint ID, tenant
name, and sending-identity ARN.

Minco adds reserved `minco_message_id` and `minco_topic` SES tags. Application
tags cannot replace them. Provider errors are converted to bounded failure
classes instead of exposing raw provider diagnostics.

Direct SES is the default minimal-cost production shape. Enabling mail does not
create a queue, worker, DLQ, schedule, database, NAT Gateway, provisioned
concurrency, dedicated IP, or provider event destination.

## Observe final delivery separately

SES can publish delivery, bounce, complaint, reject, delay, rendering-failure,
and optional engagement events. Those events are separate from submission
acceptance.

```rust
use minco_aws_adapters::ses::parse_ses_event;

let event = parse_ses_event(request_body)?;
let disposition = delivery_sink.record(event).await?;
```

The parser accepts direct SES JSON, SNS-wrapped messages, and EventBridge detail
envelopes. It requires the reserved Minco correlation tags, discards recipient
and raw provider payload data, and derives a deterministic source event ID when
the envelope does not provide one.

`MemoryMailDeliveryEventSink` gives deterministic deduplication in tests.
`TracingMailDeliveryEventSink` records source event ID, Minco message UUID,
stable topic, transport, and event kind without provider message IDs, addresses,
subject/body content, URLs, IP addresses, user agents, or attachment values.

Open and click tracking should remain disabled unless the product has an
explicit privacy need and user-facing policy.

## Preview mail locally on macOS

Start the pinned loopback-only Mailpit inbox from the repository root:

```bash
docker compose -f compose.mail.yml up -d --wait
```

Use `MailpitTransport::default()` for SMTP at `127.0.0.1:1025`, then open
`http://127.0.0.1:8025`. The service keeps at most 500 messages for seven days,
limits messages to 40 MB and 50 recipients, blocks remote CSS/fonts, disables
reverse DNS and update checks, and configures no relay, forwarding, webhook,
POP3, Prometheus, or chaos feature.

The Rust adapter refuses non-loopback plaintext SMTP, bounds connection and
command time, parses multiline responses, dot-stuffs DATA, and treats connection
loss after DATA as ambiguous.

Stop the service while retaining the local inbox:

```bash
docker compose -f compose.mail.yml down
```

Add `-v` only when the captured inbox should also be deleted.

## Test without a container

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
```

The memory transport captures the complete message without network, AWS
credentials, sleeps, or provider state.

## Add queues and workers only when justified

Use the [queues and workers guide](./background-work) when request latency,
burst absorption, delayed delivery, or durable recovery justifies SQS and a
Lambda worker. Plan IR must expose the queue, mapping, retries, DLQ, IAM,
connection budget, cost class, and `queue_message` wake source.

Operational delivery events explain transport outcomes. Audit history explains
the business action that requested delivery. Neither substitutes for the other.
