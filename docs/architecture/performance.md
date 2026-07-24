# Performance and Cost Awareness

Performance and cost are design dimensions, not post-release clean-up.

## Structural budgets

The deployment planner checks:

- Lambda reserved and provisioned concurrency;
- database connections per execution environment;
- maximum potential connection multiplication;
- request timeout compatibility;
- scheduled wakeups;
- fixed-capacity resources;
- mutable SQLite on ephemeral Lambda storage;
- explicit log retention.

The default reference profile is 512 MB, 15 seconds, reserved concurrency 5, and two
PostgreSQL connections per Lambda execution environment, for a potential maximum of ten
connections. These are guardrails, not universal tuning answers.

## Measurement ladder

1. Pure domain/application benchmarks where justified.
2. Local Axum smoke load from OpenAPI examples.
3. Database adapter measurements against the real engine.
4. Hosted dev measurements for Lambda cold/warm duration, API integration latency, errors,
   throttles, and connection acquisition.

A baseline changes only through review. Performance regressions are evidence, not hidden by
regenerating a baseline.
