---
title: Events and Notifications
description: Publish domain facts and user-facing delivery through explicit ports, outboxes, and workers.
---

# Events and Notifications

The events plugin describes domain events and transactional-outbox ports. The
notifications plugin describes email, webhook, in-app, and developer channels.
The audit plugin records append-only business history independently of
operational logs.

## Enable the capabilities

```bash
cargo minco plugin enable events --dry-run --json
cargo minco plugin enable notifications --dry-run --json
cargo minco plugin enable audit --dry-run --json
```

Compile-time features and explicit constructors still decide which adapters are
present. Enabling metadata alone does not choose SES, SNS, a webhook client, or
a database table.

## Publish facts, not commands in disguise

A domain event records something that happened using a stable application-owned
schema and identifier. The originating use case writes state and its outbox
entry atomically when guaranteed delivery is required.

```text
use case transaction -> domain state + outbox record
explicit worker      -> claim -> deliver -> mark result
```

Request-assisted dispatch can reduce latency, but the durable outbox remains
the recovery authority. There is no hidden polling loop or scheduler.

## Deliver notifications

Notification ports accept a typed message and channel intent. Adapters own
provider payloads, rate limits, transient/permanent failure mapping, and
provider identifiers. Application policy owns recipients, consent, templates,
localization, and whether a failed notification should affect the originating
use case.

Never put credentials or raw provider diagnostics into an event, public Problem
response, or generated plan.

## Add a worker explicitly

Use the [queues and workers guide](./background-work) for SQS-driven dispatch.
Plan IR must expose the queue, mapping, retries, DLQ, IAM, connection budget,
cost class, and `queue_message` wake source.

## Test failure policy

- application tests prove event creation and fail-before-persistence rules;
- adapter tests prove atomic outbox behavior against the real database engine;
- worker tests prove retry classification and partial-batch results;
- channel contract tests use fakes or provider sandboxes;
- bounded provider smokes prove only the named account, Region, channel, and
  exact message fixture.

Operational logs explain delivery. Audit history explains business action.
Neither substitutes for the other.
