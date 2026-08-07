from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path.cwd()
SDK_REVISION = "df098331cf5ea7d43ad79ab223d9eda6d4ac8e5f"
VALID_SUFFIX = "0123456789ABCDEFGHIJKL"
VALID_MERCHANT_ID = f"MER_{VALID_SUFFIX}"
VALID_STORE_ID = f"STO_{VALID_SUFFIX}"
VALID_PRODUCT_ID = f"PROD_{VALID_SUFFIX}"


def read(path: str) -> str:
    return (ROOT / path).read_text()


def write(path: str, content: str) -> None:
    target = ROOT / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(content)


def replace_once(path: str, old: str, new: str) -> None:
    source = read(path)
    count = source.count(old)
    if count != 1:
        raise SystemExit(f"expected one match in {path} for {old!r}; found {count}")
    write(path, source.replace(old, new, 1))


# Advance the post-1.1 source tree to an explicit unpublished 1.2 candidate.
cargo_path = ROOT / "Cargo.toml"
cargo = cargo_path.read_text()
cargo = cargo.replace('[workspace.package]\nversion = "1.1.0"', '[workspace.package]\nversion = "1.2.0"', 1)
pattern = re.compile(
    r'^(?P<name>(?:minco|cargo-minco)[a-z0-9-]*) = \{ version = "1\.1\.0"',
    flags=re.MULTILINE,
)
cargo, dependency_updates = pattern.subn(
    lambda match: f'{match.group("name")} = {{ version = "1.2.0"',
    cargo,
)
if dependency_updates < 30:
    raise SystemExit(f"expected at least 30 Minco dependency version updates; found {dependency_updates}")
cargo_path.write_text(cargo)

compatibility_updates = 0
for base in (ROOT / "plugins", ROOT / "extensions"):
    for path in sorted(base.glob("*/minco-plugin.json")):
        source = path.read_text()
        updated = source.replace('"core_compatibility": "^1.1.0"', '"core_compatibility": "^1.2.0"')
        compatibility_updates += int(updated != source)
        path.write_text(updated)
if compatibility_updates < 10:
    raise SystemExit(f"expected broad archive compatibility updates; found {compatibility_updates}")

# Keep the packaged agent bundle version-matched without changing the settled eight-skill boundary.
agent_root = ROOT / "crates/minco-cli/assets/agent"
for path in sorted(agent_root.rglob("*")):
    if not path.is_file() or path.suffix not in {".json", ".md"}:
        continue
    source = path.read_text()
    path.write_text(source.replace("1.1.0", "1.2.0"))

# Shared exact Waffo short-ID contract.
write(
    "plugins/minco-plugin-payments-waffo/src/identifier.rs",
    '''//! Waffo short-ID validation shared by configuration and checkout contracts.
#![allow(clippy::redundant_pub_crate)]

pub(super) fn validate_short_id(value: &str, prefix: &str) -> Result<(), ()> {
    let Some(suffix) = value
        .strip_prefix(prefix)
        .and_then(|value| value.strip_prefix('_'))
    else {
        return Err(());
    };
    if suffix.len() != 22 || !suffix.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_provider_short_id_shape() {
        assert!(validate_short_id("PROD_0123456789ABCDEFGHIJKL", "PROD").is_ok());
        assert!(validate_short_id("PROD_ABC123", "PROD").is_err());
        assert!(validate_short_id("STO_0123456789ABCDEFGHIJK!", "STO").is_err());
    }
}
''',
)

