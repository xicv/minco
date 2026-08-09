from pathlib import Path


def replace_exact(path: str, old: str, new: str, expected: int = 1) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != expected:
        raise SystemExit(f"{path}: expected {expected} matches, found {count}: {old!r}")
    file.write_text(text.replace(old, new), encoding="utf-8")


cargo = "extensions/minco-aws-adapters/Cargo.toml"
replace_exact(
    cargo,
    'ses = ["dep:aws-sdk-sesv2", "dep:minco-plugin-notifications"]',
    'ses = ["dep:aws-config", "dep:aws-sdk-sesv2", "dep:minco-plugin-notifications"]',
)
replace_exact(
    cargo,
    "[dependencies]\nasync-trait.workspace = true\n",
    "[dependencies]\nasync-trait.workspace = true\naws-config = { workspace = true, optional = true }\n",
)
replace_exact(
    cargo,
    "[dev-dependencies]\naws-config.workspace = true\n",
    "[dev-dependencies]\n",
)

readme = "extensions/minco-aws-adapters/README.md"
replace_exact(
    readme,
    "Production adapters for Minco's provider-neutral object-storage, event,\nnotification, identity-administration, and static-site ports.",
    "Production adapters for Minco's provider-neutral object-storage, event,\nnotification, rich-mail, identity-administration, and static-site ports.",
)
ses_section = '''## Amazon SES v2 mail

The `ses` feature retains the existing `SesNotificationSink` for compatible
single-recipient plain-text notification consumers and adds the explicit
`SesMailTransport` for the new `mail.send` contract.

```rust
use minco_aws_adapters::ses::{SesMailTransport, SesMailTransportConfig};
use minco_plugin_notifications::{MailAddress, MailService, TracingMailObserver};
use std::sync::Arc;

let mut config =
    SesMailTransportConfig::new(MailAddress::new("no-reply@example.com")?)?;
config.configuration_set = Some("application-mail".into());
config.default_tags.insert("environment".into(), "production".into());

let transport = Arc::new(SesMailTransport::from_sdk_config(&aws_config, config)?);
let mail = MailService::single(transport, Arc::new(TracingMailObserver))?;
```

The recommended constructor derives a service client from the normal AWS SDK
configuration with one total send attempt and bounded operation/attempt
timeouts. It uses raw MIME, a fixed configured sender, To/CC/BCC destinations,
reply-to headers, alternatives and attachments, safe custom headers, an optional
configuration set, endpoint ID, tenant name, and sending-identity ARN.

Minco reserves `minco_message_id` and `minco_topic` SES tags for correlation.
Provider acceptance returns a `MailReceipt`; it is not final mailbox-delivery
evidence. Timeouts and unknown dispatch outcomes are classified as ambiguous
and never trigger automatic retry or provider failover.

`parse_ses_event` normalizes direct, SNS-wrapped, and EventBridge-wrapped SES
delivery events. It requires the reserved correlation tags, derives a stable
source event ID when necessary, and drops recipient and raw provider payload
data before returning `MailDeliveryEvent`.

Direct SES is the default low-cost shape: enabling mail does not create a queue,
worker, DLQ, schedule, database, NAT gateway, provisioned concurrency, dedicated
IP, or event destination. Applications add those components only when latency,
durability, deliverability, or final-delivery evidence justifies their cost and
operational surface.

'''
replace_exact(
    readme,
    "The `static-site` adapter consumes an exact `StaticSiteReleaseManifest`.",
    ses_section + "The `static-site` adapter consumes an exact `StaticSiteReleaseManifest`.",
)
replace_exact(
    readme,
    "provider flags so Plan resource/cost intent and least-privilege IAM are derived\nfrom AWS selections, not from generic capabilities that may be backed by\nmemory. Production migrations and network calls remain explicit operations;",
    "provider flags so Plan resource/cost intent and least-privilege IAM are derived\nfrom AWS selections, not from generic capabilities that may be backed by\nmemory. Use the additive `AwsSesMailPlugin` marker when the rich SES mail\ntransport is selected. Production migrations and network calls remain explicit operations;",
)

decisions = "docs/DECISIONS.md"
replace_exact(
    decisions,
    "| [ADR-0035](adrs/0035-verified-direct-object-uploads.md)",
    "| [ADR-0034](adrs/0034-outbound-mail-delivery.md) | Keep rich outbound mail provider-neutral, ambiguity-safe, privacy-bounded, and direct-SES by default. | Accepted |\n| [ADR-0035](adrs/0035-verified-direct-object-uploads.md)",
)

changelog = "CHANGELOG.md"
replace_exact(
    changelog,
    "## [Unreleased]\n\nNo changes yet.",
    '''## [Unreleased]

### Added

- Added an explicit `mail.send` contract with validated To/CC/BCC/reply-to,
  text and HTML alternatives, bounded attachments and inline content, safe
  headers and tags, acceptance receipts, deterministic capture, and
  privacy-safe submission and delivery observation.
- Added a loopback-only Mailpit SMTP transport and a bounded, pinned local inbox
  for macOS and other Docker-compatible development environments.
- Added an Amazon SES v2 rich-mail transport with one SDK submission attempt,
  bounded timeouts, fixed sender identity, raw MIME, Minco correlation tags,
  configuration-set support, and normalized direct/SNS/EventBridge delivery
  events.

### Compatibility

- Existing generic notification APIs, `NotificationsPlugin::new`,
  `NotificationsPlugin::memory`, `SesNotificationSink`, and
  `aws.ses.email-notifications` remain available. The new `mail.send` and
  `aws.ses.mail-delivery` capabilities are additive and opt-in.

### Safety and cost

- Ambiguous mail-submission outcomes never retry or fail over automatically,
  provider acceptance remains distinct from final mailbox delivery, and direct
  SES introduces no queue, worker, schedule, database, NAT gateway, provisioned
  concurrency, dedicated IP, or other fixed-capacity service.''',
)
