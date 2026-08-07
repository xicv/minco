use crate::{WaffoClient, WaffoError, identifier::validate_short_id};
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
        if let Some(snapshot) = &self.price_snapshot
            && !valid_amount(&snapshot.amount)
        {
            return Err(WaffoError::InvalidConfiguration(
                "price_snapshot.amount must be a display-format numeric string",
            ));
        }
        if let Some(billing) = &self.billing_detail
            && (billing.country.len() != 2
                || !billing
                    .country
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase()))
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
#[must_use = "checkout builders return an updated value"]
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

    pub const fn with_trial(mut self, enabled: bool) -> Self {
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

    pub const fn expires_in_seconds(mut self, seconds: u64) -> Self {
        self.request.expires_in_seconds = Some(seconds);
        self
    }

    pub const fn dark_mode(mut self, enabled: bool) -> Self {
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

    pub const fn language(mut self, language: CashierLanguage) -> Self {
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
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
    {
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
        assert!(
            CreateCheckoutSessionRequest::new(product_id(), "AUD")
                .validate()
                .is_ok()
        );
        assert!(
            CreateCheckoutSessionRequest::new("PROD_ABC123", "AUD")
                .validate()
                .is_err()
        );
        assert!(
            CreateCheckoutSessionRequest::new(product_id(), "aud")
                .validate()
                .is_err()
        );

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
