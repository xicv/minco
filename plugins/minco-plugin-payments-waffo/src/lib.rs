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
mod transport;
mod webhook;

pub use checkout::{
    AuthenticatedCheckout, AuthenticatedCheckoutIdempotencyKeys, AuthenticatedCheckoutResult,
    BillingDetail, CashierLanguage, Checkout, CheckoutSession, CreateCheckoutSessionRequest,
    IssueSessionTokenRequest, PaymentMethod, PriceInfo, SessionToken, TaxCategory, WaffoWebhook,
};
pub use client::{
    ADD_WEBHOOK_PATH, CHECKOUT_CREATE_SESSION_PATH, GRAPHQL_PATH, ISSUE_SESSION_TOKEN_PATH,
    WaffoClient, validate_action_path, validate_idempotency_key,
};
pub use config::{
    CONFIGURATION_NAMESPACE, DEFAULT_API_BASE_URL, DEFAULT_REQUEST_MAX_BYTES, WaffoConfiguration,
    WaffoEnvironment,
};
pub use error::{
    UntrustedWaffoAiHint, WaffoApiError, WaffoError, WaffoGraphqlLocation, WaffoResponse,
    WaffoTransportFailure,
};
pub use graphql::validate_read_only_graphql;
pub use plugin::{PLUGIN_ID, WaffoPlugin, WaffoService};
pub use signing::{EnvironmentSecretResolver, SecretResolver, SecretValue};
pub use transport::{
    CapturedWaffoRequest, FakeWaffoTransport, ReqwestWaffoTransport, WaffoTransport,
    WaffoTransportRequest, WaffoTransportResponse,
};
pub use webhook::{
    VerifiedWaffoWebhook, WaffoWebhookEvent, WaffoWebhookMode, WaffoWebhookVerifier,
};

/// Exact official Waffo Go SDK revision reviewed for this provider contract.
pub const REVIEWED_WAFFO_SDK_REVISION: &str = "799135cbe07c45819da0ab4bf777c64fcc956220";

pub(crate) use config::RawWaffoConfiguration;