# Typed, fluent checkout value object inspired by Cashier's concise checkout workflow.
write(
    "plugins/minco-plugin-payments-waffo/src/checkout.rs",
    '''use crate::{WaffoClient, WaffoError, identifier::validate_short_id};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use url::Url;

/// Tax classification accepted by Waffo checkout price snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxCategory {
    DigitalGoods,
    Saas,
    Software,
    Ebook,
    OnlineCourse,
    Consulting,
    ProfessionalService,
}

/// Typed display-price snapshot for a hosted checkout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PriceInfo {
    pub amount: String,
    pub tax_category: TaxCategory,
}

/// Customer billing details optionally used to constrain the checkout market.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BillingDetail {
    pub country: String,
    pub is_business: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postcode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub business_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_id: Option<String>,
}

/// Hosted-checkout language accepted by the reviewed Waffo SDK contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CashierLanguage {
    #[serde(rename = "en")]
    En,
    #[serde(rename = "pt-BR")]
    PtBr,
    #[serde(rename = "es-MX")]
    EsMx,
    #[serde(rename = "id-ID")]
    IdId,
    #[serde(rename = "vi-VN")]
    ViVn,
    #[serde(rename = "ru-RU")]
    RuRu,
    #[serde(rename = "en-KE")]
    EnKe,
    #[serde(rename = "es-PE")]
    EsPe,
    #[serde(rename = "es-CO")]
    EsCo,
    #[serde(rename = "es-CL")]
    EsCl,
    #[serde(rename = "zh-Hant-TW")]
    ZhHantTw,
    #[serde(rename = "zh-Hant-HK")]
    ZhHantHk,
    #[serde(rename = "th-TH")]
    ThTh,
    #[serde(rename = "ja-JP")]
    JaJp,
    #[serde(rename = "en-NG")]
    EnNg,
    #[serde(rename = "ko-KR")]
    KoKr,
    #[serde(rename = "en-HK")]
    EnHk,
    #[serde(rename = "zh-Hans-HK")]
    ZhHansHk,
    #[serde(rename = "pl-PL")]
    PlPl,
    #[serde(rename = "tr-TR")]
    TrTr,
    #[serde(rename = "zh-Hans")]
    ZhHans,
    #[serde(rename = "ms-MY")]
    MsMy,
}

/// Payment method that may be included or excluded from hosted checkout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaymentMethod {
    #[serde(rename = "card")]
    Card,
    #[serde(rename = "applepay")]
    ApplePay,
    #[serde(rename = "googlepay")]
    GooglePay,
    #[serde(rename = "wechat")]
    WeChat,
}

/// Client request for Waffo's hosted checkout-session endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateCheckoutSessionRequest {
    pub product_id: String,
    pub currency: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_snapshot: Option<PriceInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub with_trial: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buyer_email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_detail: Option<BillingDetail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dark_mode: Option<bool>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_merchant_external_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<CashierLanguage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_payment_methods: Option<Vec<PaymentMethod>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_payment_methods: Option<Vec<PaymentMethod>>,
}

impl CreateCheckoutSessionRequest {
    pub fn new(product_id: impl Into<String>, currency: impl Into<String>) -> Self {
        Self {
            product_id: product_id.into(),
            currency: currency.into(),
            price_snapshot: None,
            with_trial: None,
            buyer_email: None,
            billing_detail: None,
            success_url: None,
            expires_in_seconds: None,
            dark_mode: None,
            metadata: BTreeMap::new(),
            order_merchant_external_id: None,
            language: None,
            include_payment_methods: None,
            exclude_payment_methods: None,
        }
    }

    /// Validate the request locally before secret resolution or provider contact.
    pub fn validate(&self) -> Result<(), WaffoError> {
        validate_short_id(&self.product_id, "PROD").map_err(|()| {
            WaffoError::InvalidConfiguration(
                "checkout product_id must be a PROD_ short ID with a 22-character base62 suffix",
            )
        })?;
        if self.currency.len() != 3 || !self.currency.bytes().all(|byte| byte.is_ascii_uppercase())
        {
            return Err(WaffoError::InvalidConfiguration(
                "checkout currency must be a three-letter uppercase ISO 4217 code",
            ));
        }
        if let Some(snapshot) = &self.price_snapshot {
            if !valid_amount(&snapshot.amount) {
                return Err(WaffoError::InvalidConfiguration(
                    "price_snapshot.amount must be a positive display-format numeric string",
                ));
            }
        }
        if let Some(billing) = &self.billing_detail
            && (billing.country.len() != 2
                || !billing.country.bytes().all(|byte| byte.is_ascii_uppercase()))
        {
            return Err(WaffoError::InvalidConfiguration(
                "billing_detail.country must be an uppercase ISO 3166-1 alpha-2 code",
            ));
        }
        if self.include_payment_methods.is_some() && self.exclude_payment_methods.is_some() {
            return Err(WaffoError::InvalidConfiguration(
                "include_payment_methods and exclude_payment_methods are mutually exclusive",
            ));
        }
        if let Some(value) = &self.order_merchant_external_id
            && (value.trim().is_empty()
                || value.chars().count() > 128
                || value.chars().any(char::is_control))
        {
            return Err(WaffoError::InvalidConfiguration(
                "order_merchant_external_id must contain 1-128 printable characters",
            ));
        }
        if self.expires_in_seconds == Some(0) {
            return Err(WaffoError::InvalidConfiguration(
                "expires_in_seconds must be a positive integer",
            ));
        }
        for (key, value) in &self.metadata {
            if key.trim().is_empty()
                || key.chars().any(char::is_control)
                || value.chars().any(char::is_control)
            {
                return Err(WaffoError::InvalidConfiguration(
                    "checkout metadata keys must be non-empty and metadata must not contain control characters",
                ));
            }
        }
        if let Some(success_url) = &self.success_url {
            let url = Url::parse(success_url).map_err(|_| {
                WaffoError::InvalidConfiguration("success_url must be an absolute HTTPS URL")
            })?;
            if url.scheme() != "https"
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(WaffoError::InvalidConfiguration(
                    "success_url must be an absolute HTTPS URL without credentials",
                ));
            }
        }
        Ok(())
    }
}

/// Fluent, serializable checkout intent for the common hosted-checkout path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkout {
    request: CreateCheckoutSessionRequest,
}

impl Checkout {
    /// Start a guest checkout without resolving credentials or contacting Waffo.
    pub fn guest(product_id: impl Into<String>, currency: impl Into<String>) -> Self {
        Self {
            request: CreateCheckoutSessionRequest::new(product_id, currency),
        }
    }

    pub fn return_to(mut self, success_url: impl Into<String>) -> Self {
        self.request.success_url = Some(success_url.into());
        self
    }

    pub fn buyer_email(mut self, buyer_email: impl Into<String>) -> Self {
        self.request.buyer_email = Some(buyer_email.into());
        self
    }

    pub fn with_trial(mut self, enabled: bool) -> Self {
        self.request.with_trial = Some(enabled);
        self
    }

    pub fn price_snapshot(mut self, price: PriceInfo) -> Self {
        self.request.price_snapshot = Some(price);
        self
    }

    pub fn billing_detail(mut self, billing: BillingDetail) -> Self {
        self.request.billing_detail = Some(billing);
        self
    }

    pub fn expires_in_seconds(mut self, seconds: u64) -> Self {
        self.request.expires_in_seconds = Some(seconds);
        self
    }

    pub fn dark_mode(mut self, enabled: bool) -> Self {
        self.request.dark_mode = Some(enabled);
        self
    }

    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.request.metadata.insert(key.into(), value.into());
        self
    }

    pub fn order_reference(mut self, reference: impl Into<String>) -> Self {
        self.request.order_merchant_external_id = Some(reference.into());
        self
    }

    pub fn language(mut self, language: CashierLanguage) -> Self {
        self.request.language = Some(language);
        self
    }

    pub fn include_payment_methods(
        mut self,
        methods: impl IntoIterator<Item = PaymentMethod>,
    ) -> Self {
        self.request.include_payment_methods = Some(methods.into_iter().collect());
        self
    }

    pub fn exclude_payment_methods(
        mut self,
        methods: impl IntoIterator<Item = PaymentMethod>,
    ) -> Self {
        self.request.exclude_payment_methods = Some(methods.into_iter().collect());
        self
    }

    /// Return the validated provider request without resolving secrets or performing I/O.
    pub fn build(self) -> Result<CreateCheckoutSessionRequest, WaffoError> {
        self.request.validate()?;
        Ok(self.request)
    }

    /// Validate and create the checkout through an explicitly constructed client.
    pub async fn create(
        self,
        client: &WaffoClient,
        idempotency_key: &str,
    ) -> Result<CheckoutSession, WaffoError> {
        let request = self.build()?;
        client
            .create_checkout_session(&request, idempotency_key)
            .await
    }
}

/// Hosted checkout-session details returned by Waffo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutSession {
    pub session_id: String,
    pub checkout_url: String,
    pub expires_at: String,
}

/// HTTP webhook record returned by Waffo's store endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaffoWebhook {
    pub id: String,
    pub store_id: String,
    pub channel: String,
    pub url: String,
    #[serde(default)]
    pub events: Vec<String>,
    pub test_mode: bool,
    pub created_at: String,
    pub updated_at: String,
}

fn valid_amount(value: &str) -> bool {
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if parts.next().is_some() || whole.is_empty() || !whole.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    match fraction {
        None => true,
        Some(value) => !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn product_id() -> &'static str {
        "PROD_0123456789ABCDEFGHIJKL"
    }

    #[test]
    fn fluent_checkout_builds_the_provider_contract_without_io() {
        let request = Checkout::guest(product_id(), "AUD")
            .buyer_email("buyer@example.com")
            .return_to("https://example.com/billing/complete?source=checkout")
            .order_reference("order-42")
            .metadata("cart_id", "cart-7")
            .language(CashierLanguage::En)
            .include_payment_methods([PaymentMethod::Card, PaymentMethod::ApplePay])
            .build()
            .unwrap();

        assert_eq!(
            serde_json::to_value(request).unwrap(),
            json!({
                "productId": product_id(),
                "currency": "AUD",
                "buyerEmail": "buyer@example.com",
                "successUrl": "https://example.com/billing/complete?source=checkout",
                "metadata": {"cart_id": "cart-7"},
                "orderMerchantExternalId": "order-42",
                "language": "en",
                "includePaymentMethods": ["card", "applepay"]
            })
        );
    }

    #[test]
    fn checkout_contract_rejects_invalid_or_ambiguous_values() {
        assert!(CreateCheckoutSessionRequest::new(product_id(), "AUD").validate().is_ok());
        assert!(CreateCheckoutSessionRequest::new("PROD_ABC123", "AUD").validate().is_err());
        assert!(CreateCheckoutSessionRequest::new(product_id(), "aud").validate().is_err());

        let mut invalid = CreateCheckoutSessionRequest::new(product_id(), "AUD");
        invalid.expires_in_seconds = Some(0);
        assert!(invalid.validate().is_err());

        let mut valid = CreateCheckoutSessionRequest::new(product_id(), "AUD");
        valid.expires_in_seconds = Some(604_801);
        assert!(valid.validate().is_ok());

        let mut invalid = CreateCheckoutSessionRequest::new(product_id(), "AUD");
        invalid.include_payment_methods = Some(vec![PaymentMethod::Card]);
        invalid.exclude_payment_methods = Some(vec![PaymentMethod::WeChat]);
        assert!(invalid.validate().is_err());
    }
}
''',
)

