use crate::PluginId;
use serde::{Serialize, Serializer, ser::SerializeStruct};
use std::fmt;

/// Authoritative owner of a composition-time service or contribution registration.
///
/// Owners are created only by Minco's composition boundary. Application code can inspect an
/// owner, but neither an application nor a plugin can construct a plugin-owned value and spoof a
/// different plugin's identity.
///
/// ```compile_fail
/// use minco_core::{PluginId, RegistrationOwner};
///
/// let forged = RegistrationOwner::plugin(PluginId::new("another-plugin")?);
/// # Ok::<(), minco_core::IdentifierError>(())
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct RegistrationOwner(RegistrationOwnerKind);

#[derive(Clone, PartialEq, Eq)]
enum RegistrationOwnerKind {
    Application,
    Plugin(PluginId),
}

impl RegistrationOwner {
    pub(crate) const fn application() -> Self {
        Self(RegistrationOwnerKind::Application)
    }

    pub(crate) const fn plugin(plugin_id: PluginId) -> Self {
        Self(RegistrationOwnerKind::Plugin(plugin_id))
    }

    /// Returns `true` for services and contributions seeded by `compose_with`.
    pub const fn is_application(&self) -> bool {
        matches!(self.0, RegistrationOwnerKind::Application)
    }

    /// Returns the authoritative plugin ID, or `None` for application-seeded registrations.
    pub const fn plugin_id(&self) -> Option<&PluginId> {
        match &self.0 {
            RegistrationOwnerKind::Application => None,
            RegistrationOwnerKind::Plugin(plugin_id) => Some(plugin_id),
        }
    }
}

impl fmt::Display for RegistrationOwner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            RegistrationOwnerKind::Application => formatter.write_str("application"),
            RegistrationOwnerKind::Plugin(plugin_id) => write!(formatter, "plugin:{plugin_id}"),
        }
    }
}

impl fmt::Debug for RegistrationOwner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("RegistrationOwner")
            .field(&self.to_string())
            .finish()
    }
}

impl Serialize for RegistrationOwner {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self.0 {
            RegistrationOwnerKind::Application => {
                let mut state = serializer.serialize_struct("RegistrationOwner", 1)?;
                state.serialize_field("kind", "application")?;
                state.end()
            }
            RegistrationOwnerKind::Plugin(plugin_id) => {
                let mut state = serializer.serialize_struct("RegistrationOwner", 2)?;
                state.serialize_field("kind", "plugin")?;
                state.serialize_field("plugin_id", plugin_id)?;
                state.end()
            }
        }
    }
}

/// Bounded singleton-service registration metadata retained after composition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServiceRegistration {
    pub rust_type: &'static str,
    pub owner: RegistrationOwner,
}

/// One ordered registration within a contribution type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContributionRegistration {
    pub owner: RegistrationOwner,
    pub installation_index: usize,
}

/// Bounded contribution metadata grouped by Rust type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContributionTypeRegistration {
    pub rust_type: &'static str,
    pub registrations: Vec<ContributionRegistration>,
}

/// Complete metadata-only registration provenance for a composed application.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RegistrationProvenance {
    pub services: Vec<ServiceRegistration>,
    pub contributions: Vec<ContributionTypeRegistration>,
}
