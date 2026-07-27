//! Minco's ergonomic facade crate.
//!
//! `minco` re-exports the provider-neutral kernel and enables the contract,
//! HTTP, and official default-plugin crates through Cargo features. Database,
//! Lambda, planning, release, and test support remain opt-in so applications
//! compile only the capabilities they use.
//!
//! # Minimal composition
//!
//! ```rust
//! use minco::prelude::*;
//!
//! let manager = minco::default_plugin_manager()?;
//! let _application = manager.compose(&PluginSelection::default())?;
//! # Ok::<(), minco::core::PluginError>(())
//! ```
#![forbid(unsafe_code)]

/// Provider-neutral plugin, capability, service, and application graph APIs.
pub use minco_core as core;

#[cfg(feature = "config")]
/// Typed environments, configuration schema, provenance, and secret references.
pub use minco_config as config;

#[cfg(feature = "contract")]
/// `OpenAPI` loading, validation, operation inventory, and deterministic binding generation.
pub use minco_contract as contract;

#[cfg(feature = "http")]
/// Axum and Tower delivery conventions.
pub use minco_http as http;

#[cfg(feature = "plan")]
/// Provider-neutral deployment planning, structural cost checks, and SAM rendering.
pub use minco_plan as plan;

#[cfg(feature = "release")]
/// Immutable release manifests and artifact verification.
pub use minco_release as release;

#[cfg(feature = "test")]
/// In-process HTTP and command-evidence test helpers.
pub use minco_test as test;

#[cfg(feature = "plugin-health")]
/// Official health and readiness plugin.
pub use minco_plugin_health as plugin_health;

#[cfg(feature = "plugin-observability")]
/// Official structured-observability plugin.
pub use minco_plugin_observability as plugin_observability;

#[cfg(feature = "plugin-idempotency")]
/// Official idempotency primitives and plugin.
pub use minco_plugin_idempotency as plugin_idempotency;

#[cfg(feature = "plugin-sessions")]
/// Official provider-neutral session issuance and revocation plugin.
pub use minco_plugin_sessions as plugin_sessions;

#[cfg(feature = "plugin-identity")]
/// Official verified-claims identity and permission plugin.
pub use minco_plugin_identity as plugin_identity;

#[cfg(feature = "plugin-object-storage")]
/// Official provider-neutral object-storage plugin.
pub use minco_plugin_object_storage as plugin_object_storage;

#[cfg(feature = "plugin-events")]
/// Official domain-event and explicit outbox plugin.
pub use minco_plugin_events as plugin_events;

#[cfg(feature = "plugin-notifications")]
/// Official provider-neutral notification plugin.
pub use minco_plugin_notifications as plugin_notifications;

#[cfg(feature = "plugin-audit")]
/// Official append-only audit plugin.
pub use minco_plugin_audit as plugin_audit;

#[cfg(feature = "plugin-feedback")]
/// Official AI-ready client feedback-loop plugin.
pub use minco_plugin_feedback as plugin_feedback;

#[cfg(feature = "plugin-static-site")]
/// Official provider-neutral static-site deployment plugin.
pub use minco_plugin_static_site as plugin_static_site;

#[cfg(feature = "sqlx-postgres")]
/// Bounded `SQLx` `PostgreSQL` pool support.
pub use minco_sqlx_postgres as sqlx_postgres;

#[cfg(feature = "sqlx-sqlite")]
/// `SQLx` `SQLite` pool support.
pub use minco_sqlx_sqlite as sqlx_sqlite;

#[cfg(feature = "aws-lambda")]
/// Native AWS Lambda HTTP and SSM integration.
pub use minco_aws_lambda as aws_lambda;

#[cfg(feature = "aws-worker")]
/// Native AWS Lambda SQS partial-batch worker runtime.
pub use minco_aws_worker as aws_worker;

#[cfg(feature = "aws-adapters")]
/// Production S3, SQS, SES, Cognito, webhook, and static-site adapters.
pub use minco_aws_adapters as aws_adapters;

