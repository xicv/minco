# minco-plugin-audit

Append-only audit events for security-relevant and business-relevant actions.
The port keeps operational logs separate from durable audit history. Production
applications inject a PostgreSQL, DynamoDB, object-storage, or other append-only
adapter.
