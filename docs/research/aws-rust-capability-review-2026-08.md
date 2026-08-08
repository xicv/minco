# AWS and Rust capability review — August 2026

- Reviewed: 2026-08-07
- Dependency context: workspace/lockfile AWS SDK versions; `lambda_http` and
  `lambda_runtime` 1.3; `aws_lambda_events` 1.2
- Authority: AWS primary documentation and official project repositories
- Rule: upstream availability is not Minco support

This review adds no AWS resource, renderer, runtime scanner, generic service
facade or always-on control plane. Full machine fields are in
`verification/aws-capability-candidates.toml`. No candidate is `supported`.

## AWS candidates

| Candidate | State | Residual cost/wake concern | Minco decision |
|---|---|---|---|
| Lambda Function URLs | declared, unsupported | Lambda request/compute/transfer; no API Gateway request charge | Fail before rendering |
| Aurora Serverless v2 zero ACU | research | storage/cluster remains; slow resume/pause blockers | Defer database profile |
| Aurora DSQL | research | storage; bounded PostgreSQL subset | Defer use-case adapter |
| AppConfig/experimentation | deferred | polling, receipts, experiment-hours | No Minco poller |
| EventBridge Scheduler | research | scheduled wake/target/DLQ/log cost | Explicit schedule slice only |
| EventBridge Pipes | deferred | requests plus source/enrichment/target | No generic event facade |
| Step Functions Express | deferred | requests/duration/memory/logs | Workflow-specific only |
| AppSync Events | research | connection minutes/operations/transfer | Provider boundary incomplete |
| Powertools for Rust | deferred | telemetry/data overhead | Official list lacks Rust |
| Lambda Web Adapter | research | Lambda request/compute | Upstream is not Minco support |
| CloudFront Functions/KVS | deferred | request and KVS metering | CDN-specific JavaScript only |
| Lambda Managed Instances | rejected for minimal | retained EC2 capacity/fee | Separate fixed profile only |
| Lambda Durable Functions | deferred | request/state/duration | Runtime list lacks Rust |
| Lambda MicroVMs | deferred | lifecycle/pricing/cleanup unknown | Sandbox design required |

### Lambda Function URLs

AWS provides a public HTTPS endpoint with `AWS_IAM` or `NONE` auth and no
PrivateLink. The endpoint adds no API Gateway request charge; Lambda request,
duration, streaming and transfer remain. Minco cannot reuse its API Gateway JWT
boundary. Rendering, auth, qualification, promotion and recovery are missing,
so `MINCO-PLAN-INGRESS-001` remains correct.