/// Common imports for application composition.
pub mod prelude {
    #[cfg(feature = "config")]
    pub use minco_config::{
        ConfigLayer, ConfigSourceKind, ConfigurationField, ConfigurationGraph, ConfigurationSchema,
        ConfigurationValueKind, Environment, EnvironmentClass, SecretReference,
    };
    pub use minco_core::{
        ApplicationGraph, CapabilityProvision, CapabilityRequirement, ComposedApplication,
        ContributionCollection, ContributionRegistrar, ContributionRegistration,
        ContributionTypeRegistration, DuplicateServiceRegistration, FrozenContributions,
        FrozenServices, GraphBuilder, GraphError, HealthCheckDescriptor, IdleCostClass,
        MigrationSet, OperationDescriptor, Plugin, PluginContext, PluginDescriptor, PluginError,
        PluginFinalizeContext, PluginId, PluginManager, PluginSelection, RegistrationOwner,
        RegistrationProvenance, ResourceIntent, ResourceKind, ServiceCollection, ServiceError,
        ServiceRegistrar, ServiceRegistration, WakeSource,
    };

    #[cfg(feature = "http")]
    pub use minco_http::{
        ApiFailure, HttpConfigurationError, HttpHeaderPolicy, HttpRuntimeConfig, Principal,
        ProblemDetails, RequestMetadata, apply_standard_middleware, problem_response,
    };
}

/// Builds a plugin manager containing every official plugin enabled at compile time.
///
/// With the default feature set this registers health, observability, and
/// idempotency. Applications can disable any registered default at runtime with
/// [`core::PluginSelection`] or remove it from the binary with
/// `default-features = false` and explicit features.
pub fn default_plugin_manager() -> Result<core::PluginManager, core::PluginError> {
    let mut manager = core::PluginManager::default();
    register_enabled_plugins(&mut manager)?;
    Ok(manager)
}

fn register_enabled_plugins(manager: &mut core::PluginManager) -> Result<(), core::PluginError> {
    #[cfg(feature = "plugin-health")]
    manager.register(plugin_health::HealthPlugin)?;

    #[cfg(feature = "plugin-observability")]
    manager.register(plugin_observability::ObservabilityPlugin::default())?;

    #[cfg(feature = "plugin-idempotency")]
    manager.register(plugin_idempotency::IdempotencyPlugin::memory())?;

    #[cfg(feature = "plugin-sessions")]
    manager.register(plugin_sessions::SessionsPlugin::memory())?;

    #[cfg(feature = "plugin-identity")]
    manager.register(plugin_identity::IdentityPlugin::default())?;

    #[cfg(feature = "plugin-object-storage")]
    manager.register(plugin_object_storage::ObjectStoragePlugin::memory())?;

    #[cfg(feature = "plugin-events")]
    manager.register(plugin_events::EventsPlugin::memory().0)?;

    #[cfg(feature = "plugin-notifications")]
    manager.register(plugin_notifications::NotificationsPlugin::memory().0)?;

    #[cfg(feature = "plugin-audit")]
    manager.register(plugin_audit::AuditPlugin::memory().0)?;

    #[cfg(feature = "plugin-feedback")]
    manager.register(plugin_feedback::FeedbackPlugin::memory())?;

    #[cfg(feature = "plugin-static-site")]
    manager.register(plugin_static_site::StaticSitePlugin::default())?;

    // Keep the no-feature build warning-free when every registration above is
    // compiled out.
    let _ = manager;
    Ok(())
}

/// Composes all compile-time official plugins using the supplied runtime selection.
pub fn compose_defaults(
    selection: &core::PluginSelection,
) -> Result<core::ComposedApplication, core::PluginError> {
    default_plugin_manager()?.compose(selection)
}

#[cfg(all(test, feature = "default-plugins"))]
mod tests {
    use super::*;

    #[test]
    fn default_features_compose_the_official_plugin_set() {
        let application = compose_defaults(&core::PluginSelection::default()).unwrap();
        let ids = application
            .graph
            .plugins
            .iter()
            .map(|plugin| plugin.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, ["health", "idempotency", "observability"]);
    }
}