# Library exports and reviewed provider-contract marker.
lib_path = "plugins/minco-plugin-payments-waffo/src/lib.rs"
lib = read(lib_path)
lib = lib.replace("mod graphql;\n", "mod graphql;\nmod identifier;\n", 1)
lib = lib.replace(
    "pub use checkout::{CheckoutSession, CreateCheckoutSessionRequest, WaffoWebhook};",
    "pub use checkout::{\n    BillingDetail, CashierLanguage, Checkout, CheckoutSession, CreateCheckoutSessionRequest,\n    PaymentMethod, PriceInfo, TaxCategory, WaffoWebhook,\n};",
    1,
)
marker = "pub(crate) use config::RawWaffoConfiguration;\n"
if marker not in lib:
    raise SystemExit("could not locate Waffo library marker")
lib = lib.replace(
    marker,
    f'''/// Exact official Waffo Go SDK revision reviewed for this provider contract.\npub const REVIEWED_WAFFO_SDK_REVISION: &str = "{SDK_REVISION}";\n\n{marker}''',
    1,
)
write(lib_path, lib)

# Configuration uses the same exact short-ID contract as checkout.
config_path = "plugins/minco-plugin-payments-waffo/src/config.rs"
config = read(config_path)
config = config.replace("use crate::WaffoError;", "use crate::{WaffoError, identifier::validate_short_id};", 1)
config = config.replace('validate_short_id(&raw.merchant_id, "MER_")', 'validate_short_id(&raw.merchant_id, "MER")')
config = config.replace('validate_short_id(store_id, "STO_")', 'validate_short_id(store_id, "STO")')
old_validator = '''fn validate_short_id(value: &str, prefix: &str) -> Result<(), ()> {
    let Some(suffix) = value.strip_prefix(prefix) else {
        return Err(());
    };
    if suffix.is_empty()
        || value.len() > 128
        || !suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(());
    }
    Ok(())
}

'''
if old_validator not in config:
    raise SystemExit("could not locate duplicate Waffo short-ID validator")
config = config.replace(old_validator, "", 1)
write(config_path, config)

# Runtime descriptor follows the lock-step package version.
replace_once(
    "plugins/minco-plugin-payments-waffo/src/plugin.rs",
    'VersionReq::parse("^1.1.0").expect("static core compatibility is valid");',
    'VersionReq::parse(concat!("^", env!("CARGO_PKG_VERSION")))\n                .expect("static core compatibility is valid");',
)

