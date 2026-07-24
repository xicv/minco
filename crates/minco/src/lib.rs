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

#[cfg(feature = "sqlx-postgres")]
/// Bounded `SQLx` `PostgreSQL` pool support.
pub use minco_sqlx_postgres as sqlx_postgres;

#[cfg(feature = "sqlx-sqlite")]
/// `SQLx` `SQLite` pool support.
pub use minco_sqlx_sqlite as sqlx_sqlite;

#[cfg(feature = "aws-lambda")]
/// Native AWS Lambda HTTP and SSM integration.
pub use minco_aws_lambda as aws_lambda;

/// Common imports for application composition.
pub mod prelude {
    pub use minco_core::{
        ApplicationGraph, CapabilityProvision, CapabilityRequirement, ComposedApplication,
        FrozenServices, GraphBuilder, GraphError, HealthCheckDescriptor, IdleCostClass,
        MigrationSet, OperationDescriptor, Plugin, PluginContext, PluginDescriptor, PluginError,
        PluginId, PluginManager, PluginSelection, ResourceIntent, ResourceKind, ServiceCollection,
        ServiceError, WakeSource,
    };

    #[cfg(feature = "http")]
    pub use minco_http::{
        ApiFailure, HttpRuntimeConfig, Principal, ProblemDetails, RequestMetadata,
        apply_standard_middleware, problem_response,
    };
}

/// Builds a plugin manager containing every official plugin enabled at compile time.
///
/// With the default feature set this registers health, observability, and
/// idempotency. Applications can disable any registered default at runtime with
/// [`core::PluginSelection`] or remove it from the binary with
/// `default-features = false` and explicit features.
pub fn default_plugin_manager() -> Result<core::PluginManager, core::PluginError> {
    let manager = core::PluginManager::default();

    #[cfg(any(
        feature = "plugin-health",
        feature = "plugin-observability",
        feature = "plugin-idempotency"
    ))]
    let mut manager = manager;

    #[cfg(feature = "plugin-health")]
    manager.register(plugin_health::HealthPlugin)?;

    #[cfg(feature = "plugin-observability")]
    manager.register(plugin_observability::ObservabilityPlugin::default())?;

    #[cfg(feature = "plugin-idempotency")]
    manager.register(plugin_idempotency::IdempotencyPlugin)?;

    Ok(manager)
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
