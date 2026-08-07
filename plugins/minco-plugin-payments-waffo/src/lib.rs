//! Static Minco integration for Waffo Pancake's signed payment API.
//!
//! The plugin keeps discovery and composition deterministic. Remote calls occur
//! only through an explicitly constructed [`WaffoClient`], while secrets remain
//! opaque Minco references until a caller supplies a [`SecretResolver`].
#![forbid(unsafe_code)]

mod checkout;
mod client;
mod config;
mod configuration_schema;
mod error;
mod graphql;
mod identifier;
mod plugin;
mod signing;
mod webhook;

pub use checkout::{
    BillingDetail, CashierLanguage, Checkout, CheckoutSession, CreateCheckoutSessionRequest,
    PaymentMethod, PriceInfo, TaxCategory, WaffoWebhook,
};
pub use client::{
    ADD_WEBHOOK_PATH, CHECKOUT_CREATE_SESSION_PATH, GRAPHQL_PATH, WaffoClient,
    validate_idempotency_key,
};
pub use config::{
    CONFIGURATION_NAMESPACE, DEFAULT_API_BASE_URL, DEFAULT_REQUEST_MAX_BYTES, WaffoConfiguration,
    WaffoEnvironment,
};
pub use error::{WaffoApiError, WaffoError};
pub use plugin::{PLUGIN_ID, WaffoPlugin, WaffoService};
pub use signing::{EnvironmentSecretResolver, SecretResolver, SecretValue};
pub use webhook::{
    VerifiedWaffoWebhook, WaffoWebhookEvent, WaffoWebhookMode, WaffoWebhookVerifier,
};

/// Exact official Waffo Go SDK revision reviewed for this provider contract.
pub const REVIEWED_WAFFO_SDK_REVISION: &str = "df098331cf5ea7d43ad79ab223d9eda6d4ac8e5f";

pub(crate) use config::RawWaffoConfiguration;