# Replace obsolete shortened provider IDs in source tests and examples.
replacements = {
    "MER_ABC123": VALID_MERCHANT_ID,
    "MER_TEST123": VALID_MERCHANT_ID,
    "STO_ABC123": VALID_STORE_ID,
    "PROD_ABC123": VALID_PRODUCT_ID,
}
plugin_root = ROOT / "plugins/minco-plugin-payments-waffo"
for path in sorted(plugin_root.rglob("*")):
    if not path.is_file() or path.suffix not in {".rs", ".md", ".json", ".toml"}:
        continue
    source = path.read_text()
    for old, new in replacements.items():
        source = source.replace(old, new)
    path.write_text(source)

# One-command common checkout CLI plus fixed provider-contract reporting.
cli_path = "plugins/minco-plugin-payments-waffo/src/bin/minco-waffo.rs"
cli = read(cli_path)
cli = cli.replace(
    '''use minco_plugin_payments_waffo::{
    CreateCheckoutSessionRequest, SecretResolver, SecretValue, WaffoConfiguration, WaffoError,
    WaffoPlugin, WaffoService,
};''',
    '''use minco_plugin_payments_waffo::{
    Checkout, CreateCheckoutSessionRequest, REVIEWED_WAFFO_SDK_REVISION, SecretResolver,
    SecretValue, WaffoConfiguration, WaffoError, WaffoPlugin, WaffoService,
};''',
    1,
)
checkout_command = '''    /// Create a common hosted checkout directly from command-line flags.
    Checkout {
        #[arg(long)]
        product_id: String,
        #[arg(long, default_value = "AUD")]
        currency: String,
        #[arg(long)]
        return_to: Option<String>,
        #[arg(long)]
        buyer_email: Option<String>,
        #[arg(long)]
        order_reference: Option<String>,
        #[arg(long = "metadata", value_name = "KEY=VALUE", value_parser = parse_metadata_entry)]
        metadata: Vec<(String, String)>,
        #[arg(long)]
        idempotency_key: String,
    },
'''
anchor = "    /// Create a hosted checkout session from a typed JSON request file or stdin.\n    CheckoutCreate {"
if anchor not in cli:
    raise SystemExit("could not locate checkout CLI enum anchor")
cli = cli.replace(anchor, checkout_command + anchor, 1)
parse_helper = '''fn parse_metadata_entry(value: &str) -> std::result::Result<(String, String), String> {
    let Some((key, value)) = value.split_once('=') else {
        return Err("metadata must use KEY=VALUE".into());
    };
    if key.trim().is_empty()
        || key.chars().any(char::is_control)
        || value.chars().any(char::is_control)
    {
        return Err("metadata keys must be non-empty and metadata must not contain control characters".into());
    }
    Ok((key.to_owned(), value.to_owned()))
}

'''
loaded_anchor = "#[derive(Debug)]\nstruct LoadedConfiguration {"
if loaded_anchor not in cli:
    raise SystemExit("could not locate CLI loaded configuration anchor")
cli = cli.replace(loaded_anchor, parse_helper + loaded_anchor, 1)
cli = cli.replace(
    "    configuration_digest: String,\n",
    "    configuration_digest: String,\n    provider_contract_revision: &'static str,\n",
    1,
)
cli = cli.replace(
    "                configuration_digest: loaded.digest.clone(),\n",
    "                configuration_digest: loaded.digest.clone(),\n                provider_contract_revision: REVIEWED_WAFFO_SDK_REVISION,\n",
    1,
)
checkout_arm = '''        Command::Checkout {
            product_id,
            currency,
            return_to,
            buyer_email,
            order_reference,
            metadata,
            idempotency_key,
        } => {
            let mut checkout = Checkout::guest(product_id, currency);
            if let Some(value) = return_to {
                checkout = checkout.return_to(value);
            }
            if let Some(value) = buyer_email {
                checkout = checkout.buyer_email(value);
            }
            if let Some(value) = order_reference {
                checkout = checkout.order_reference(value);
            }
            for (key, value) in metadata {
                checkout = checkout.metadata(key, value);
            }
            let request = checkout.build()?;
            let client = loaded.service.client(&resolver).await?;
            let result = client
                .create_checkout_session(&request, &idempotency_key)
                .await?;
            emit("checkout", result, cli.compact)
        }
'''
match_anchor = "        Command::CheckoutCreate {\n"
if match_anchor not in cli:
    raise SystemExit("could not locate checkout CLI match anchor")
cli = cli.replace(match_anchor, checkout_arm + match_anchor, 1)
cli = cli.replace(
    '''            let client = loaded.service.client(&resolver).await?;
            let result = client
                .create_checkout_session(&request, &idempotency_key)
                .await?;''',
    '''            request.validate()?;
            let client = loaded.service.client(&resolver).await?;
            let result = client
                .create_checkout_session(&request, &idempotency_key)
                .await?;''',
    1,
)
write(cli_path, cli)

# Package the plugin-local agent guidance without expanding the global eight-skill bundle.
replace_once(
    "plugins/minco-plugin-payments-waffo/Cargo.toml",
    '  "README.md",\n',
    '  "README.md",\n  "agent/**",\n',
)

