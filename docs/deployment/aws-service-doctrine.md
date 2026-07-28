# AWS service and zero-provisioned-compute doctrine

Status: accepted product doctrine
Reviewed: 2026-07-28
Applies to: Minco `0.4.0` source and later planning

Minco targets zero provisioned application compute at idle. Storage, retained
logs, DNS, secrets, database storage, schedules and other fixed/request
dimensions remain explicit and bounded. “Zero idle” never means “zero bill.”
Use the cost classes and pricing-confidence labels defined in
[ADR 0025](../adrs/0025-zero-provisioned-compute-review-loop.md).

## Preferred defaults

| Capability | Preferred service/profile | Idle and cost posture |
|---|---|---|
| HTTP ingress | API Gateway HTTP API | Request-priced; no Minco-owned fixed compute |
| HTTP/worker compute | ARM64 Lambda ZIP | `zero_compute`; invocations and duration remain request-priced |
| Object/artifact storage | S3 private buckets | `storage_only`, plus request and transfer dimensions |
| Static delivery | CloudFront with OAC | Request/transfer or eligible flat-rate plan; distribution and logs remain explicit |
| Non-secret configuration | SSM Parameter Store standard parameters | No additional Parameter Store charge for standard parameters under current published pricing; API and related services can still cost |
| Queues | SQS standard or FIFO selected explicitly | Request-priced with retention, retry and DLQ policy visible |
| Logs and metrics | Bounded CloudWatch | Log retention is explicit because log groups otherwise retain data indefinitely |
| Infrastructure | CloudFormation change sets | Preview and apply stay separate; provider completion is not runtime proof |

Source: current AWS primary documentation for
[API Gateway pricing](https://aws.amazon.com/api-gateway/pricing/),
[Lambda](https://aws.amazon.com/lambda/lambda-functions/),
[S3 pricing](https://aws.amazon.com/s3/pricing/),
[CloudFront pricing](https://aws.amazon.com/cloudfront/pricing/),
[SSM pricing](https://aws.amazon.com/systems-manager/pricing/),
[SQS pricing](https://aws.amazon.com/sqs/pricing/), and
[CloudWatch Logs retention](https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/WhatIsCloudWatchLogs.html),
reviewed 2026-07-28. Prices, allowances and availability must be refreshed for
the exact target account and Region before an operator accepts a cost claim.

## Explicit opt-in profiles

These services are valid only when the application selects and justifies their
correctness, wake-source, connection, retention and cost behavior:

- DynamoDB on-demand for access-pattern-specific ports. Current on-demand mode
  charges for requests and storage rather than provisioned throughput, but
  table/storage/backup/transfer dimensions and configurable throughput limits
  remain visible.
- Aurora DSQL as experimental research only. It is not a relational SQLx
  substitute: transaction, isolation, DDL/DML and quota limits must be proven
  against the application access patterns and selected Region.
- Aurora Serverless v2 PostgreSQL with a deliberately configured zero-ACU
  auto-pause range, where supported. Resume latency, connections, storage,
  I/O, backup and minimum-capacity behavior remain explicit.
- Neon PostgreSQL under its own provider contract and operational gate.
- RDS Data API for specialist Aurora profiles. HTTP access and no persistent
  client connection can simplify Lambda networking, but payload, response,
  timeout and transaction limitations remain application constraints.
- Cognito, SES, Transcribe and Bedrock only for explicitly selected product
  capabilities, quotas, data policy and regional availability.
- EventBridge Scheduler only for a declared `scheduled_wakeup`. One-time
  schedules set `ActionAfterCompletion=DELETE`; deletion is lifecycle hygiene,
  not proof that the target invocation or retained resources disappeared.

Primary references reviewed 2026-07-28:
[DynamoDB on-demand](https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/on-demand-capacity-mode.html),
[Aurora DSQL pricing](https://aws.amazon.com/rds/aurora/dsql/pricing/),
[Aurora DSQL quotas](https://docs.aws.amazon.com/aurora-dsql/latest/userguide/CHAP_quotas.html),
[Aurora DSQL access and transactions](https://docs.aws.amazon.com/aurora-dsql/latest/userguide/accessing.html),
[Aurora Serverless v2 auto-pause](https://docs.aws.amazon.com/AmazonRDS/latest/AuroraUserGuide/aurora-serverless-v2-auto-pause.html),
[RDS Data API](https://docs.aws.amazon.com/AmazonRDS/latest/AuroraUserGuide/data-api.html),
[RDS Data API limitations](https://docs.aws.amazon.com/AmazonRDS/latest/AuroraUserGuide/data-api.troubleshooting.html),
and
[EventBridge Scheduler cleanup](https://docs.aws.amazon.com/scheduler/latest/UserGuide/managing-schedule-delete.html).

## Not default

The minimal profile does not add a NAT Gateway, ALB, EC2, ECS, provisioned RDS,
ElastiCache, OpenSearch, recurring schedule, preview custom domain, indefinite
log retention or provisioned concurrency. Any future profile that selects one
of these must expose its fixed/request dimensions, retention, wake sources and
removal behavior and cannot call itself zero-provisioned-compute by default.

CloudFront flat-rate plans are eligibility-dependent, account-level commercial
choices. The published allowances are not hard limits and AWS documents
eligibility constraints, so Minco does not select a plan automatically or
encode a timeless price. See the current
[CloudFront flat-rate documentation](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/flat-rate-pricing-plan.html).

## Operator evidence

Before AWS mutation, record:

1. exact account, Region, role and environment;
2. exact release manifest, source and artifact digests;
3. every cost class and any unpriced or eligibility-dependent dimension;
4. explicit schedules and their cleanup policy;
5. database correctness, wake, connection and cost assumptions;
6. log and object retention;
7. change-set, migration and destructive-action approvals.

Static Plan/SAM evidence does not prove current pricing, account eligibility,
service availability, deployment success, runtime behavior or cleanup. Those
remain separate live gates.
