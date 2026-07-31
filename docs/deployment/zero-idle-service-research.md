# Zero-idle service research and decision record

Status: accepted research input
Evidence reviewed: 2026-07-31
Scope: database, CloudFront, schedule cleanup and cost-evidence planning

This document records provider facts as dated evidence, not timeless product
promises. It separates structural properties that Minco can test locally from
live properties that must be refreshed for the target account, Region, engine
version and price catalog.

## Decision

Plan IR schema 2 remains the deployment topology boundary. It is not expanded
into a general pricing engine and does not add an Aurora DSQL, RDS Data API or
CloudFront adapter based on research alone.

The smallest proven additions are:

1. cost output carries a typed `cost_class` and `pricing_confidence` for each
   represented dimension;
2. a one-time schedule can carry an optional cleanup contract containing
   `action_after_completion`, residual resources and a manual fallback;
3. the SAM/CloudFormation renderer fails closed because neither current
   deployment schema exposes `ActionAfterCompletion`; a future guarded
   Scheduler API apply must bind the exact plan to a receipt;
4. old schema 2 schedules without `cleanup` still deserialize and serialize
   without the new field;
5. provider allowances never turn an eligibility-dependent estimate into a
   complete zero-dollar claim.

The cost extension is proven against materially different profiles:

- DynamoDB separates request-priced reads/writes from retained storage, with
  missing AWS rates classified `region_dependent`;
- zero-ACU Aurora separates scale-to-zero compute from retained storage and
  request-priced I/O;
- provisioned RDS remains `fixed_monthly`;
- Neon Free is `free_tier_dependent`, while dated paid Neon rates are `priced`.

## Database comparison

| Profile | Correctness and transactions | Wake and connections | Current limits and eligibility | Cost dimensions | Decision |
|---|---|---|---|---|---|
| Aurora DSQL | ACID, strong consistency and fixed Repeatable Read-equivalent isolation on a PostgreSQL 16-compatible subset; optimistic conflicts occur at commit; DDL and DML must be in separate transactions | Standard PostgreSQL v3 wire protocol; IAM token is needed for each new connection; sessions last at most one hour | One DDL statement and at most 3,000 mutated rows per transaction; five-minute transaction limit; SQL, data type and Region support are evolving | DPU usage scales to zero at idle; storage remains; multi-Region storage is charged in each Region; any current free allowance is account-dependent | Research only. A dedicated adapter needs compatibility, conflict-retry, token-redaction and migration qualification |
| DynamoDB on-demand | ACID item transactions are bounded to 100 unique items and 4 MiB in one Region; it does not provide relational joins or SQL constraints | No relational connection pool; traffic drives request units; access patterns and item sizes are part of correctness | New/on-demand peak behavior and account/table quotas can throttle sudden growth; explicit maximum throughput can bound spend by throttling | No throughput charge at zero traffic; read/write request units, storage, indexes, streams, backups, global tables and transfer remain | Use only through access-pattern-specific ports/adapters |
| Aurora Serverless v2 | Full selected Aurora PostgreSQL engine behavior; normal PostgreSQL transactions | Zero-ACU auto-pause requires a supported engine/Region and no activity that prevents pause; resume is typically about 15 seconds and can exceed 30 seconds after deep sleep; connections and retries must tolerate this | PostgreSQL minimum versions currently include 16.3, 15.7, 14.12 and 13.15; RDS Proxy, logical replication, global databases, maintenance and other activity can keep instances awake | Instance charge is zero only while actually paused; storage, I/O, backup, transfer, logs and optional features remain | Supported opt-in profile; require a live auto-pause observation before a zero-compute claim |
| Neon | PostgreSQL service with normal relational semantics, subject to Neon's compatibility contract | Compute suspends after inactivity; connection attempts/background work can keep it awake; clients must reconnect after suspension | Plan allowances and eligibility are provider policy; pooled connection endpoints are preferred for serverless clients | Active compute, retained storage and history; Free allowance is never treated as a complete estimate | Recommended external minimal-idle PostgreSQL option when provider governance is acceptable |
| Aurora plus RDS Data API | Aurora remains the database of record; Data API exposes statement, batch and explicit begin/commit/rollback operations | HTTPS calls avoid a persistent application-side database pool and can avoid Lambda VPC configuration; IAM and a Secrets Manager secret are required | Writer-only queries; 1 MiB response limit; default statement timeout 45 seconds; transaction expires after three minutes without another call; availability depends on engine, version and Region | Underlying Aurora dimensions plus Data API requests/data, Secrets Manager and optional PrivateLink | Specialist transport profile, not a generic replacement for SQLx ports |