write(
    "plugins/minco-plugin-payments-waffo/agent/skills/minco-waffo-payments/SKILL.md",
    f'''---
name: minco-waffo-payments
description: Build and review Minco Waffo Pancake checkout, signed actions, GraphQL queries, and verified webhooks. Use when a Minco application enables payments-waffo or invokes minco-waffo.
---

# Minco Waffo payments

Use this skill only after reading the application's `AGENTS.md`, Minco graph, and the
`payments-waffo` configuration. The provider contract was reviewed against Waffo Go SDK
revision `{SDK_REVISION}`.

## Route the task

- Common hosted checkout: read `references/checkout.md`.
- Webhook registration, verification, projection, or replay safety: read `references/webhooks.md`.
- Tests, fixtures, failure simulation, or provider-contract review: read `references/testing.md`.

## Invariants

1. The application owns orders, entitlements, subscriptions, and transaction projections.
2. A checkout redirect is not proof of payment; verified webhook state is authoritative.
3. Use `orderMerchantExternalId` or string metadata to correlate provider events with an
   application-owned identifier.
4. Never place private keys or webhook public-key values in TOML, arguments, logs, prompts,
   receipts, or generated plans. Use `env:` locally and exact `ssm:` references in AWS.
5. Reuse the same provider idempotency key for an explicit retry of the same request.
6. Do not infer production readiness from offline conformance or a successful test checkout.
7. Do not add ORM models, global billing state, hidden polling, or scheduled reconciliation to
   the plugin. Add application-owned ports and explicit workers only when the product requires them.

## Start with machine-readable checks

```bash
minco-waffo --config minco.waffo.toml config-check
minco-waffo --config minco.waffo.toml doctor
minco-waffo idempotency-key
```

`config-check` resolves no secrets. `doctor` parses configured keys but contacts no Waffo API.
''',
)

write(
    "plugins/minco-plugin-payments-waffo/agent/skills/minco-waffo-payments/references/checkout.md",
    f'''# Hosted checkout workflow

Prefer the fluent Rust value object or the direct CLI for the common guest-checkout path.
Both validate the typed request before provider contact.

```rust,no_run
use minco_plugin_payments_waffo::{{Checkout, EnvironmentSecretResolver}};

# async fn example(service: &minco_plugin_payments_waffo::WaffoService) -> Result<(), Box<dyn std::error::Error>> {{
let client = service.client(&EnvironmentSecretResolver).await?;
let session = Checkout::guest("{VALID_PRODUCT_ID}", "AUD")
    .buyer_email("buyer@example.com")
    .return_to("https://example.com/billing/complete")
    .order_reference("order-42")
    .metadata("cart_id", "cart-7")
    .create(&client, "checkout_order_42")
    .await?;
println!("{{}}", session.checkout_url);
# Ok(())
# }}
```

```bash
minco-waffo --config minco.waffo.toml checkout \\
  --product-id {VALID_PRODUCT_ID} \\
  --currency AUD \\
  --buyer-email buyer@example.com \\
  --return-to https://example.com/billing/complete \\
  --order-reference order-42 \\
  --metadata cart_id=cart-7 \\
  --idempotency-key checkout_order_42
```

Use `checkout-create --body FILE` for advanced typed fields. Keep the pending order in the
application. Do not mark it paid from the success redirect; wait for a verified webhook or an
explicit provider query authorised by application policy.

Authenticated customer checkout is a separate Waffo flow that issues a customer-session token.
Do not simulate it by putting identity or tokens into metadata. Implement it only through the
reviewed auth endpoint and keep tokens out of logs and URL query strings.
''',
)

write(
    "plugins/minco-plugin-payments-waffo/agent/skills/minco-waffo-payments/references/webhooks.md",
    '''# Webhook workflow

Treat webhook handling as an explicit application slice:

1. Preserve the exact request bytes and signature header.
2. Enforce the configured body-size bound.
3. Verify the signature and timestamp before JSON decoding.
4. Reject the wrong Waffo environment and expected store.
5. Atomically claim the delivery/event deduplication key in durable application storage.
6. Apply an idempotent application-owned projection or domain command.
7. Record received, handled, ignored, retryable-failure, and terminal-failure outcomes.
8. Return success only after the chosen durability boundary is satisfied.

The plugin verifies provider authenticity; it must not become the owner of product-specific
subscription tables, access rules, invoices, or order state. Typed application events are useful,
but they follow successful verification and durable deduplication.

Never parse and re-serialize the body before signature verification. Never use a checkout return
URL as the authoritative payment signal.
''',
)

write(
    "plugins/minco-plugin-payments-waffo/agent/skills/minco-waffo-payments/references/testing.md",
    '''# Testing payments safely

Keep the default test suite offline.

- Build checkout requests and assert exact serialized JSON.
- Use exact 22-character Waffo short-ID fixtures.
- Generate ephemeral RSA keys for signing and webhook tests.
- Verify tampering, stale timestamps, environment mismatch, store mismatch, duplicate delivery,
  and same-idempotency-key/different-body conflicts.
- Exercise `config-check`, `doctor`, and command help without contacting Waffo.
- Test application webhook projections against fake ports and durable idempotency-store behavior.
- Separate provider sandbox evidence from local conformance and production evidence.

A future transport test double should queue endpoint-specific responses and retain redacted
requests for assertions, analogous to a billing fake, without adding a global singleton or
allowing unregistered network requests.
''',
)

