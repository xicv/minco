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
| DynamoDB on-demand | Read/write request units, storage, streams/backups/transfer | No provisioned compute | Excellent for key-value/event/idempotency workloads; no relational joins/constraints | Cost profile included; adapter roadmap item |
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
