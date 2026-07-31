# Database Deployment Options and Cost Model

Minco separates **business persistence ports** from deployment/database profiles.
PostgreSQL and SQLite have official SQLx adapters. DynamoDB requires a
use-case-specific adapter because relational SQL ports must not be silently
translated into key-value semantics.

## Profiles

| Profile | Cost shape | Idle behavior | Operational trade-off | Minco status |
|---|---|---|---|---|
| Neon Free/Launch | Compute-unit hours, storage, history; provider can suspend compute | Near-zero compute while suspended, storage remains | External provider; direct migration and pooled runtime URLs | Recommended minimal-idle PostgreSQL profile |
| Self-hosted PostgreSQL | VM/instance, disk, snapshots, transfer, monitoring and operator time | Fixed capacity while host runs | Maximum control; patching, backup/restore, HA and incident ownership | Supported planning profile |
| RDS PostgreSQL | Instance/serverless capacity, storage, backup, I/O/transfer | Usually fixed or minimum capacity | AWS-managed patching/backups; VPC and connection planning | Supported planning profile |
| Aurora Serverless v2 | ACU-seconds, storage, I/O/transfer, backups | Can auto-pause only on supported configuration and no wake activity | AWS-native elasticity; careful pooling/monitoring required | Supported planning profile |
| Aurora DSQL | DPU usage, storage and optional multi-Region replication | DPU usage scales to zero; storage remains | PostgreSQL-compatible subset, optimistic conflicts and bounded transactions; IAM-token lifecycle | Research only; no production adapter |
| DynamoDB on-demand | Read/write request units, storage, streams/backups/transfer | No provisioned compute | Excellent for key-value/event/idempotency workloads; no relational joins/constraints | Cost profile included; adapter roadmap item |
| Aurora with RDS Data API | Aurora dimensions plus Data API calls/data, Secrets Manager and optional PrivateLink | Depends on the selected Aurora profile | HTTPS and IAM instead of an application-side pool; writer-only, payload and timeout limits | Specialist research profile |
| Persistent SQLite | Host/storage cost | Depends on the host | Excellent local/single-process store | Official local/native adapter; mutable Lambda deployment rejected |

## Cost-estimation policy

Minco's estimator is evidence-oriented:

1. Structural rules always run: fixed capacity, NAT Gateway, scheduled wakeups,
   provisioned concurrency, connection multiplication, and mutable SQLite on
   ephemeral Lambda storage.
2. Published provider rates may be embedded only with a source date and pricing
   region/tier.
3. AWS regional rates are supplied in the environment/configuration or imported
   from an approved pricing snapshot. Missing rates produce an **incomplete
   estimate**, not a guessed zero.
4. Estimates show each component and assumption separately.
5. Human operational cost for self-hosted databases is called out but not
   converted into a fictional universal hourly rate.
6. Provider free allowances and account eligibility are classified but never
   converted into a complete zero-dollar estimate.

## Selection guidance

Use Neon or another managed PostgreSQL service for a new relational MVP when
external hosting is acceptable and scale-to-zero matters. Use RDS/Aurora when
AWS-native governance, private networking, support, or compliance dominates.
Use self-hosted PostgreSQL only when an owner accepts patching, hardening,
monitoring, backup/restore and availability duties. Use DynamoDB for modules whose
access patterns and invariants are naturally key-value/event-oriented; do not
replace relational business truth solely to lower idle compute.

## Connection budget

For Lambda + PostgreSQL:

```text
maximum potential connections
  = reserved Lambda concurrency
  × max pool connections per execution environment
```

The plan fails when this exceeds the configured provider/database budget. The
default example uses one small pool per execution environment and no provisioned
concurrency.

See [zero-idle service research](zero-idle-service-research.md) for the dated
correctness, transaction, wake, connection, quota, Region and pricing evidence
behind these profile boundaries.
