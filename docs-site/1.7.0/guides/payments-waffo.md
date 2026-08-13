---
title: Waffo Hosted Payments
description: Add provider-specific Waffo checkout and verified webhooks without giving the framework ownership of payment state.
---

# Waffo Hosted Payments

`minco-plugin-payments-waffo` is an opt-in beta integration for Waffo Pancake.
It supplies signed typed actions, hosted checkout, read-only GraphQL, raw-body
webhook verification and offline test doubles. It does not add a generic
billing model or make Minco authoritative for orders, subscriptions,
entitlements, invoices or transactions.

The provider contract was reviewed against the official Waffo Go SDK at exact
revision `799135cbe07c45819da0ab4bf777c64fcc956220`. That source review and
local tests do not prove a live Waffo account or production readiness.

## Select it explicitly

The plugin is absent from the facade defaults:

```toml
[dependencies]
minco = { version = "1.6.0", features = ["plugin-payments-waffo"] }
minco-plugin-payments-waffo = "1.6.0"
```

Use the exact published 1.6.0 versions shown above. The feature also enables Minco's
idempotency plugin because typed provider actions require an explicit claim.

## Configure without embedding secrets

```toml
schema = 1
environment_class = "test"

[values.plugins.payments-waffo]
environment = "test"
merchant_id = "MER_0123456789ABCDEFGHIJKL"
private_key = "env:WAFFO_PRIVATE_KEY"
```

Use opaque `env:` references locally. An application/runtime-owned AWS resolver
may resolve exact `ssm:` references, but this provider-neutral plugin does not
link the AWS SDK. Production mode must match `environment_class =
"production"`; writes also require `allow_production_writes = true`.

A custom API origin is a trusted-operator test seam only. It needs
`allow_custom_api_base_url = true` and cannot be used with production mode.

## Create checkout deliberately

```rust,no_run
use minco_config::EnvironmentClass;
use minco_plugin_payments_waffo::{Checkout, EnvironmentSecretResolver};

# async fn example(service: &minco_plugin_payments_waffo::WaffoService) -> Result<(), Box<dyn std::error::Error>> {
let client = service
    .client(EnvironmentClass::Test, &EnvironmentSecretResolver)
    .await?;
let response = Checkout::guest("PROD_0123456789ABCDEFGHIJKL", "AUD")
    .order_reference("order-42")
    .return_to("https://example.com/billing/complete")
    .create(&client, "checkout_order_42")
    .await?;
println!("{}", response.data.checkout_url);
# Ok(())
# }
```

Use the same provider idempotency key only for an intentional retry of the
same request. Authenticated checkout has separate token and checkout keys.
Session bearer tokens are redacted, zeroized and excluded from Minco's generic
idempotency persistence. Signed HTTP requests reject redirects. Provider
checkout destinations must be absolute HTTPS URLs without credentials or an
existing fragment before Minco adds the token fragment.

## Make verified webhooks authoritative

Preserve the exact request bytes and signature header. Before decoding JSON:

1. enforce the configured body-size bound;
2. verify the signature and timestamp window;
3. bind the expected Waffo environment and store;
4. atomically claim the provider/store/delivery identity; and
5. apply one idempotent application-owned command or projection.

A checkout return URL is navigation, not payment proof. Provider `aiHint`
values are untrusted diagnostics, not agent instructions.

## Test and classify evidence

`FakeWaffoTransport` queues endpoint-specific results and never falls back to
the network. Cover exact request bodies, repeated keys, conflicting bodies,
tampered/stale webhooks, wrong environments/stores, redirects, unsafe checkout
URLs, malformed responses and byte bounds.

The offline conformance report keeps `provider_live = not_run` and
`production_readiness = not_assessed`. No hidden poller, scheduler, queue,
database, fixed compute or AWS resource is created by this plugin.

Next: [install and compose plugins](../plugins/using-plugins), [test plugin
conformance](./plugin-conformance), or review [testing and evidence](../reference/testing).
