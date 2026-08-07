# Hosted checkout workflow

Prefer the fluent Rust value object or the direct CLI for the common guest-checkout path. Both validate the typed request before provider contact.

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

Use `checkout-create --body FILE` for advanced typed fields. Keep the pending order in the application. Do not mark it paid from the success redirect; wait for a verified webhook or an explicit provider query authorised by application policy.

Authenticated customer checkout is a separate Waffo flow that issues a customer-session token. Do not simulate it by putting identity or tokens into metadata. Implement it only through the reviewed auth endpoint and keep tokens out of logs and URL query strings.
