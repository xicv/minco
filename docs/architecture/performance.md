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

## Checked candidate baseline

`verification/performance-policy.toml` defines a bounded clean-runner policy.
An exact hosted run may produce a reviewed `PASS` record after the source
manifest is final. Until then the discriminated record is `NOT RUN` with a
reason and limitations and no invented measurements. A PASS records exact
source, p50/p95/p99/maximum, throughput, counts/failures/error rate, warm/cold
classification, environment fingerprint, artifact/memory values actually
observed, scope and limitations. The hosted runner record repeats the verified
source-tree digest; the validator requires it to match both the baseline and
the current canonical source manifest. Its Git revision remains a provenance
locator, while the recomputed source-tree digest is the offline source authority.

The policy is intentionally not a production SLO. Hosted runner placement,
contention, architecture, and network path differ from a real AWS target. Its
purpose is to detect large regressions and missing evidence before release.
Provider p95/p99 and cold/warm measurements remain a separate live application
gate.

Freshness uses the explicit repository review date (or a validated reported
CLI override), never runner wall time. The validator rejects non-finite values,
requires monotonic percentiles, treats zero-to-zero as finite zero, and fails a
zero-to-positive regression as unbounded.

`validate_operational_evidence.py` fails when a PASS baseline is stale, bound to a
different source tree/version, internally inconsistent, outside reviewed
budgets, or missing its explicit `no AWS contact` / `no production SLO claim`
scope. Regenerating a baseline changes evidence; it does not automatically
justify raising a budget.

## Provider and capability freshness

`verification/provider-evidence.toml` separates current published-release proof
from historical observations. Source qualification may retain an explicit
`not_run` warning, but a release policy that requires live proof must use
`--require-current-provider` and fail.

`verification/aws-capability-candidates.toml` records upstream AWS/Rust options
with support status, cost classes, wake sources, prerequisites, source links,
implementation/test paths, blockers, and adoption triggers. Upstream availability
never changes Minco support implicitly.
