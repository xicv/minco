use crate::WaffoError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

/// Client request for Waffo's hosted checkout-session endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateCheckoutSessionRequest {
    pub product_id: String,
    pub currency: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price_snapshot: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub with_trial: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buyer_email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_detail: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dark_mode: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_merchant_external_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_payment_methods: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_payment_methods: Option<Vec<String>>,
}

impl CreateCheckoutSessionRequest {
    pub(super) fn validate(&self) -> Result<(), WaffoError> {
        validate_short_id(&self.product_id, "PROD_").map_err(|()| {
            WaffoError::InvalidConfiguration("checkout product_id must be a PROD_ short ID")
        })?;
        if self.currency.len() != 3 || !self.currency.bytes().all(|byte| byte.is_ascii_uppercase())
        {
            return Err(WaffoError::InvalidConfiguration(
                "checkout currency must be a three-letter uppercase ISO 4217 code",
            ));
        }
        for value in [
            self.price_snapshot.as_ref(),
            self.billing_detail.as_ref(),
            self.metadata.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            if !value.is_object() {
                return Err(WaffoError::InvalidConfiguration(
                    "checkout structured fields must be JSON objects",
                ));
            }
        }
        if self.include_payment_methods.is_some() && self.exclude_payment_methods.is_some() {
            return Err(WaffoError::InvalidConfiguration(
                "include_payment_methods and exclude_payment_methods are mutually exclusive",
            ));
        }
        for method in self
            .include_payment_methods
            .iter()
            .chain(&self.exclude_payment_methods)
            .flatten()
        {
            if !matches!(
                method.as_str(),
                "card" | "applepay" | "googlepay" | "wechat"
            ) {
                return Err(WaffoError::InvalidConfiguration(
                    "unsupported checkout payment method",
                ));
            }
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
        if self
            .expires_in_seconds
            .is_some_and(|seconds| !(1..=604_800).contains(&seconds))
        {
            return Err(WaffoError::InvalidConfiguration(
                "expires_in_seconds must be between 1 second and 7 days",
            ));
        }
        if self.language.as_deref().is_some_and(|language| {
            !matches!(
                language,
                "en" | "pt-BR"
                    | "es-MX"
                    | "id-ID"
                    | "vi-VN"
                    | "ru-RU"
                    | "en-KE"
                    | "es-PE"
                    | "es-CO"
                    | "es-CL"
                    | "zh-Hant-TW"
                    | "zh-Hant-HK"
                    | "th-TH"
                    | "ja-JP"
                    | "en-NG"
                    | "ko-KR"
                    | "en-HK"
                    | "zh-Hans-HK"
                    | "pl-PL"
                    | "tr-TR"
                    | "zh-Hans"
                    | "ms-MY"
            )
        }) {
            return Err(WaffoError::InvalidConfiguration(
                "unsupported checkout language",
            ));
        }
        if let Some(success_url) = &self.success_url {
            let url = Url::parse(success_url).map_err(|_| {
                WaffoError::InvalidConfiguration("success_url must be an absolute HTTPS URL")
            })?;
            if url.scheme() != "https" || url.host_str().is_none() {
                return Err(WaffoError::InvalidConfiguration(
                    "success_url must be an absolute HTTPS URL",
                ));
            }
        }
        Ok(())
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

fn validate_short_id(value: &str, prefix: &str) -> Result<(), ()> {
    let Some(suffix) = value.strip_prefix(prefix) else {
        return Err(());
    };
    if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> CreateCheckoutSessionRequest {
        CreateCheckoutSessionRequest {
            product_id: "PROD_ABC123".into(),
            currency: "AUD".into(),
            price_snapshot: None,
            with_trial: None,
            buyer_email: None,
            billing_detail: None,
            success_url: Some("https://example.com/paid".into()),
            expires_in_seconds: Some(2_700),
            dark_mode: None,
            metadata: None,
            order_merchant_external_id: Some("order-1".into()),
            language: Some("en".into()),
            include_payment_methods: Some(vec!["card".into()]),
            exclude_payment_methods: None,
        }
    }

    #[test]
    fn checkout_contract_rejects_unsupported_or_ambiguous_values() {
        assert!(request().validate().is_ok());

        let mut invalid = request();
        invalid.language = Some("en-AU".into());
        assert!(invalid.validate().is_err());

        let mut invalid = request();
        invalid.expires_in_seconds = Some(604_801);
        assert!(invalid.validate().is_err());

        let mut invalid = request();
        invalid.exclude_payment_methods = Some(vec!["wechat".into()]);
        assert!(invalid.validate().is_err());

        let mut invalid = request();
        invalid.order_merchant_external_id = Some("   ".into());
        assert!(invalid.validate().is_err());
    }
}
