# Minco Waffo Pancake payments plugin

`minco-plugin-payments-waffo` is an opt-in Minco integration for Waffo Pancake hosted checkout, signed server actions, read-only GraphQL queries, and standard HTTP webhooks. It is currently unreleased work in draft PR #125.

The provider contract in this source tree was reviewed against the official Waffo Go SDK at `df098331cf5ea7d43ad79ab223d9eda6d4ac8e5f`. Provider review, source qualification, sandbox evidence, publication, deployment, and production readiness remain separate states.

## Design

- plugin discovery and installation are deterministic and make no network calls;
- private and public keys remain opaque `env:` or `ssm:` references until an explicit operation;
- writes require a caller-supplied Waffo idempotency key and Minco's idempotency capability;
- production writes require matching production environments and the persisted production guard;
- request, response, and webhook bodies are bounded;
- webhook signatures are verified against untouched raw bytes before JSON decoding;
- no hidden timers, queues, databases, ORM models, or fixed-capacity infrastructure are introduced;
- applications own orders, entitlements, subscriptions, invoices, transaction projections, and access policy.

## Add the plugin

From a Minco application:

```bash
cargo minco plugin add payments-waffo
```

The facade feature is `plugin-payments-waffo`. The plugin depends on Minco's official idempotency plugin and remains disabled by default.

## Configuration

Create `minco.waffo.toml`:

```toml
schema = 1
environment_class = "test"

[values.plugins.payments-waffo]
environment = "test"
merchant_id = "MER_0123456789ABCDEFGHIJKL"
private_key = "env:WAFFO_PRIVATE_KEY"

# Configure all webhook fields together when registration and verification are needed.
# webhook_public_key = "ssm:/my-app/test/waffo-webhook-public-key"
# store_id = "STO_0123456789ABCDEFGHIJKL"
# webhook_url = "https://api.example.com/webhooks/waffo"
# webhook_events = ["order.completed", "subscription.payment_succeeded"]
```

Secret values never belong in this file. Local development can use `env:NAME`. Lambda deployments should use an exact `ssm:/absolute/name` reference with least-privilege `ssm:GetParameter` access.

A production configuration must declare both `environment_class = "production"` and `environment = "production"`. Mutating commands additionally require:

```toml
allow_production_writes = true
```

Production credentials are restricted to Waffo's official API origin. A custom compatible endpoint is available only for test credentials and requires the explicit `allow_custom_api_base_url = true` flag.

## Fluent checkout

The common path is a pure fluent value object followed by one explicit provider call:

```rust,no_run
use minco_plugin_payments_waffo::{Checkout, EnvironmentSecretResolver};

# async fn example(service: &minco_plugin_payments_waffo::WaffoService) -> Result<(), Box<dyn std::error::Error>> {
let client = service.client(&EnvironmentSecretResolver).await?;
let session = Checkout::guest("PROD_0123456789ABCDEFGHIJKL", "AUD")
    .buyer_email("buyer@example.com")
    .return_to("https://example.com/billing/complete")
    .order_reference("order-42")
    .metadata("cart_id", "cart-7")
    .create(&client, "checkout_order_42")
    .await?;
println!("{}", session.checkout_url);
# Ok(())
# }
```

The value object validates exact Waffo product IDs, typed price and billing structures, payment methods, languages, string metadata, currency, and HTTPS return URLs before the provider request is sent.

## Command-line automation

The direct checkout command covers the common workflow without a hand-authored JSON body:

```bash
minco-waffo --config minco.waffo.toml checkout \
  --product-id PROD_0123456789ABCDEFGHIJKL \
  --currency AUD \
  --buyer-email buyer@example.com \
  --return-to https://example.com/billing/complete \
  --order-reference order-42 \
  --metadata cart_id=cart-7 \
  --idempotency-key checkout_order_42
```

Advanced and operational commands remain available:

```bash
minco-waffo --config minco.waffo.toml config-check
minco-waffo --config minco.waffo.toml doctor
minco-waffo idempotency-key
minco-waffo --config minco.waffo.toml checkout-create --body checkout.json --idempotency-key order_42
minco-waffo --config minco.waffo.toml action --path /v1/actions/store/add-webhook --body request.json --idempotency-key webhook_42
minco-waffo --config minco.waffo.toml graphql --query query.graphql --variables variables.json
minco-waffo --config minco.waffo.toml webhook-add --idempotency-key webhook_registration_42
WAFFO_SIGNATURE='t=...,v1=...' minco-waffo --config minco.waffo.toml webhook-verify --body raw-body.json
```

Every successful command emits a versioned JSON envelope. `config-check` resolves no secrets; `doctor` resolves and parses configured keys but does not contact Waffo. The CLI's Minco idempotency store is process-local, while the explicit Waffo key must be reused across an intentional retry.

## Webhook authority

A successful checkout return is user experience, not payment authority. Preserve the exact webhook body, verify it, durably claim the emitted deduplication key, then apply an application-owned projection or domain command. Keep received, handled, ignored, retryable-failure, and terminal-failure outcomes inspectable.

The plugin does not create generic subscription or transaction tables because those models, authorization rules, and retention requirements are product policy.

## Agent guidance

Plugin-focused Codex guidance is packaged under:

```text
agent/skills/minco-waffo-payments/SKILL.md
```

It routes checkout, webhook, and testing work without expanding Minco's global eight-skill bundle.
