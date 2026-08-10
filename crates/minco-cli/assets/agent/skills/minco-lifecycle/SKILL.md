---
name: minco-lifecycle
description: >-
  Plan and execute Minco configuration, database, seed, local development,
  testing, packaging, or deployment-lifecycle work through explicit guarded
  commands. Use when a task concerns environments, secrets, migrations, seeds,
  dev processes, local AWS services, release artifacts, or evidence receipts.
---

# Use the Minco lifecycle

Choose the narrowest lifecycle command and inspect before applying.

1. Validate configuration with `cargo minco config check --json` and use
   `config explain` or `config diff` without exposing secret values.
2. For databases, inspect `cargo minco db plan --json` and status before any
   migration or seed action. Production migrations remain explicit release
   operations and never run at Lambda startup.
3. Classify seeds and use their dry-run/preservation guards before application.
4. Inspect local topology with `cargo minco dev --dry-run --json`. Start only
   requested profiles, workers, frontend processes, or owned local services
   such as PostgreSQL, Rustack, or Mailpit; never adopt a foreign resource.
5. For rich mail, inspect submission, capture and delivery-observation states
   separately; ambiguous provider acceptance never authorizes an automatic
   retry or failover.
6. Inspect topology-aware cost whenever runtime, ingress, queues, schedules,
   realtime, concurrency or fixed resources change.
7. Run focused tests associated with the changed behavior.
8. Keep package, hosted verification, deployment, promotion, rollback, runtime,
   and review receipts separate and bound to exact artifacts.

Planning never authorizes apply. Stop before any database mutation, provider
call, deployment, promotion, rollback, cleanup, or production action unless the
user explicitly requests it and the command's exact guards are satisfied.

Read [workflow.md](references/workflow.md) for the lifecycle decision table.
