# Protect HTTP operations with API Gateway traffic budgets

Minco can apply explicit request-rate and burst targets at its existing API
Gateway HTTP API ingress before requests invoke Lambda. This is an opt-in
production boundary for coarse application protection; it is not a distributed
per-user rate limiter, an authorization mechanism, or a hard spending cap.

## Why the limit lives at ingress

API Gateway HTTP APIs use token-bucket throttling. AWS supports one default
stage target plus route-specific overrides and begins returning `429 Too Many
Requests` when requests exceed the effective target. AWS documents these limits
as **best-effort targets**, not guaranteed ceilings.

For Minco's default AWS topology this is preferable to adding Redis, a database
counter, or Tower middleware merely to protect the whole API. Requests rejected
at API Gateway do not invoke the Lambda application or consume its downstream
database work. API Gateway request charges can still apply, so throttling must
not be described as a zero-cost firewall or exact spend limit.

## Define an explicit policy

Traffic overrides use canonical OpenAPI operation IDs. Minco resolves those IDs
against the reviewed `DeploymentPlan` rather than asking applications to repeat
method/path strings.

```rust
use minco_plan::{HttpTrafficPolicy, TrafficBudget};

let traffic = HttpTrafficPolicy::new(Some(TrafficBudget::new(25.0, 50)))
    .with_operation(
        "createObjectTransfer",
        TrafficBudget::new(2.0, 4),
    )
    .with_operation(
        "exportAuditLedger",
        TrafficBudget::new(1.0, 2),
    );
```

`rate_per_second` must be finite and greater than zero, and `burst` must be
non-zero. Unknown operation IDs fail closed. If two selected operations resolve
to the same API Gateway route key, rendering also fails rather than silently
replacing one setting.

Minco deliberately does not invent rate values from Lambda concurrency,
database connection limits, or expected traffic. Those dimensions inform human
capacity planning but do not prove the correct product-level traffic budget.

## Render the protected topology

Use the traffic-aware renderer when this policy is selected:

```rust
use minco_plan::render_sam_with_traffic_policy;

let sam = render_sam_with_traffic_policy(&deployment_plan, &traffic)?;
```

Equivalent helpers are available when packaging supplies one or many code URI
overrides:

- `render_sam_with_code_uri_and_traffic_policy`
- `render_sam_with_code_uris_and_traffic_policy`

The renderer adds AWS SAM `DefaultRouteSettings` and `RouteSettings` to Minco's
`$default` HTTP API stage and the equivalent settings to the separately rendered
`candidate` stage. Candidate verification therefore does not accidentally run
against a less-protected traffic topology.

The ordinary `render_sam`, `render_sam_with_code_uri`, and
`render_sam_with_code_uris` functions are unchanged. No throttle exists unless
the application explicitly selects a traffic policy.

## Choose budgets deliberately

A useful starting process is:

1. identify operations where abuse or accidental retries can create expensive
   Lambda, database, mail, object, or third-party work;
2. choose a conservative whole-API default that still leaves normal burst room;
3. tighten expensive mutation or export operations individually by operation ID;
4. keep client retry behavior bounded and honor `429` responses with backoff;
5. review traffic changes with the deployment change set just like concurrency,
   timeout, and other capacity controls; and
6. adjust only from measured product traffic and failure evidence.

A gateway-wide budget is deliberately coarse. Login attempts, password reset,
OTP verification, tenant quotas, per-user abuse controls, and billing limits
usually require a stable identity key plus shared state. Those remain
application/provider-specific concerns and should not be approximated with
per-Lambda memory counters.

## `429` response boundary

Minco application-generated throttling failures can use its Problem Details and
`Retry-After` helpers. API Gateway-generated throttling responses are owned by
the ingress provider and are not claimed to use Minco's Problem Details body.
Clients should therefore treat HTTP status `429` as authoritative even when the
provider body differs from an application response.

## Cost and architecture boundary

Selecting this policy:

- creates no Redis/cache service;
- adds no scheduled wakeup;
- adds no provisioned or always-on compute;
- adds no Rust request-path limiter or distributed counter;
- adds no extra AWS resource beyond settings on stages Minco already renders;
  and
- can reduce Lambda and downstream work during excess traffic, while not
  eliminating API Gateway request charges.

This preserves Minco's minimal-cost principle: use the managed AWS boundary that
already receives the request before introducing another runtime subsystem.

## Provider references

- AWS API Gateway: *Throttle requests to your HTTP APIs for better throughput*
- AWS SAM: `AWS::Serverless::HttpApi` `DefaultRouteSettings` and `RouteSettings`
- AWS CloudFormation: `AWS::ApiGatewayV2::Stage` route settings
