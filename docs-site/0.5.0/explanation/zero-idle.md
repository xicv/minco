---
title: Zero idle, precisely
description: Understand Minco’s zero-provisioned-compute promise and residual cost model.
---

# Zero idle, precisely

Minco targets **zero provisioned application compute at idle**. This is a
precise infrastructure property, not a promise of a zero AWS bill.

The minimal profile has no:

- NAT Gateway;
- fixed application compute;
- provisioned Lambda concurrency;
- scheduled poller or implicit wakeup.

## Costs that can remain

Storage, retained logs, DNS, secrets, database storage, schedules, requests, and
other selected resources can cost money while no user is active. Plans keep
those dimensions visible.

| Cost class | Meaning |
|---|---|
| `zero_compute` | no provisioned application compute remains while idle |
| `request_only` | cost follows invocations or operations |
| `storage_only` | retained bytes or records can cost money without traffic |
| `scheduled_wakeup` | a declared schedule can invoke work without traffic |
| `fixed_monthly` | the selected resource has a recurring fixed dimension |

Pricing evidence also states whether a value is priced, unpriced,
Region-dependent, free-tier-dependent, or eligibility-dependent.

## Why it changes development

When idle environments are cheap, teams can deploy earlier and keep review
loops closer to the work. Minco therefore treats feedback, exact source,
immutable artifacts, tests, hosted observations, and cleanup evidence as part
of the framework—not afterthoughts.

Every preview remains application-owned and opt-in. Expiry does not authorize
deletion; cleanup still requires an exact identity, plan, environment guard,
and explicit apply authority.
