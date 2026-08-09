# minco-aws-adapters

Production adapters for Minco's provider-neutral object-storage, event,
notification, rich-mail, identity-administration, and static-site ports.

Adapters accept normal AWS SDK clients, so `AWS_ENDPOINT_URL` can route supported
S3 and SQS seams through Rustack without emulator-specific application code.
Runtime construction is explicit and makes no network call during plugin
composition.

Enable only the required service features:

```toml
minco-aws-adapters = { version = "0.6.0", features = ["s3", "sqs"] }
```

The `full` feature enables S3, SQS, SES v2, Cognito user-pool administration,
signed webhooks, S3/CloudFront static-site publication, and AppSync Events
publication. Select only `appsync-events` for the minimal realtime adapter. It
accepts the exact regional AppSync `/event` endpoint, obtains credentials from
the normal AWS credentials provider, signs for the `appsync` service, publishes
one bounded envelope per call, and never includes provider response bodies or
credential material in its public errors.

## Amazon SES v2 mail

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

The `static-site` adapter consumes an exact `StaticSiteReleaseManifest`. It
uses a conditional `.minco/deployment-lock`, uploads each object with SHA-256,
rechecks S3 checksum/size/media/cache metadata before deleting stale keys, and
waits for the deterministic CloudFront invalidation. A missing or ambiguous
lock cleanup fails closed; the adapter never steals or silently expires a lock.

Register the crate's explicit `AwsAdaptersPlugin` marker with the selected
provider flags so Plan resource/cost intent and least-privilege IAM are derived
from AWS selections, not from generic capabilities that may be backed by
memory. Use the additive `AwsSesMailPlugin` marker when the rich SES mail
transport is selected. Production migrations and network calls remain explicit
operations; plugin composition performs neither.

See `docs/deployment/aws-plugin-adapters.md` in the Minco repository for local
Rustack, bounded real-AWS, IAM, migration, and cleanup procedures.