# Plugin documentation now presents the streamlined path first and keeps application state explicit.
write(
    "plugins/minco-plugin-payments-waffo/README.md",
    f'''# Minco Waffo Pancake payments plugin

`minco-plugin-payments-waffo` is an opt-in Minco integration for Waffo Pancake hosted checkout,
signed server actions, read-only GraphQL queries, and standard HTTP webhooks. It is part of the
unpublished Minco `1.2.0` candidate and remains beta.

The provider contract in this source tree was reviewed against the official Waffo Go SDK at
`{SDK_REVISION}`. Provider review, source qualification, sandbox evidence, publication, deployment,
and production readiness remain separate states.

## Design

- plugin discovery and installation are deterministic and make no network calls;
- private and public keys remain opaque `env:` or `ssm:` references until an explicit operation;
- writes require a caller-supplied Waffo idempotency key and Minco's idempotency capability;
- production writes require matching production environments and the persisted production guard;
- request, response, and webhook bodies are bounded;
- webhook signatures are verified against untouched raw bytes before JSON decoding;
- no hidden retries, timers, queues, databases, ORM models, or fixed-capacity infrastructure are introduced;
- applications own orders, entitlements, subscriptions, invoices, transaction projections, and access policy.

## Add the plugin

During candidate development, use the workspace or Git dependency. After an authorised lock-step
release, applications can enable the facade feature `plugin-payments-waffo`.

```bash
cargo minco plugin add payments-waffo
```

The plugin depends on Minco's official idempotency plugin and remains disabled by default.

## Configuration

Create `minco.waffo.toml`:

```toml
schema = 1
environment_class = "test"

[values.plugins.payments-waffo]
environment = "test"
merchant_id = "{VALID_MERCHANT_ID}"
private_key = "env:WAFFO_PRIVATE_KEY"

# Configure all webhook fields together only when registration/verification is needed.
# webhook_public_key = "ssm:/my-app/test/waffo-webhook-public-key"
# store_id = "{VALID_STORE_ID}"
# webhook_url = "https://api.example.com/webhooks/waffo"
# webhook_events = ["order.completed", "subscription.payment_succeeded"]
```

Secret values never belong in this file. Local development can use `env:NAME`. Lambda deployments
should use an exact `ssm:/absolute/name` reference with least-privilege `ssm:GetParameter` access.

## Fluent checkout

The common path is a pure fluent value object followed by one explicit provider call:

```rust,no_run
use minco_plugin_payments_waffo::{{Checkout, EnvironmentSecretResolver}};

# async fn example(service: &minco_plugin_payments_waffo::WaffoService) -> Result<(), Box<dyn std::error::Error>> {{
let client = service.client(&EnvironmentSecretResolver).await?;
let session = Checkout::guest("{VALID_PRODUCT_ID}", "AUD")
    .buyer_email("buyer@example.com")
    .return_to("https://example.com/billing/complete")
    .order_reference("order-42")
    .metadata("cart_id", "cart-7")
    .create(&client, "checkout_order_42")
    .await?;
println!("{{}}", session.checkout_url);
# Ok(())
# }}
```

The builder is serializable and validates exact Waffo identifiers, typed price/billing structures,
payment methods, languages, metadata, currency, and HTTPS return URLs before provider contact.

## Command-line automation

The direct checkout command covers the common workflow without a hand-authored JSON body:

```bash
minco-waffo --config minco.waffo.toml checkout \\
  --product-id {VALID_PRODUCT_ID} \\
  --currency AUD \\
  --buyer-email buyer@example.com \\
  --return-to https://example.com/billing/complete \\
  --order-reference order-42 \\
  --metadata cart_id=cart-7 \\
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

Every successful command emits a versioned JSON envelope. `config-check` resolves no secrets;
`doctor` resolves/parses configured keys but does not contact Waffo. The CLI's Minco idempotency
store is process-local, while the explicit Waffo key is reused across an intentional retry.

## Webhook authority

A successful checkout return is user experience, not payment authority. Preserve the exact webhook
body, verify it, durably claim the emitted deduplication key, then apply an application-owned
projection or domain command. Keep received/handled/failure outcomes inspectable. The plugin does
not create generic subscription or transaction tables because those models and retention rules are
product policy.

## Agent guidance

Plugin-focused Codex guidance is packaged under:

```text
agent/skills/minco-waffo-payments/SKILL.md
```

It routes checkout, webhook, and testing work without expanding Minco's global eight-skill bundle.
''',
)

# Current release truth and candidate documentation.
truth_path = "verification/repository-truth.toml"
truth = read(truth_path)
truth = truth.replace('workspace_version = "1.1.0"', 'workspace_version = "1.2.0"', 1)
truth = truth.replace('workspace_release_state = "published"', 'workspace_release_state = "candidate"', 1)
truth = truth.replace('publishable_package_count = 33', 'publishable_package_count = 34', 1)
truth = truth.replace('new_publishable_packages = []', 'new_publishable_packages = ["minco-plugin-payments-waffo"]', 1)
truth = truth.replace('release_candidate_task = "M14-T01"', 'release_candidate_task = "M14-T03"', 1)
truth = truth.replace(
    '# The published 1.1.0 baseline contains the complete 33-package family from\n# immutable tag v1.1.0. The agent-native CLI and skills advance in lock-step\n# with the existing realtime, lifecycle, ProjectView/MCP/workbench, and\n# explicit DynamoDB boundaries.\n',
    '# The immutable published 1.1.0 baseline contains 33 packages. The source\n# workspace is an unpublished 1.2.0 candidate with one new publishable package:\n# minco-plugin-payments-waffo. Publication and production evidence are separate.\n',
    1,
)
write(truth_path, truth)

root_readme = read("README.md")
root_readme = root_readme.replace('> Current workspace version: `1.1.0`', '> Current workspace version: `1.2.0`', 1)
root_readme = root_readme.replace('> Workspace release state: `published`', '> Workspace release state: `candidate`', 1)
root_readme = root_readme.replace('> Current publishable package count: `33`', '> Current publishable package count: `34`', 1)
write("README.md", root_readme)

write(
    "PUBLISHING.md",
    '''# Publishing Minco

The authoritative crate-family release procedure is
[`docs/development/publishing.md`](docs/development/publishing.md).

The immutable published boundary is the complete 33-package lock-step `1.1.0`
family from tag `v1.1.0` at `4d81543f7c5adb773655f23278abfe084de9f3e0`.
The source workspace is now an **unpublished 34-package `1.2.0` candidate** that
adds `minco-plugin-payments-waffo`. Source qualification, merge, tag, upload,
registry verification, docs.rs, documentation deployment, provider sandbox
proof, application deployment, and production readiness remain separate states.

The safe default performs no upload:

```bash
uv sync --locked --only-dev
uv run --locked python scripts/validate_publish.py
scripts/release/publish.sh
```

The Waffo package is a first-publication candidate. Before any irreversible
release, independently verify package ownership/bootstrap policy, exact tag
identity, OIDC authentication, registry absence for every exact `1.2.0`
package, package order, and the complete 34-package post-upload complement.

The irreversible upload requires a clean, correctly tagged release and an
explicit flag:

```bash
scripts/release/publish.sh --execute
```

Never use `--allow-dirty` or `--no-verify` for a Minco release. This task does
not authorise a tag, crates.io upload, GitHub release, documentation promotion,
AWS mutation, or live Waffo payment operation.
''',
)

