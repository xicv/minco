//! Provider-neutral typed configuration graph for Minco applications.
//!
//! The graph composes documented layers without resolving secret values.
//! Secret-bearing fields accept only [`SecretReference`] values, and command
//! output can use [`ConfigurationGraph::explain`] without exposing those
//! references.
#![forbid(unsafe_code)]

mod diagnostic;
mod graph;
mod layer;
mod schema;
mod secret;

pub use diagnostic::{ConfigDiagnostic, ConfigurationError};
pub use graph::{
    ConfigDiff, ConfigDiffEntry, ConfigExplanation, ConfigProvenance, ConfigSource,
    ConfigurationGraph, Environment, EnvironmentClass,
};
pub use layer::{ConfigLayer, ConfigLayerError, ConfigSourceKind};
pub use schema::{ConfigurationField, ConfigurationSchema, ConfigurationValueKind};
pub use secret::{SecretProvider, SecretReference, SecretReferenceError};

fn value_matches(kind: ConfigurationValueKind, value: &serde_json::Value) -> bool {
    match kind {
        ConfigurationValueKind::String => value.is_string(),
        ConfigurationValueKind::Integer => value.as_i64().is_some() || value.as_u64().is_some(),
        ConfigurationValueKind::Number => value.is_number(),
        ConfigurationValueKind::Boolean => value.is_boolean(),
        ConfigurationValueKind::StringList => value
            .as_array()
            .is_some_and(|values| values.iter().all(serde_json::Value::is_string)),
        ConfigurationValueKind::Object => value.is_object(),
    }
}