Primary evidence:

- [Aurora DSQL overview and Region availability](https://docs.aws.amazon.com/aurora-dsql/latest/userguide/what-is-aurora-dsql.html)
- [Aurora DSQL access and session behavior](https://docs.aws.amazon.com/aurora-dsql/latest/userguide/accessing.html)
- [Aurora DSQL quotas](https://docs.aws.amazon.com/aurora-dsql/latest/userguide/CHAP_quotas.html)
- [Aurora DSQL SQL compatibility](https://docs.aws.amazon.com/aurora-dsql/latest/userguide/working-with-postgresql-compatibility-supported-sql-features.html)
- [Aurora DSQL DDL and transaction behavior](https://docs.aws.amazon.com/aurora-dsql/latest/userguide/working-with-ddl.html)
- [Aurora DSQL pricing](https://aws.amazon.com/rds/aurora/dsql/pricing/)
- [DynamoDB on-demand capacity](https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/on-demand-capacity-mode.html)
- [DynamoDB transaction constraints](https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/Constraints.html)
- [Aurora Serverless v2 auto-pause](https://docs.aws.amazon.com/AmazonRDS/latest/AuroraUserGuide/aurora-serverless-v2-auto-pause.html)
- [RDS Data API operations](https://docs.aws.amazon.com/AmazonRDS/latest/AuroraUserGuide/data-api-operations.html)
- [RDS Data API limitations](https://docs.aws.amazon.com/AmazonRDS/latest/AuroraUserGuide/data-api.limitations.html)
- [RDS Data API timeout behavior](https://docs.aws.amazon.com/AmazonRDS/latest/AuroraUserGuide/data-api-timeouts.html)
- [Neon pricing](https://neon.com/pricing)
- [Neon scale to zero](https://neon.com/docs/introduction/scale-to-zero)

### Region conclusion

Aurora DSQL currently lists both Sydney (`ap-southeast-2`) and Melbourne
(`ap-southeast-4`) for single-Region clusters. Neither is in the currently
published Asia Pacific multi-Region set, which lists Osaka, Seoul and Tokyo.
Minco therefore cannot infer an Australian multi-Region topology from
single-Region availability.

## Bounded Aurora DSQL and SQLx experiment

The experiment used the repository's locked SQLx `0.9.0`, Rust `1.97.1`, no
credentials in source and no network connection. A read-only AWS check found
no DSQL cluster in `ap-southeast-2`; no cluster or other AWS resource was
created.

The compiled probe demonstrated that current SQLx can express:

- a PostgreSQL endpoint on port 5432 with `VerifyFull` TLS;
- the `admin` role and `postgres` database;
- a runtime password suitable for an IAM token;
- a lazy pool with zero minimum connections, one maximum connection and a
  3,300-second maximum lifetime, below DSQL's one-hour session maximum;
- separate SQLx transactions for one DDL statement and later DML/rollback.

It also found a blocker: `Debug` output for SQLx `PgConnectOptions` contains the
configured password. A future DSQL connector must wrap connection settings in
a redacted type and prove that IAM tokens cannot enter logs, diagnostics,
receipts or serialized plans. It must generate fresh tokens for new
connections and retry optimistic commit conflicts within an application-owned
idempotency policy.

The experiment did **not** establish a live session, execute SQL, measure
latency, validate every migration, or prove production compatibility. A future
adapter qualification needs an explicitly approved disposable cluster and
must test:

1. one supported DDL statement committed separately;
2. insert/update/delete and rollback;
3. rejection of mixed DDL and DML;
4. the 3,000-row mutation boundary;
5. transaction/session expiry;
6. optimistic conflict retry and idempotency;
7. the actual application's SQLx migrations and queries;
8. teardown plus a no-residual-resource receipt.

## CloudFront commercial profiles

CloudFront pay-as-you-go is `request_only` plus transfer and optional feature
dimensions. It has no Minco-owned provisioned application compute, but it is
not a zero-dollar service claim.

Flat-rate plans are `fixed_monthly` and `eligibility_dependent`. As published
on 2026-07-31, one plan covers one distribution and can bundle CloudFront,
WAF, Route 53, CloudWatch Logs ingestion, TLS, edge compute and S3 storage
credits. Published request/data allowances are:

| Tier | Requests/month | Data transfer/month |
|---|---:|---:|
| Free | 1 million | 100 GB |
| Pro | 10 million | 50 TB |
| Business | 125 million | 50 TB |
| Premium | 500 million | 50 TB |

These are allowances, not hard limits. AWS documents historical-usage
eligibility and possible delivery adjustment after sustained excess. Other
features, unattached resources and unsupported configurations can still incur
separate charges. Minco therefore records the selected commercial profile but
does not auto-select a tier or embed its current price in Plan IR.

Source: [CloudFront flat-rate pricing plans](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/flat-rate-pricing-plan.html).

## One-time schedule lifecycle

An opted-in one-time `at(...)` schedule can now record:

```toml
[triggers.cleanup]
action_after_completion = "delete"
residual_resources = [
  "target outputs",
  "CloudWatch logs",
  "database records",
]
manual_fallback = "inspect the exact review receipt and run guarded cleanup"
```

The plan and cost evidence record `ActionAfterCompletion: DELETE`. The current
SAM `ScheduleV2` event and `AWS::Scheduler::Schedule` CloudFormation resource
do not expose that Scheduler service API property. The renderer therefore
fails closed instead of emitting an invalid or incomplete template. A future
guarded Scheduler API operation must bind the exact schedule, target, role,
cleanup contract and provider response to a durable receipt.

Completion deletion removes the schedule, not its target Lambda, target
output, log group, database records or review environment. Empty
residual-resource or fallback declarations fail locally, and completion
deletion is rejected for recurring `rate(...)` or `cron(...)` schedules.
Provider-side completion and deletion still need live observation; any
residual cleanup remains a separately planned and explicitly approved
operation.

Source: [EventBridge Scheduler completion cleanup](https://docs.aws.amazon.com/scheduler/latest/UserGuide/managing-schedule-delete.html).
Renderer boundary:
[SAM `ScheduleV2` properties](https://docs.aws.amazon.com/serverless-application-model/latest/developerguide/sam-property-function-schedulev2.html),
[CloudFormation `AWS::Scheduler::Schedule`](https://docs.aws.amazon.com/AWSCloudFormation/latest/TemplateReference/aws-resource-scheduler-schedule.html),
and the
[Scheduler `CreateSchedule` API](https://docs.aws.amazon.com/scheduler/latest/APIReference/API_CreateSchedule.html).

## Enforcement boundary

Local, deterministic checks can enforce:

- no NAT Gateway, fixed compute, undeclared schedule or provisioned
  concurrency in the minimal profile;
- cost class, missing-rate and pricing-confidence classification;
- zero ACU paired with an auto-pause interval;
- connection-budget arithmetic;
- one-time schedule cleanup shape and deterministic renderer rejection until
  guarded apply exists;
- eligibility-dependent allowances never becoming complete estimates.

They cannot prove current price, account eligibility, service availability,
engine compatibility, actual pause, cold-start latency, successful cleanup or
the final bill. Those are live release observations tied to the exact account,
Region, artifact, provider response and time.
