# minco-aws-adapters

Production adapters for Minco's provider-neutral object-storage, event,
notification, identity-administration, and static-site ports.

Adapters accept normal AWS SDK clients, so `AWS_ENDPOINT_URL` can route supported
S3 and SQS seams through Rustack without emulator-specific application code.
Runtime construction is explicit and makes no network call during plugin
composition.

Enable only the required service features:

```toml
minco-aws-adapters = { version = "0.6.0", features = ["s3", "sqs"] }
```

The `full` feature enables S3, SQS, SES v2, Cognito user-pool administration,
signed webhooks, and S3/CloudFront static-site publication.

Register the crate's explicit `AwsAdaptersPlugin` marker with the selected
provider flags so Plan resource/cost intent and least-privilege IAM are derived
from AWS selections, not from generic capabilities that may be backed by
memory. Production migrations and network calls remain explicit operations;
plugin composition performs neither.

See `docs/deployment/aws-plugin-adapters.md` in the Minco repository for local
Rustack, bounded real-AWS, IAM, migration, and cleanup procedures.
