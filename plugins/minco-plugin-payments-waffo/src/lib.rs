//! Static Minco integration for Waffo Pancake's signed payment API.
//!
//! The plugin keeps discovery and composition deterministic. Remote calls occur
//! only through an explicitly constructed [`WaffoClient`], while secrets remain
//! opaque Minco references until a caller supplies a [`SecretResolver`].
#![forbid(unsafe_code)]

mod checkout;
mod client;
pub(crate) mod config;
pub(crate) mod configuration_schema;
mod error;
pub(crate) mod graphql;
mod plugin;
pub(crate) mod signing;
mod webhook;

pub use checkout::{CheckoutSession, CreateCheckoutSessionRequest, WaffoWebhook};
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

pub(crate) use config::RawWaffoConfiguration;
