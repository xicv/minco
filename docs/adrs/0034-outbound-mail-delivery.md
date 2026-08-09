# ADR 0034: Provider-neutral outbound mail with explicit delivery evidence

## Status

Accepted

## Context

Minco already exposes a generic notification port and a narrow Amazon SES
adapter. That surface is sufficient for single-recipient plain-text notices,
but everyday web applications also need CC/BCC, reply-to, text and HTML
alternatives, inline content, bounded attachments, deterministic tests, local
visual inspection, provider acceptance receipts, and later delivery, bounce,
complaint, delay, reject, rendering, and optional engagement evidence.

These concerns cannot be folded into the generic notification type without
silently discarding email semantics. They also cannot justify an always-running
mail server, worker, database, metric publisher, or scheduler: Minco's default
must retain zero provisioned application compute at idle and make every queue,
outbox worker, event destination, schedule, or dedicated deliverability service
an explicit product decision.

Mail submission is not safely repeatable. A provider may accept a message and
the client may lose the response. Retrying or failing over that ambiguous
outcome can deliver duplicates. Provider acceptance is also not mailbox
delivery; final delivery state arrives independently through provider events.

## Decision

Minco adds an additive `mail.send` capability beside the existing
`notifications.send` capability.

### Message and transport boundary

`MailMessage` owns an application-generated UUID, stable bounded topic,
recipient envelope, subject, text/HTML bodies, bounded runtime attachments,
safe custom headers, provider tags, application metadata, and creation time.
The runtime attachment bytes deliberately do not implement Serde, preventing a
large byte vector from accidentally becoming an outbox or queue persistence
format. Durable mail intent should store object references, digests, lengths,
and media types in an application-owned schema. Rendering is intentionally
in-memory and can transiently retain the raw attachment, Base64 output, and
final MIME buffer; large-file workflows should send an access-controlled object
link instead of approaching the 25 MiB raw boundary.

`MailTransport` returns a `MailReceipt` only when a provider has accepted a
submission and returned a valid provider message identifier. `MailService`
observes prepared, attempting, failed, and accepted states. A malformed
acceptance receipt emits an ambiguous failed-attempt observation before it is
returned. The service advances to a fallback only for an explicit throttled or
unavailable outcome. Timeouts, connection loss after SMTP DATA, malformed
acceptance receipts, and unknown SDK outcomes are ambiguous and stop the chain.

The existing notification API and `SesNotificationSink` remain compatible.
`LegacyNotificationMailTransport` is opt-in and rejects every rich feature it
cannot preserve instead of degrading the message silently. A notifications
plugin advertises `mail.send` only when a mail service is explicitly installed.

### Rendering and limits

The provider-neutral MIME renderer uses CRLF, Base64 transfer encoding,
`multipart/alternative`, `multipart/related`, and `multipart/mixed` as needed.
BCC recipients remain in the transport envelope and never appear in MIME
headers. Custom headers cannot replace envelope, MIME, routing, or signature
headers, including Minco and SES control headers. Address display names and
subjects use bounded UTF-8 encoded words; custom application header values are
printable ASCII. Every physical header line is checked against the 998-byte RFC
hard limit. Recipient count, body size, attachment count and raw bytes, custom
headers, tags, metadata, content IDs, and the final rendered message size are
bounded before transport I/O. An independent RFC 5322/MIME parser exercises the
rendered rich-message fixture in tests.

Mailbox duplicate detection preserves the local part and case-normalizes only
the domain. Internationalized local parts are not claimed in this version.

### Amazon SES v2

The SES transport uses raw MIME so one renderer defines local and provider
behavior. It fixes the configured sender identity, adds reserved
`minco_message_id` and `minco_topic` tags, and optionally applies a configuration
set, endpoint ID, tenant name, and sending-identity ARN. Minco topics allow dots,
while SES tags do not, so the topic is reversibly encoded with unpadded URL-safe
Base64. The final merged tag set is capped at 50; this is a Minco operational
limit because the current SES v2 service model constrains each tag component but
does not declare a list maximum.

The public constructor derives an SES client from the normal AWS SDK
configuration with one total send attempt plus bounded operation and attempt
timeouts. The arbitrary-client executor seam is private and test-only, so public
construction cannot bypass the one-attempt policy. Minco does not hide SDK
retries at the mail-submission boundary.
Service responses proving throttling or pre-acceptance rejection are classified
separately from ambiguous transport and server failures.

