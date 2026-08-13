# minco-plugin-audit

Append-only audit events for security-relevant and business-relevant actions.
The port keeps operational logs separate from durable audit history.

The legacy `AuditEvent` and `AuditSink` remain available. The additive V2
contract adds bounded semantic records, idempotent batch append, cursor-based
resource history, explicit related-resource gathering, size/rotation/retention
policy and inspectable storage health.

Permanent history belongs in a physically separate ledger. PostgreSQL and
SQLite adapters use a bounded transactional source journal before delivering to
that ledger. A DynamoDB source may commit its domain mutation and a canonical
record in a separate audit table through one cross-table transaction. Neither a
relay nor an archive schedule is implicit.

`AuditSizePolicy::sqlite_100_mib` is an initial finite-disk segment policy, not a
universal storage limit. PostgreSQL partitions and DynamoDB hot/archive horizons
remain provider-specific. Audit queries use a stable record cursor, so sealed or
archived segments do not appear in application resource identities.

Production applications inject explicit PostgreSQL, SQLite, DynamoDB,
object-storage or other append-only adapters and retain authorization in their
application use cases.
