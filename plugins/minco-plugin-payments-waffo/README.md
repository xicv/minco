# Minco Waffo Pancake payments plugin

`minco-plugin-payments-waffo` is an opt-in Minco integration for Waffo Pancake's hosted checkout, signed server API, read-only GraphQL queries, and standard HTTP webhooks.

It follows Minco's static-composition model:

- plugin discovery and installation are deterministic and make no network calls;
- private and public keys remain opaque `env:` or `ssm:` references until an explicit operation resolves them;
- outbound writes require both a caller-supplied Waffo idempotency key and Minco's `idempotency.claim` service;
- production writes require `environment = "production"`, a Minco `production` environment class, and the persisted `allow_production_writes = true` guard;
- request, response, and webhook bodies are bounded;
- webhook signatures are verified against the untouched raw body before JSON decoding;
- no hidden retries, timers, queues, databases, or fixed-capacity infrastructure are introduced.

## Add the plugin

From a Minco application:

```bash
cargo minco plugin add payments-waffo
```

The facade feature is `plugin-payments-waffo`. The plugin also depends on the official idempotency plugin, which Minco resolves statically.

## Configuration

Create `minco.waffo.toml`:

```toml
schema = 1
environment_class = "test"

[values.plugins.payments-waffo]
environment = "test"
merchant_id = "MER_REPLACE_ME"
private_key = "env:WAFFO_PRIVATE_KEY"

# Configure all four webhook fields together when webhook automation is needed.
# webhook_public_key = "ssm:/my-app/test/waffo-webhook-public-key"
# store_id = "STO_REPLACE_ME"
# webhook_url = "https://api.example.com/webhooks/waffo"
# webhook_events = ["order.created", "subscription.renewed"]
```

Secret values never belong in this file. For Lambda deployments, use an `ssm:/absolute/name` reference and grant only `ssm:GetParameter` for that exact parameter. Local development can use `env:NAME`.

A production configuration must declare both `environment_class = "production"` and `environment = "production"`. Mutating commands additionally require:

```toml
allow_production_writes = true
```

Production credentials are restricted to Waffo's official API origin. A custom compatible endpoint is available only for test credentials and requires the explicit `allow_custom_api_base_url = true` flag.

## Command-line automation

Install the dedicated provider CLI without expanding `cargo-minco` or the default Minco binary:

```bash
cargo install minco-plugin-payments-waffo \
  --version 1.1.0 \
  --features cli \
  --bin minco-waffo
```

Every successful command emits a versioned JSON envelope. Errors also emit JSON and use stable `waffo.*` codes.

```bash
minco-waffo --config minco.waffo.toml config-check
minco-waffo --config minco.waffo.toml doctor
minco-waffo idempotency-key

minco-waffo --config minco.waffo.toml checkout-create \
  --body checkout.json \
  --idempotency-key order_2026_08_06_001

minco-waffo --config minco.waffo.toml action \
  --path /v1/actions/store/add-webhook \
  --body request.json \
  --idempotency-key webhook_2026_08_06_001

minco-waffo --config minco.waffo.toml graphql \
  --query query.graphql \
  --variables variables.json

minco-waffo --config minco.waffo.toml webhook-add \
  --idempotency-key webhook_registration_001

WAFFO_SIGNATURE='t=...,v1=...' \
  minco-waffo --config minco.waffo.toml webhook-verify --body raw-body.json
```

Use `-` for a request body, GraphQL query, variables document, or webhook body read from standard input. Do not use standard input for both the configuration file and an operation body in the same process.

`doctor` resolves and parses configured keys but does not contact Waffo. Write retries must be explicit and reuse the same idempotency key so Waffo and Minco can replay the original result safely.

## Application usage

```rust,no_run
use minco_plugin_payments_waffo::{
    CreateCheckoutSessionRequest, EnvironmentSecretResolver, WaffoService,
};

# async fn example(service: &WaffoService, request: CreateCheckoutSessionRequest) -> Result<(), Box<dyn std::error::Error>> {
let client = service.client(&EnvironmentSecretResolver).await?;
let session = client
    .create_checkout_session(&request, "order_2026_08_06_001")
    .await?;
println!("{}", session.checkout_url);
# Ok(())
# }
```

For an HTTP webhook handler, preserve the exact request bytes, pass the `X-Waffo-Signature` header and raw bytes to `WaffoWebhookVerifier::verify`, then claim the emitted `event_dedupe_key` before applying business effects.
