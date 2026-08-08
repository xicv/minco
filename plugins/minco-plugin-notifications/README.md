# minco-plugin-notifications

Provider-neutral notification delivery plus a first-class mail contract for
Minco applications.

The existing `Notification`, `NotificationSink`, and `NotificationService` APIs
remain available. New mail code should use `MailMessage` and `MailService` for
multiple recipients, text and HTML alternatives, reply-to, CC/BCC, attachments,
inline content, custom headers, delivery tags, provider receipts, failover, and
lifecycle observation.

## Send rich mail

```rust
use minco_plugin_notifications::{
    MailAddress, MailMessage, MailService, MailpitTransport, TracingMailObserver,
};
use std::sync::Arc;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let transport = Arc::new(MailpitTransport::default());
let mail = MailService::single(transport, Arc::new(TracingMailObserver))?;
let message = MailMessage::builder("account.welcome", "Welcome to Minco")
    .to(MailAddress::named("person@example.com", "Example Person")?)
    .reply_to(MailAddress::new("support@example.com")?)
    .text("Welcome to Minco.")
    .html("<p>Welcome to <strong>Minco</strong>.</p>")
    .tag("message_type", "transactional")
    .build()?;

let receipt = mail.send(message).await?;
assert_eq!(receipt.transport, "mailpit");
# Ok(())
# }
```

Application-owned topics and message UUIDs are the stable correlation keys.
Transport adapters add provider message IDs after acceptance. A caller that
needs durable exactly-once intent should combine the stable message ID with
Minco's idempotency and transactional-outbox plugins; email providers do not
supply an end-to-end idempotency guarantee.

## Local browser inbox

Start the pinned Mailpit container from the repository root:

```bash
docker compose -f compose.mail.yml up -d
```

Use SMTP at `127.0.0.1:1025` and open `http://127.0.0.1:8025` for the browser UI.
`MailpitTransport` refuses remote plaintext SMTP addresses, so this adapter
cannot accidentally become a production unauthenticated relay.

Stop and remove the container while retaining captured mail:

```bash
docker compose -f compose.mail.yml down
```

Add `-v` only when the local inbox should also be deleted.

## Deterministic tests

`MemoryMailTransport` captures complete `MailMessage` values without network
access. `MemoryMailObserver` captures the lifecycle sequence.

```rust
use minco_plugin_notifications::{
    MailAddress, MailMessage, MailService, MemoryMailObserver, MemoryMailTransport,
};
use std::sync::Arc;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let transport = Arc::new(MemoryMailTransport::default());
let observer = Arc::new(MemoryMailObserver::default());
let mail = MailService::single(transport.clone(), observer)?;

mail.send(
    MailMessage::builder("invoice.ready", "Invoice ready")
        .to(MailAddress::new("person@example.com")?)
        .text("Your invoice is ready.")
        .build()?,
)
.await?;

transport.assert_sent_count(1).await;
transport.assert_sent_to("person@example.com").await;
# Ok(())
# }
```

## Delivery semantics

`MailErrorKind` separates invalid configuration, authentication, rejection,
throttling, temporary unavailability, and ambiguous outcomes. `MailService`
uses a fallback transport only after an explicitly retry-safe throttled or
unavailable result. It never falls through after an ambiguous result because
the first provider may already have accepted the message.

Observers receive prepared, attempting, accepted, failed, delivered, bounce,
complaint, delay, rendering, and optional engagement events. The built-in
tracing observer emits message UUID, stable topic, transport, event kind,
attempt, and provider message ID. It intentionally excludes recipients,
subjects, bodies, attachment names, and metadata values.
