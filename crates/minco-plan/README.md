# minco-plan

Provider-neutral deployment planning for Minco.

The Plan IR connects OpenAPI operations to runtime, ingress, authentication,
the configured static application graph, database, performance, and cost-policy
decisions. Schema 2 explicitly models one HTTP API function, worker functions,
queues, dead-letter policy, SQS mappings and reviewed schedule triggers.
Schema 1 remains supported for the original API-only topology.

Schema 2 checks stable references and CloudFormation identities, function and
mapping concurrency, aggregate connection pressure, queue visibility and
retention, FIFO/redrive compatibility, partial-batch responses and minimal-idle
schedule policy. It derives typed IAM intent rather than accepting secret
values or free-form policy documents.

Cost output classifies structural dimensions as `zero_compute`,
`request_only`, `storage_only`, `scheduled_wakeup` or `fixed_monthly`, with
`priced`, `unpriced`, `region_dependent`, `free_tier_dependent` or
`eligibility_dependent` pricing confidence. Provider allowances never become a
complete zero-dollar estimate solely because current usage fits.

It also records only the AWS service set needed for local adapter conformance
and can render deterministic AWS SAM/CloudFormation for PostgreSQL-compatible
API/worker topologies. Local topology never runs schedules. Generic DynamoDB
SAM fails closed because its table and IAM must be declared by an
access-pattern-specific adapter.

An explicitly selected static-site intent can add a retained, encrypted,
publicly blocked S3 bucket, CloudFront OAC with SigV4 signing, an explicit cache
policy, SPA fallback, and optional certificate/Route 53 parameters. The default
plan remains API-only and adds none of those resources.
An optional one-time schedule cleanup contract records
`ActionAfterCompletion: DELETE`, residual resources and a manual fallback; it
is rejected for recurring schedules. SAM rendering fails closed because the
current SAM and CloudFormation schedule schemas do not expose that Scheduler
API property.
`allowed_origins` and `allowed_headers` are exact configuration inputs: empty,
wildcard, invalid, and duplicate header lists are rejected, and the normalized
values are carried into both Plan IR and generated SAM CORS policy. A plugin
that requires an additional browser request header must have that header
explicitly represented in the selected application configuration.

Applications should treat the generated plan as reviewable deployment evidence,
not as an exact cloud bill forecast.

See
[`docs/deployment/plan-schema-v2-migration.md`](../../docs/deployment/plan-schema-v2-migration.md)
for the schema 2 compatibility boundary and explicit upgrade procedure.
