# ADR 0025: Zero provisioned compute and the Verified Review Loop

## Status

Accepted

## Context

“Zero idle” was used as shorthand for the absence of always-on application
compute, but it could be misread as a promise of a zero AWS bill. Storage,
retained logs, DNS, secrets, database storage, schedules and other dimensions
can cost money while an application receives no traffic. They can also create
wakeups or retain data after compute has scaled to zero.

Feedback already provides a typed, bounded vertical slice. The framework also
needs an application-review model that can bind untrusted feedback to the
exact source, release and optional review environment that produced it without
requiring a Minco-hosted control plane.

Provider behavior was checked on 2026-07-28 against the AWS documentation for
[Lambda scale-to-zero functions](https://aws.amazon.com/lambda/lambda-functions/),
[DynamoDB on-demand capacity](https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/on-demand-capacity-mode.html),
[CloudWatch Logs retention](https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/WhatIsCloudWatchLogs.html),
[SSM Parameter Store pricing](https://aws.amazon.com/systems-manager/pricing/),
[Aurora Serverless v2 auto-pause](https://docs.aws.amazon.com/AmazonRDS/latest/AuroraUserGuide/aurora-serverless-v2-auto-pause.html),
and
[EventBridge Scheduler completion cleanup](https://docs.aws.amazon.com/scheduler/latest/UserGuide/managing-schedule-delete.html).
These sources are provider reference data, not a guarantee about a particular
account, Region, workload or future price.

## Decision

Minco targets zero provisioned application compute at idle. Storage, retained
logs, DNS, secrets, database storage, schedules and other fixed/request
dimensions remain explicit and bounded.

Plans and documentation use these cost classes:

- `zero_compute`: no provisioned application compute remains while idle;
- `request_only`: usage is driven by requests, invocations or operations;
- `storage_only`: retained bytes or records can cost money without traffic;
- `scheduled_wakeup`: an explicit schedule can invoke work without traffic;
- `fixed_monthly`: a selected resource has a recurring fixed price dimension.

Pricing evidence uses these confidence labels:

- `priced`: a dated, Region-specific price was resolved;
- `unpriced`: Minco has no reliable current price;
- `region_dependent`: availability or price depends on the selected Region;
- `free_tier_dependent`: a zero estimate relies on free-tier allowance;
- `eligibility_dependent`: a plan or discount depends on account eligibility.

This release records the vocabulary and operator doctrine without expanding
Plan IR schema 2 into a general pricing engine. A later research task must
prove the smallest useful structured extension against at least two provider
profiles.

The Verified Review Loop is repository-native:

1. an application builds an immutable release and optional review environment;
2. review metadata binds a stable review ID to the source revision, release
   manifest digest, artifact digests, target, owner and expiry;
3. Feedback submissions retain their own stable IDs and digests and reference
   that review ID;
4. automation treats all feedback text and attachments as untrusted input,
   validates size and media policy, and never executes supplied content;
5. review, deployment, verification and cleanup receipts remain inspectable in
   the application repository or its explicitly selected storage;
6. any future delivery trace links one reviewed change to one exact artifact
   and environment without introducing a global Minco service.

No review environment is created implicitly. Local Feedback works without
AWS, and a deployed review loop is an application-owned, opt-in deployment
profile. Preview lifecycle, delivery-trace schema and guarded cleanup remain
future M10 work.

## Consequences

- “Zero idle” is a compute property, not a price promise.
- Fixed, retained, scheduled and unknown dimensions remain visible even when
  compute can scale to zero.
- Account eligibility, Region availability, quotas and provider price changes
  are live operational gates.
- Feedback can participate in a traceable review workflow without becoming a
  hosted Minco product or trusted instruction channel.
- Review identity and cleanup are explicit before any automation is added.

## Safety

Plans and receipts contain identifiers, digests, classification and secret
names only, never secret values. Feedback remains size-bounded, content-typed,
authorization-checked and untrusted. Expiry does not authorize deletion;
cleanup needs an exact identity, a preceding plan, environment guards and an
explicit apply approval.