### Delivery-event boundary

`MailDeliveryEvent` represents submission, delivery, permanent/transient/
undetermined bounce, complaint, reject, delay, rendering failure, and optional
open/click/subscription events. Occurrence time comes from the event-specific
object; submission, reject, and rendering failure use the SES mail timestamp
because those official shapes do not provide a later event timestamp. Missing
or invalid required timestamps fail closed rather than falling back to wall
clock time.

Direct SES JSON can be normalized only through an API explicitly named as
already trusted; it rejects SNS and EventBridge wrappers. Wrapped input must use
an exact `SesEventTrustPolicy` plus a caller-supplied
`SesEventEnvelopeVerifier`. SNS policy checks the exact topic ARN before the
verifier authenticates the AWS signature and certificate URL. EventBridge
policy checks source, account, Region, allowed detail type, and optional
resource ARN; the verifier must attest the selected rule/bus invocation
boundary. Minco deliberately does not implement ad-hoc SNS cryptography or
pretend an unsigned EventBridge JSON body authenticates itself.

Normalization requires Minco correlation tags, decodes the topic, drops
recipient and raw provider payload data, and derives a deterministic opaque
source event ID from the authenticated envelope ID or canonical direct event.
Unknown bounce types remain undetermined instead of being guessed transient.

Delivery-event sinks own deduplication. The memory sink is deterministic and
test-scoped. The tracing sink caps its in-process source-ID window at 4,096
entries. Neither provides replay safety across restarts; that requires a durable
application-owned event store. The tracing sink records a digest of the source
ID, Minco message UUID, stable topic, transport, and event kind; it excludes raw
source IDs, recipients, subjects, bodies, URLs, IP addresses, user agents,
metadata values, attachment names, and provider message IDs.

### Local development

A pinned Mailpit Compose service is development-only. Host ports are bound to
loopback, the inbox is bounded by count and age, container CPU/memory/PIDs,
message size, and recipient count. Reverse DNS and update checks are disabled,
and the Mailpit preview remote-CSS/font control is enabled; that control is not
a general browser or host-network isolation boundary and does not block remote
images such as tracking pixels. Untrusted HTML should not be opened without a
separately isolated browser/network policy. No relay, forwarding,
webhook, POP3, Prometheus, or chaos feature is configured. The image-native
`/mailpit readyz` health command and a bounded host-side `/readyz` probe provide
separate container and host reachability evidence. The service has no automatic
restart policy.

`MailpitTransport` accepts loopback endpoints only, implements bounded SMTP
commands and multiline responses, dot-stuffs DATA, and treats connection loss
after DATA as ambiguous.

## Consequences

- Rich mail is available without changing existing notification consumers.
- Direct SES remains the smallest production architecture and adds no idle
  application compute.
- Applications can combine stable message UUIDs with Minco idempotency and an
  explicit transactional outbox when business durability requires it.
- Queues, workers, DLQs, schedules, provider event destinations, open/click
  tracking, dedicated IPs, and deliverability platforms remain explicit cost,
  privacy, and operational choices.
- Runtime attachment bytes are convenient and bounded but are not a durable
  intent representation.
- Exactly-once email delivery is not claimed.
- Provider acceptance and final mailbox delivery remain separate evidence.
- Runtime capability truth is selection-dependent: the static notifications
  distribution describes its plain constructor, while an explicitly installed
  rich `MailService` adds `mail.send`; the AWS aggregate selection must
  independently opt into `rich_mail` before it requires and provides the SES
  rich-mail capabilities.

## Compatibility

The change is additive after Minco 1.1. Existing `Notification`,
`NotificationSink`, `NotificationService`, `NotificationsPlugin::new`,
`NotificationsPlugin::memory`, `SesNotificationSink`, and
`aws.ses.email-notifications` remain available. The new `mail.send` and
`aws.ses.mail-delivery` capabilities are opt-in.

## Safety and privacy

All address, header, content-ID, tag, provider-identifier, envelope, and event
inputs are bounded and reject control characters. Ordinary errors and telemetry
do not include addresses, subjects, bodies, attachments, credentials, raw SES
responses, URLs, IP addresses, or user agents. Local plaintext SMTP is restricted
to loopback. Production transport uses AWS credentials and the selected Region
through the standard SDK configuration.