write(
    "CODEX_HANDOFF.md",
    f'''# Minco 1.2.0 Waffo candidate handoff

Date: 2026-08-07
Published baseline: `1.1.0`
Current workspace version: `1.2.0`
Workspace release state: `candidate`
Published `1.1.0` source: `4d81543f7c5adb773655f23278abfe084de9f3e0`
Active task: `M14-T03`
Provider contract reviewed: Waffo Go SDK `{SDK_REVISION}`

## Closed release boundary

Minco `1.1.0` remains the immutable, published 33-package family. No part of
this candidate changes that historical tag, registry state, documentation
release, or retained evidence.

## Candidate boundary

The source workspace is an unpublished 34-package `1.2.0` candidate adding the
opt-in beta `minco-plugin-payments-waffo`. The plugin provides typed/fluent hosted
checkout, signed actions, read-only GraphQL, raw-body webhook verification, a
config-driven CLI, and plugin-local agent guidance. It introduces no default
feature, ORM, hidden queue, schedule, fixed compute, database, or provider
contact during composition.

The common checkout flow is streamlined, but application-owned orders,
entitlements, subscription/transaction projections, webhook processing outcomes,
and durable deduplication remain outside the provider plugin.

## Evidence boundary

Previous Waffo run `31069913728` is historical evidence for an earlier branch
head. Re-run focused and repository gates against the current exact head before
marking PR #125 ready. No live Waffo credential, payment, AWS apply, deployment,
tag, release, or registry publication is authorised by this handoff.

## Recovery

```bash
cd /Users/xicao/Projects/minco
git fetch --all --tags --prune
jj git import
jj workspace list
```

Use one isolated JJ workspace for M14-T03. Preserve unrelated work and use Git
only for GitHub transport.
''',
)

changelog = read("CHANGELOG.md")
release_notes = f'''## [1.2.0] - 2026-08-07

The complete 34-package workspace is an unpublished candidate. The published
baseline remains immutable `1.1.0`; source qualification, merge, tag, registry
publication, documentation, provider sandbox evidence, deployment, and
production readiness are independent states.

### Added

- Added the opt-in beta `minco-plugin-payments-waffo` package with signed Waffo
  actions, typed hosted checkout, read-only GraphQL, bounded responses, explicit
  idempotency, and raw-body RSA webhook verification.
- Added a Cashier-inspired fluent guest-checkout value object and direct
  `minco-waffo checkout` command for the common product/currency/return/order
  correlation path while retaining the advanced JSON command.
- Added plugin-local agent guidance for checkout, webhook projection, and
  offline testing without expanding Minco's global agent-skill bundle.

### Changed

- Advanced the lock-step source workspace and archive-visible official
  compatibility ranges from `1.1.0` to the unpublished `1.2.0` candidate.
- Aligned Waffo merchant, store, and product IDs with the exact reviewed
  22-character base62 short-ID contract and replaced untyped common checkout
  fields with explicit Rust types.

### Safety boundary

- Checkout return URLs remain a user-experience signal, not payment authority.
  Applications own orders, entitlements, subscriptions, transactions, durable
  deduplication, and webhook processing outcomes.
- No live Waffo request, AWS mutation, deployment, tag, GitHub release, docs
  promotion, or crates.io upload is part of this candidate task.

'''
anchor = "## [1.1.0] - 2026-08-06\n"
if anchor not in changelog:
    raise SystemExit("could not locate 1.1 changelog anchor")
changelog = changelog.replace(anchor, release_notes + anchor, 1)
write("CHANGELOG.md", changelog)

write(
    "docs/adoption/1.1.0-to-1.2.0.md",
    '''# Upgrade from Minco 1.1.0 to the 1.2.0 candidate

Published baseline: `1.1.0`
Candidate workspace version: `1.2.0`
Candidate publication status: `unpublished`

## Scope

The candidate adds one opt-in beta package, `minco-plugin-payments-waffo`, and
does not change Minco's default feature set. Existing applications that do not
enable the new feature retain their 1.1 architecture and runtime behavior.

Applications adopting Waffo should:

1. enable `plugin-payments-waffo` explicitly;
2. keep RSA material behind `env:` or exact `ssm:` references;
3. use exact Waffo short IDs;
4. create application-owned pending orders before checkout;
5. correlate with `orderMerchantExternalId` or string metadata;
6. make verified, durably deduplicated webhooks authoritative for payment state;
7. keep provider sandbox, deployment, and production evidence separate.

The candidate does not provide generic billable ORM models, subscription tables,
access middleware, invoice storage, or scheduled reconciliation. Those remain
application policy expressed through normal Minco ports, adapters, operations,
and explicit workers.
''',
)

