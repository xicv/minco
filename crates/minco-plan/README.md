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

It also records only the AWS service set needed for local adapter conformance
and can render deterministic AWS SAM/CloudFormation for PostgreSQL-compatible
API/worker topologies. Local topology never runs schedules. Generic DynamoDB
SAM fails closed because its table and IAM must be declared by an
access-pattern-specific adapter.
`allowed_origins` and `allowed_headers` are exact configuration inputs: empty,
wildcard, invalid, and duplicate header lists are rejected, and the normalized
values are carried into both Plan IR and generated SAM CORS policy. A plugin
that requires an additional browser request header must have that header
explicitly represented in the selected application configuration.

Applications should treat the generated plan as reviewable deployment evidence,
not as an exact cloud bill forecast.

See
[`docs/deployment/plan-schema-v2-migration.md`](../../docs/deployment/plan-schema-v2-migration.md)
for the likely 0.4 compatibility boundary and explicit upgrade procedure.
