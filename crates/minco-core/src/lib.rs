//! Provider-neutral Minco kernel: plugin composition, capabilities, resources and graph validation.
#![forbid(unsafe_code)]

/// Semantic version of the public Minco core plugin API.
pub const CORE_API_VERSION: &str = env!("CARGO_PKG_VERSION");

mod contribution;
mod graph;
mod plugin;
mod provenance;
mod service;
mod types;

pub use contribution::{ContributionCollection, ContributionRegistrar, FrozenContributions};
pub use graph::{ApplicationGraph, GraphBuilder, GraphError};
pub use plugin::{
    ComposedApplication, Plugin, PluginContext, PluginError, PluginFinalizeContext, PluginManager,
    PluginSelection,
};
pub use provenance::{
    ContributionRegistration, ContributionTypeRegistration, RegistrationOwner,
    RegistrationProvenance, ServiceRegistration,
};
pub use service::{
    DuplicateServiceRegistration, FrozenServices, ServiceCollection, ServiceError,
    ServiceRegistrar, Shared,
};
pub use types::{
    CapabilityProvision, CapabilityRequirement, ConfigurationField, ConfigurationValueKind,
    DataClass, HealthCheckDescriptor, IdentifierError, IdleCostClass, MigrationSet,
    OperationDescriptor, PluginDescriptor, PluginId, PluginStability, ResourceIntent, ResourceKind,
    WakeSource,
};