Sources: [configuration](https://docs.aws.amazon.com/lambda/latest/dg/urls-configuration.html),
[authentication](https://docs.aws.amazon.com/lambda/latest/dg/urls-auth.html),
[selection/cost](https://docs.aws.amazon.com/lambda/latest/dg/furls-http-invoke-decision.html),
[streaming](https://docs.aws.amazon.com/lambda/latest/dg/configuration-response-streaming.html).

### Aurora Serverless v2 and DSQL

Eligible Aurora engines/Regions can pause at zero ACU, but storage and retained
cluster resources remain. Resume is usually around 15 seconds and can exceed
30 after deep sleep. RDS Proxy, replication/global databases, zero-ETL and
maintenance affect pause. A profile needs exact eligibility, connection,
migration/recovery, latency, pricing and provider proof.

Sources: [auto-pause](https://docs.aws.amazon.com/AmazonRDS/latest/AuroraUserGuide/aurora-serverless-v2-auto-pause.html),
[requirements](https://docs.aws.amazon.com/AmazonRDS/latest/AuroraUserGuide/aurora-serverless-v2.requirements.html).

Aurora DSQL is serverless/distributed with PostgreSQL wire compatibility, IAM
tokens, optimistic concurrency and asynchronous DDL. It is a documented subset,
not PostgreSQL equivalence. Activity can reach zero DPUs; storage remains.
Minco needs a use-case-shaped port/adapter and explicit token, DDL, recovery,
performance and cost contracts.

Sources: [overview](https://docs.aws.amazon.com/aurora-dsql/latest/userguide/what-is-aurora-dsql.html),
[compatibility](https://docs.aws.amazon.com/aurora-dsql/latest/userguide/working-with.html),
[authentication](https://docs.aws.amazon.com/aurora-dsql/latest/userguide/authentication-authorization.html),
[pricing](https://aws.amazon.com/rds/aurora/dsql/pricing/).

### AppConfig

AppConfig supports validated configuration/flags, staged rollout, monitoring
and rollback. Its agent caches and polls; the Lambda extension documents a
45-second default poll interval. Experimentation adds experiment-hour and
request/receipt costs. Minco adopts validation/rollback patterns, but rejects
an always-on framework poller.

Sources: [overview](https://docs.aws.amazon.com/appconfig/latest/userguide/what-is-appconfig.html),
[agent](https://docs.aws.amazon.com/appconfig/latest/userguide/appconfig-agent.html),
[Lambda extension](https://docs.aws.amazon.com/appconfig/latest/userguide/appconfig-integration-lambda-extensions-config.html),
[pricing](https://aws.amazon.com/systems-manager/pricing/).

### Scheduler, Pipes and Express workflows

Scheduler supplies one-time/rate/cron schedules, retries and optional DLQ with
at-least-once delivery. Charges include invocations plus target/queue/log costs;
wake and cleanup must remain explicit.

Pipes connects one source to one target with filtering/enrichment. AWS notes
that unmatched SQS messages are removed. Pricing uses 64 KiB chunks after
filtering, with other service costs additional. Source-specific ordering,
failure and IAM must not be erased by a facade.

Express workflows have a five-minute limit. Async is at-least-once; sync is
at-most-once. Express does not supply idempotency, lacks `.sync` and callback
task-token patterns, and uses CloudWatch Logs for history.

Sources: [Scheduler](https://docs.aws.amazon.com/scheduler/latest/UserGuide/what-is-scheduler.html),
[Pipes](https://docs.aws.amazon.com/eventbridge/latest/userguide/pipes-concepts.html),
[Pipes filtering](https://docs.aws.amazon.com/eventbridge/latest/userguide/eb-pipes-event-filtering.html),
[Express comparison](https://docs.aws.amazon.com/step-functions/latest/dg/choosing-workflow-type.html).

### AppSync Events

Managed WebSocket pub/sub supports HTTP/WebSocket publishing and API key, IAM,
Cognito, OIDC or Lambda auth. Operations, connection minutes and transfer are
metered, so connected clients are not zero-cost. Minco still needs exact
channels/auth, backpressure/fan-out, promotion, recovery and live evidence.

Sources: [Event API](https://docs.aws.amazon.com/appsync/latest/eventapi/event-api-welcome.html),
[pricing](https://aws.amazon.com/appsync/pricing/).

### Rust Lambda utilities

AWS's Powertools page lists Python, TypeScript, Java and .NET—not Rust. The Rust
guide points to AWS SDK for Rust, Lambda runtime, Cargo Lambda, `lambda_http`
and tracing. A placeholder crate is not official support. Lambda Web Adapter is
AWS-maintained and useful as a packaging/translation pattern, but it does not
supply Minco's health, security, cost, promotion or recovery contract.

Sources: [Powertools list](https://docs.aws.amazon.com/lambda/latest/dg/powertools-for-lambda.html),
[Rust guide](https://docs.aws.amazon.com/lambda/latest/dg/lambda-rust.html),
[Web Adapter](https://github.com/aws/aws-lambda-web-adapter).

### CloudFront Functions and KVS

Functions is constrained JavaScript at viewer/connection events: no request
body, network, filesystem, environment variables or timers. KVS adds replicated
reads with separate limits/metering. Neither is a Rust application runtime.

Sources: [Functions](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/cloudfront-functions.html),
[restrictions](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/cloudfront-function-restrictions.html),
[KVS](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/kvs-with-functions.html).

### Newer Lambda compute/orchestration

Managed Instances normally retains multi-AZ EC2-backed capacity and does not
scale to zero, so it is rejected for the minimal profile despite custom Rust
runtimes. [Documentation](https://docs.aws.amazon.com/lambda/latest/dg/lambda-managed-instances.html).

Durable Functions adds checkpoint/replay orchestration, but current supported
runtimes omit Rust. Replay, migration, state protection, recovery and pricing
need a dedicated design. [Runtime list](https://docs.aws.amazon.com/lambda/latest/dg/durable-supported-runtimes.html).

AWS announced Lambda MicroVMs in July 2026 for isolated sandbox/job workloads.
It is not ordinary Lambda isolation. Minco needs a sandbox threat model, image/
persistence lifecycle, cleanup, pricing and live proof.
[AWS Compute Blog](https://aws.amazon.com/blogs/compute/announcing-lambda-microvms-serverless-compute-environments-with-vm-level-isolation-and-near-instant-startup/).

## Open-source lessons

| Projects | Adopt | Reject | Evidence needed |
|---|---|---|---|
| Cargo Lambda | Cargo-native deterministic artifacts, bounded emulation | Delegated deployment authority | artifact/security/promotion/recovery parity |
| nextest/semver-checks/llvm-cov/mutants | structured profiles, baselines, scoped coverage/mutations | unmeasured repo-wide gates | CI cost, determinism, useful signal |
| Loco/SST/Encore/Wing/Nitric | conventions, visible graphs, preflight/runtime split, concise intent | hidden services, required control plane, erased AWS semantics, new language layer | two implementations plus cost/security/recovery/provider proof |

Build-time tools add no deployed idle resource. They improve AI-first work only
when their state and findings remain deterministic, inspectable and explainable.
