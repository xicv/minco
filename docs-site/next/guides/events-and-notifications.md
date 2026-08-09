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
Unicode display names and subjects use encoded words, printable-ASCII custom
headers are folded, Minco/SES control headers are reserved, and every physical
header line is checked against the 998-byte hard limit. Tests also parse the
rich fixture with an independent RFC 5322/MIME parser.

Attachment bytes are runtime values and deliberately are not serializable.
Durable intent should store object references, digests, lengths, and media types
in an application-owned schema rather than placing raw attachment bytes into an
outbox or queue record.

Rendering is deliberately in-memory and can retain the raw attachment, Base64
output, and final MIME buffer together. Use an access-controlled object link for
large files and size the runtime from measured peak memory; 25 MiB is a hard raw
boundary, not a recommended message size.

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
message IDs. Observer execution is concurrent and independently time-bounded,
so a slow observer cannot suppress later observers or hold the send path
indefinitely. A malformed acceptance receipt produces an ambiguous failed-
attempt observation before the error is returned.

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
tags cannot replace them. Topics are reversibly encoded as unpadded URL-safe
Base64 because dotted Minco topics are not valid SES tag values. The merged set
is capped at 50 as a Minco operational limit; the current SES v2 service model
does not specify a list maximum. Provider errors are converted to bounded
failure classes instead of exposing raw provider diagnostics.

Rich SES capability selection is explicit on both sides. A notifications plugin
advertises `mail.send` only when built with a `MailService`, and
`AwsAdapterSelection { rich_mail: true, .. }` independently requires that
capability and provides `aws.ses.mail-delivery` plus SES resource/IAM intent.

Direct SES is the default minimal-cost production shape. Enabling mail does not
create a queue, worker, DLQ, schedule, database, NAT Gateway, provisioned
concurrency, dedicated IP, or provider event destination.

## Observe final delivery separately

SES can publish submission, delivery, bounce, complaint, reject, delay,
rendering-failure, and optional engagement events. Those events are separate
from submission acceptance.

```rust
use minco_aws_adapters::ses::{
    SesEventTrustPolicy, verify_and_normalize_ses_event,
};

let policy = SesEventTrustPolicy::sns(expected_topic_arn)?;
let event = verify_and_normalize_ses_event(request_body, &policy, &sns_verifier)?;
let disposition = delivery_sink.record(event).await?;
```

Wrapped events are normalized only after an exact policy match and the supplied
verifier succeeds. An SNS verifier must validate the AWS signature and
certificate URL; EventBridge callers must attest the selected rule/bus
invocation boundary and match source, account, Region, detail type, and optional
resource ARN. `normalize_trusted_ses_event` exists only for direct SES JSON from
an already authenticated internal transport and rejects wrappers.

Normalization requires the encoded correlation tags, uses the event-specific
timestamp without a wall-clock fallback, preserves unknown bounce types as
undetermined, discards recipient/raw provider payload data, and derives an
opaque deterministic source event ID.

`MemoryMailDeliveryEventSink` gives deterministic deduplication in tests.
`TracingMailDeliveryEventSink` deduplicates and records a source-ID digest,
Minco message UUID, stable topic, transport, and event kind without raw source
or provider message IDs, addresses, subject/body content, URLs, IP addresses,
user agents, or attachment values. The tracing window is capped at 4,096 source
IDs and is not persistent; durable replay protection requires an
application-owned event store. The memory sink remains a deterministic
test-scoped fixture.

Open and click tracking should remain disabled unless the product has an
explicit privacy need and user-facing policy.

## Preview mail locally on macOS

Start the pinned loopback-only Mailpit inbox from the repository root:

```bash
docker compose -f compose.mail.yml up -d --wait
plugins/minco-plugin-notifications/scripts/mailpit-ready.sh
plugins/minco-plugin-notifications/scripts/mailpit-smoke.sh
```

Use `MailpitTransport::default()` for SMTP at `127.0.0.1:1025`, then open
`http://127.0.0.1:8025`. The service keeps at most 500 messages for seven days,
limits messages to 40 MB and 50 recipients, and bounds container CPU, memory,
and PIDs. The Mailpit UI's remote-CSS/font control, reverse-DNS disablement, and
update-check disablement are enabled; the UI control is not a general browser or
host-network isolation boundary and does not block remote images or tracking
pixels. Do not open untrusted HTML without separate browser/network isolation.
The service configures no relay, forwarding, webhook, POP3, Prometheus, chaos
feature, or automatic restart policy.

Compose uses Mailpit's native health command, while `mailpit-ready.sh` proves
host reachability through `/readyz`. The smoke sends rich mail over SMTP and
checks Mailpit's API. Mailpit reconstructs BCC from SMTP envelope metadata in
its raw-message API, so the byte-exact SMTP unit test separately proves Minco's
transmitted MIME omitted BCC.

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
