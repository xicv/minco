//! Provider-neutral Minco kernel: plugin composition, capabilities, resources and graph validation.
#![forbid(unsafe_code)]

mod graph;
mod plugin;
mod service;
mod types;

pub use graph::{ApplicationGraph, GraphBuilder, GraphError};
pub use plugin::{
    ComposedApplication, Plugin, PluginContext, PluginError, PluginManager, PluginSelection,
};
pub use service::{FrozenServices, ServiceCollection, ServiceError};
pub use types::{
    CapabilityProvision, CapabilityRequirement, HealthCheckDescriptor, IdentifierError,
    IdleCostClass, MigrationSet, OperationDescriptor, PluginDescriptor, PluginId, ResourceIntent,
    ResourceKind, WakeSource,
};
