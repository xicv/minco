# minco-aws-adapters

Production adapters for Minco's provider-neutral object-storage, event, mail,
identity-administration, static-site, and realtime ports.

Adapters accept normal AWS SDK clients, so `AWS_ENDPOINT_URL` can route supported
S3 and SQS seams through Rustack without emulator-specific application code.
Runtime construction is explicit and makes no network call during plugin
composition.

Enable only the required service features:

```toml
minco-aws-adapters = { version = "1.1.0", features = ["ses"] }
```

The `full` feature enables S3, SQS, SES v2, Cognito user-pool administration,
signed webhooks, S3/CloudFront static-site publication, and AppSync Events
publication. Select only the features used by the application to minimise
compile time and deployed code size.

## SES mail

`SesMailTransport` supports Minco's rich mail envelope, including To/CC/BCC,
reply-to, text and HTML bodies, attachments, inline content IDs, safe custom
headers, message tags, configuration sets, multi-region endpoint IDs, and SES
tenants.

```rust
use minco_aws_adapters::ses::{SesMailTransport, SesMailTransportConfig};
use minco_plugin_notifications::{
    MailAddress, MailService, TracingMailObserver,
};
use std::sync::Arc;

# fn example(client: aws_sdk_sesv2::Client) -> Result<(), Box<dyn std::error::Error>> {
let mut config =
    SesMailTransportConfig::new(MailAddress::new("no-reply@example.com")?)?;
config.configuration_set = Some("application-mail".into());
config.default_tags.insert("environment".into(), "production".into());
let transport = Arc::new(SesMailTransport::new(client, config)?);
let _mail = MailService::single(transport, Arc::new(TracingMailObserver))?;
# Ok(())
# }
```

Each request includes the application message UUID and stable topic as reserved
SES message tags. A successful send returns the SES message ID. The adapter
disables transparent AWS SDK retries for `SendEmail`: a timeout, dispatch
failure, or malformed response can occur after provider acceptance, so those
outcomes are classified as ambiguous and must be reconciled before another
send.

Use `parse_ses_event` for direct SES event JSON, SNS notifications, or
EventBridge detail envelopes, then pass the provider-neutral result through the
same configured observer:

```rust
# use minco_aws_adapters::ses::parse_ses_event;
# use minco_plugin_notifications::MailService;
# async fn example(mail: &MailService, payload: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
let event = parse_ses_event(payload)?;
mail.observe_provider_event(event).await?;
# Ok(())
# }
```

The parser requires Minco correlation tags and deliberately omits recipient
addresses from the resulting lifecycle event.

## Cost posture

For ordinary transactional volume, call SES directly from the request or an
explicit application worker and use shared IP delivery. Do not provision a
queue, scheduler, dedicated IP, or paid deliverability feature merely because
mail is enabled. Add Minco's existing transactional outbox and SQS worker only
when delayed delivery, burst absorption, or durable recovery is a business
requirement. Use SES configuration-set events and structured logs before adding
per-message custom CloudWatch metrics; provider events preserve richer delivery
state with lower cardinality and less duplicated telemetry.

The `static-site` adapter consumes an exact `StaticSiteReleaseManifest`. It
uses a conditional `.minco/deployment-lock`, uploads each object with SHA-256,
rechecks S3 checksum/size/media/cache metadata before deleting stale keys, and
waits for the deterministic CloudFront invalidation. A missing or ambiguous
lock cleanup fails closed; the adapter never steals or silently expires a lock.

Register the crate's explicit `AwsAdaptersPlugin` marker with the selected
provider flags so Plan resource/cost intent and least-privilege IAM are derived
from AWS selections, not from generic capabilities that may be backed by
memory. Production migrations and network calls remain explicit operations;
plugin composition performs neither.

See `docs/deployment/aws-plugin-adapters.md` in the Minco repository for local
Rustack, bounded real-AWS, IAM, migration, and cleanup procedures.