write(
    "tasks/M14/M14-T03-waffo-payments-plugin.md",
    f'''# M14-T03 — Waffo Pancake payments plugin

Status: in_progress
Milestone: M14
Owner: framework
Provider review: `{SDK_REVISION}`

## Goal

Ship an opt-in Waffo payment integration that preserves Minco's zero-idle-cost,
contract-first, static-plugin, AWS-native, and agent-automatable boundaries.

## Implemented

- [x] Signed provider actions, hosted checkout, read-only GraphQL, and raw-body webhook verification.
- [x] Typed configuration with unresolved `env:` / `ssm:` secret references.
- [x] Explicit provider and Minco idempotency claims for mutating actions.
- [x] Bounded bodies, environment guards, production-write guard, and no hidden retries.
- [x] Dedicated stable-JSON CLI.
- [x] Cashier-inspired fluent guest checkout and direct checkout CLI.
- [x] Exact Waffo short-ID validation and typed common checkout fields.
- [x] Plugin-local checkout/webhook/testing skill for coding agents.
- [x] Rebased candidate narrative onto immutable Minco 1.1.0 and opened the 1.2.0 release line.

## Required before ready for review

- [ ] Bind every application client/verifier construction path to Minco `EnvironmentClass` before secret resolution.
- [ ] Bind webhook verification and dedupe scopes to the configured store and mode.
- [ ] Preserve complete ordered provider warnings/errors, including untrusted AI hints and GraphQL locations/path.
- [ ] Scope local idempotency by provider environment and canonical API origin.
- [ ] Replace the handwritten GraphQL scanner with a maintained parser.
- [ ] Restrict/canonicalize generic production actions and return JSON for normal CLI parse failures.
- [ ] Add an injectable no-network transport fake and endpoint/request assertions.
- [ ] Add Waffo authenticated checkout through the reviewed customer-session-token endpoint.
- [ ] Pass exact-head focused, static, publication, generated-reference, and source-manifest gates.

## Evidence

Historical run `31069913728` passed the earlier 16-test implementation. It is not
exact-head evidence for this candidate. No live Waffo credentials or mutations are
used by automated qualification.
''',
)

# Permanent bounded PR qualification; the bootstrap workflow removes itself.
write(
    ".github/workflows/waffo-payments.yml",
    f'''name: Waffo payments focused qualification

on:
  pull_request:
    paths:
      - "Cargo.toml"
      - "Cargo.lock"
      - "crates/minco/**"
      - "crates/minco-cli/assets/agent/**"
      - "plugins/catalog.toml"
      - "plugins/minco-plugin-payments-waffo/**"
      - "verification/repository-truth.toml"
      - "docs/adoption/1.1.0-to-1.2.0.md"
      - ".github/workflows/waffo-payments.yml"

permissions:
  contents: read

concurrency:
  group: waffo-payments-${{{{ github.event.pull_request.number }}}}-${{{{ github.sha }}}}
  cancel-in-progress: true

jobs:
  focused:
    runs-on: ubuntu-latest
    timeout-minutes: 45
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
      - uses: astral-sh/setup-uv@c771a70e6277c0a99b617c7a806ffedaca235ff9 # v9.0.0
        with:
          version: "0.11.32"
          enable-cache: false
      - run: uv sync --locked --only-dev
      - uses: dtolnay/rust-toolchain@2c7215f132e9ebf062739d9130488b56d53c060c
        with:
          toolchain: 1.97.1
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@c19371144df3bb44fab255c43d04cbc2ab54d1c4 # v2.9.1
        with:
          cache-targets: false
          cache-on-failure: false
      - name: Verify formatting only for Waffo-touched Rust
        run: |
          rustfmt --edition 2024 --check \\
            plugins/minco-plugin-payments-waffo/src/lib.rs \\
            plugins/minco-plugin-payments-waffo/src/identifier.rs \\
            plugins/minco-plugin-payments-waffo/src/checkout.rs \\
            plugins/minco-plugin-payments-waffo/src/config.rs \\
            plugins/minco-plugin-payments-waffo/src/plugin.rs \\
            plugins/minco-plugin-payments-waffo/src/bin/minco-waffo.rs
      - name: Test the plugin and CLI targets
        run: cargo test -p minco-plugin-payments-waffo --all-features --locked
      - name: Lint only the payment plugin
        run: cargo clippy -p minco-plugin-payments-waffo --all-targets --all-features --locked -- -D warnings
      - name: Check opt-in facade composition
        run: cargo check -p minco --no-default-features --features plugin-payments-waffo --locked
      - name: Smoke machine-readable common commands
        run: |
          cargo run --quiet -p minco-plugin-payments-waffo --features cli --bin minco-waffo -- idempotency-key > /tmp/waffo-key.json
          python3 - <<'PY'
          import json
          from pathlib import Path
          value = json.loads(Path('/tmp/waffo-key.json').read_text())
          assert value['schema'] == 1 and value['ok'] is True
          PY
          cat > /tmp/minco.waffo.toml <<'TOML'
          schema = 1
          environment_class = "test"

          [values.plugins.payments-waffo]
          environment = "test"
          merchant_id = "{VALID_MERCHANT_ID}"
          private_key = "env:WAFFO_PRIVATE_KEY"
          TOML
          cargo run --quiet -p minco-plugin-payments-waffo --features cli --bin minco-waffo -- \\
            --config /tmp/minco.waffo.toml config-check > /tmp/waffo-config.json
          python3 - <<'PY'
          import json
          from pathlib import Path
          output = Path('/tmp/waffo-config.json').read_text()
          value = json.loads(output)
          assert value['schema'] == 1 and value['ok'] is True
          assert value['data']['providerContractRevision'] == '{SDK_REVISION}'
          assert 'WAFFO_PRIVATE_KEY' not in output
          PY
          cargo run --quiet -p minco-plugin-payments-waffo --features cli --bin minco-waffo -- checkout --help >/dev/null
      - name: Validate repository and package metadata
        run: |
          uv run --locked python scripts/validate_static.py --output /tmp/static-validation.json
          uv run --locked python scripts/validate_publish.py --output /tmp/publish-validation.json
          scripts/docs/generate-reference.sh --check
          python3 scripts/source_manifest.py --check
''',
)

print(json.dumps({
    "workspace_version": "1.2.0",
    "provider_revision": SDK_REVISION,
    "merchant_fixture": VALID_MERCHANT_ID,
    "product_fixture": VALID_PRODUCT_ID,
}, indent=2))
