# minco-plan

Provider-neutral deployment planning for Minco.

The Plan IR connects OpenAPI operations to runtime, ingress, authentication,
the configured static application graph, database, performance, and cost-policy
decisions. It includes structural checks for fixed compute, scheduled wakeups,
connection pressure, mutable SQLite on Lambda, DynamoDB acknowledgement, and
other minimal-idle-cost constraints.

It also records the minimal AWS service set needed for local adapter
conformance and can render Minco's baseline AWS SAM/CloudFormation topology.

Applications should treat the generated plan as reviewable deployment evidence,
not as an exact cloud bill forecast.
