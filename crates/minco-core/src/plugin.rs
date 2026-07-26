use crate::{
    ApplicationGraph, CORE_API_VERSION, ConfigurationField, ConfigurationValueKind,
    ContributionCollection, ContributionRegistrar, FrozenContributions, FrozenServices,
    GraphBuilder, GraphError, PluginDescriptor, PluginId, RegistrationOwner,
    RegistrationProvenance, ServiceCollection, ServiceError, ServiceRegistrar,
};
use semver::Version;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use thiserror::Error;

/// Statically linked Minco extension.
///
/// `install` must be deterministic and side-effect free with respect to remote systems.
/// Connections, migrations, background work, and other runtime effects belong in explicitly
/// registered services and lifecycle components, not in plugin discovery.
pub trait Plugin: Send + Sync + 'static {
    fn descriptor(&self) -> PluginDescriptor;

    /// Adjusts graph metadata from the plugin's validated runtime configuration.
    ///
    /// This hook is intentionally limited to the descriptor: it must be deterministic,
    /// side-effect free, and must not construct clients or connect to infrastructure. It allows
    /// configuration-dependent capabilities, dependencies, resources, operations, migrations,
    /// and health checks to participate in graph validation before installation. Plugin identity,
    /// version, default selection, and configuration schema are immutable.
    fn configure_descriptor(
        &self,
        _descriptor: &mut PluginDescriptor,
        _configuration: Option<&serde_json::Value>,
    ) -> Result<(), PluginError> {
        Ok(())
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError>;

    /// Completes composition after every enabled plugin has installed its services and
    /// contributions.
    ///
    /// Finalization is the narrow startup hook for registries that aggregate independent
    /// contributions, such as health checks. It must remain deterministic and must not perform
    /// migrations, network calls, or background work.
    fn finalize(&self, _context: &mut PluginFinalizeContext<'_>) -> Result<(), PluginError> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct PluginContext<'a> {
    plugin_id: &'a PluginId,
    configuration: Option<&'a serde_json::Value>,
    services: &'a mut ServiceCollection,
    contributions: &'a mut ContributionCollection,
}

/// Second-pass plugin context exposed after every plugin has completed installation.
///
/// Services remain mutable so an authoritative registry can be populated, while contributions
/// are read-only to make the two-phase lifecycle deterministic.
#[derive(Debug)]
pub struct PluginFinalizeContext<'a> {
    plugin_id: &'a PluginId,
    configuration: Option<&'a serde_json::Value>,
    services: &'a mut ServiceCollection,
    contributions: &'a ContributionCollection,
}

impl PluginFinalizeContext<'_> {
    pub const fn plugin_id(&self) -> &PluginId {
        self.plugin_id
    }

    pub fn services(&mut self) -> ServiceRegistrar<'_> {
        ServiceRegistrar {
            services: self.services,
            owner: RegistrationOwner::plugin(self.plugin_id.clone()),
        }
    }

    pub const fn contributions(&self) -> &ContributionCollection {
        self.contributions
    }

    pub const fn raw_configuration(&self) -> Option<&serde_json::Value> {
        self.configuration
    }

    pub fn configuration<T>(&self) -> Result<T, PluginError>
    where
        T: DeserializeOwned + Default,
    {
        deserialize_configuration(self.plugin_id, self.configuration)
    }
}

impl PluginContext<'_> {
    pub const fn plugin_id(&self) -> &PluginId {
        self.plugin_id
    }

    pub fn services(&mut self) -> ServiceRegistrar<'_> {
        ServiceRegistrar {
            services: self.services,
            owner: RegistrationOwner::plugin(self.plugin_id.clone()),
        }
    }

    pub fn contributions(&mut self) -> ContributionRegistrar<'_> {
        ContributionRegistrar {
            contributions: self.contributions,
            owner: RegistrationOwner::plugin(self.plugin_id.clone()),
        }
    }

    pub const fn raw_configuration(&self) -> Option<&serde_json::Value> {
        self.configuration
    }

    /// Deserializes the selected plugin configuration, or returns `T::default` when no
    /// configuration was supplied.
    pub fn configuration<T>(&self) -> Result<T, PluginError>
    where
        T: DeserializeOwned + Default,
    {
        deserialize_configuration(self.plugin_id, self.configuration)
    }
}

fn deserialize_configuration<T>(
    plugin_id: &PluginId,
    configuration: Option<&serde_json::Value>,
) -> Result<T, PluginError>
where
    T: DeserializeOwned + Default,
{
    configuration.map_or_else(
        || Ok(T::default()),
        |value| {
            serde_json::from_value(value.clone()).map_err(|source| {
                PluginError::InvalidConfiguration {
                    plugin: plugin_id.clone(),
                    source,
                }
            })
        },
    )
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginSelection {
    #[serde(default)]
    pub enabled: BTreeSet<PluginId>,
    #[serde(default)]
    pub disabled: BTreeSet<PluginId>,
    /// Plugin-specific configuration indexed by stable plugin ID.
    #[serde(default)]
    pub configuration: BTreeMap<PluginId, serde_json::Value>,
}

impl PluginSelection {
    pub fn is_enabled(&self, descriptor: &PluginDescriptor) -> bool {
        if self.disabled.contains(&descriptor.id) {
            return false;
        }
        self.enabled.contains(&descriptor.id) || descriptor.default_enabled
    }

    pub fn set_configuration<T>(
        &mut self,
        plugin_id: PluginId,
        configuration: &T,
    ) -> Result<(), PluginError>
    where
        T: Serialize,
    {
        let value = serde_json::to_value(configuration).map_err(|source| {
            PluginError::InvalidConfiguration {
                plugin: plugin_id.clone(),
                source,
            }
        })?;
        self.configuration.insert(plugin_id, value);
        Ok(())
    }
}

struct RegisteredPlugin {
    plugin: Arc<dyn Plugin>,
    descriptor: PluginDescriptor,
}

impl std::fmt::Debug for RegisteredPlugin {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RegisteredPlugin")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
struct EffectivePlugin {
    plugin: Arc<dyn Plugin>,
    descriptor: PluginDescriptor,
    configuration: Option<serde_json::Value>,
}

impl std::fmt::Debug for EffectivePlugin {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EffectivePlugin")
            .field("descriptor", &self.descriptor)
            .field("configuration_present", &self.configuration.is_some())
            .finish_non_exhaustive()
    }
}

struct ResolvedGraph {
    enabled: BTreeMap<PluginId, EffectivePlugin>,
    ordered: Vec<PluginId>,
    graph: ApplicationGraph,
}

#[derive(Default)]
pub struct PluginManager {
    plugins: BTreeMap<PluginId, RegisteredPlugin>,
}

impl std::fmt::Debug for PluginManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginManager")
            .field("plugin_ids", &self.plugins.keys())
            .finish()
    }
}

impl PluginManager {
    pub fn register<P>(&mut self, plugin: P) -> Result<(), PluginError>
    where
        P: Plugin,
    {
        self.register_arc(Arc::new(plugin))
    }

    pub fn register_arc(&mut self, plugin: Arc<dyn Plugin>) -> Result<(), PluginError> {
        let descriptor = plugin.descriptor();
        validate_configuration_descriptor(&descriptor)?;
        let core_version =
            Version::parse(CORE_API_VERSION).map_err(|source| PluginError::InvalidCoreVersion {
                value: CORE_API_VERSION.to_owned(),
                source,
            })?;
        if !descriptor.core_compatibility.matches(&core_version) {
            return Err(PluginError::IncompatibleCore {
                plugin: descriptor.id,
                requirement: descriptor.core_compatibility.to_string(),
                actual: core_version,
            });
        }
        if self.plugins.contains_key(&descriptor.id) {
            return Err(PluginError::DuplicatePlugin(descriptor.id));
        }
        self.plugins.insert(
            descriptor.id.clone(),
            RegisteredPlugin { plugin, descriptor },
        );
        Ok(())
    }

    pub fn descriptors(&self) -> Vec<PluginDescriptor> {
        self.plugins
            .values()
            .map(|registration| registration.descriptor.clone())
            .collect()
    }

    pub fn compose(&self, selection: &PluginSelection) -> Result<ComposedApplication, PluginError> {
        self.compose_with(
            selection,
            ServiceCollection::default(),
            ContributionCollection::default(),
        )
    }

    /// Resolves and validates the configured application graph without installing services.
    ///
    /// Deployment planning and other read-only tooling use this method so graph inspection
    /// cannot construct clients, connect to infrastructure, or trigger plugin lifecycle hooks.
    pub fn build_graph(
        &self,
        selection: &PluginSelection,
    ) -> Result<ApplicationGraph, PluginError> {
        Ok(self.resolve_graph(selection)?.graph)
    }

    /// Composes plugins on top of application-provided services and contributions.
    ///
    /// This is the explicit dependency-injection boundary for concrete database pools, AWS
    /// clients, clocks, and other composition-root concerns. Registrations remain typed and
    /// duplicate service types are still rejected; Minco never falls back to a global locator.
    pub fn compose_with(
        &self,
        selection: &PluginSelection,
        mut services: ServiceCollection,
        mut contributions: ContributionCollection,
    ) -> Result<ComposedApplication, PluginError> {
        // Validate the complete configured application graph before constructing services. This
        // prevents externally backed services from being created for an invalid composition.
        let ResolvedGraph {
            enabled,
            ordered,
            graph,
        } = self.resolve_graph(selection)?;

        for id in &ordered {
            let effective = enabled
                .get(id)
                .ok_or_else(|| PluginError::UnknownPlugin(id.clone()))?;
            effective.plugin.install(&mut PluginContext {
                plugin_id: id,
                configuration: effective.configuration.as_ref(),
                services: &mut services,
                contributions: &mut contributions,
            })?;
        }

        for id in &ordered {
            let effective = enabled
                .get(id)
                .ok_or_else(|| PluginError::UnknownPlugin(id.clone()))?;
            effective.plugin.finalize(&mut PluginFinalizeContext {
                plugin_id: id,
                configuration: effective.configuration.as_ref(),
                services: &mut services,
                contributions: &contributions,
            })?;
        }

        Ok(ComposedApplication {
            graph,
            services: services.freeze(),
            contributions: contributions.freeze(),
        })
    }

    fn resolve_graph(&self, selection: &PluginSelection) -> Result<ResolvedGraph, PluginError> {
        self.validate_selection(selection)?;
        let enabled = self.resolve_enabled(selection)?;
        let ordered = topological_order(&enabled)?;
        let mut graph_builder = GraphBuilder::default();
        for id in &ordered {
            let effective = enabled
                .get(id)
                .ok_or_else(|| PluginError::UnknownPlugin(id.clone()))?;
            graph_builder.add_plugin(effective.descriptor.clone());
        }
        let graph = graph_builder.build()?;
        Ok(ResolvedGraph {
            enabled,
            ordered,
            graph,
        })
    }

    fn validate_selection(&self, selection: &PluginSelection) -> Result<(), PluginError> {
        if let Some(id) = selection.enabled.intersection(&selection.disabled).next() {
            return Err(PluginError::ContradictorySelection(id.clone()));
        }
        for selected in selection
            .enabled
            .iter()
            .chain(&selection.disabled)
            .chain(selection.configuration.keys())
        {
            if !self.plugins.contains_key(selected) {
                return Err(PluginError::UnknownPlugin(selected.clone()));
            }
        }
        Ok(())
    }

    fn resolve_enabled(
        &self,
        selection: &PluginSelection,
    ) -> Result<BTreeMap<PluginId, EffectivePlugin>, PluginError> {
        let mut enabled = BTreeMap::new();
        for (id, registration) in &self.plugins {
            if selection.is_enabled(&registration.descriptor) {
                enabled.insert(id.clone(), self.effective_plugin(id, selection)?);
            }
        }

        let mut changed = true;
        while changed {
            changed = false;
            let descriptors = enabled
                .values()
                .map(|effective| effective.descriptor.clone())
                .collect::<Vec<_>>();
            for descriptor in descriptors {
                for dependency in descriptor.plugin_dependencies {
                    if selection.disabled.contains(&dependency) {
                        return Err(PluginError::DisabledRequiredPlugin {
                            plugin: descriptor.id,
                            dependency,
                        });
                    }
                    if !enabled.contains_key(&dependency) {
                        if !self.plugins.contains_key(&dependency) {
                            return Err(PluginError::MissingPluginDependency {
                                plugin: descriptor.id,
                                dependency,
                            });
                        }
                        enabled.insert(
                            dependency.clone(),
                            self.effective_plugin(&dependency, selection)?,
                        );
                        changed = true;
                    }
                }
            }
        }
        Ok(enabled)
    }

    fn effective_plugin(
        &self,
        id: &PluginId,
        selection: &PluginSelection,
    ) -> Result<EffectivePlugin, PluginError> {
        let registration = self
            .plugins
            .get(id)
            .ok_or_else(|| PluginError::UnknownPlugin(id.clone()))?;
        let configuration =
            normalize_configuration(&registration.descriptor, selection.configuration.get(id))?;
        let mut descriptor = registration.descriptor.clone();
        registration
            .plugin
            .configure_descriptor(&mut descriptor, configuration.as_ref())?;
        validate_configured_descriptor(&registration.descriptor, &descriptor)?;
        Ok(EffectivePlugin {
            plugin: Arc::clone(&registration.plugin),
            descriptor,
            configuration,
        })
    }
}

fn validate_configuration_descriptor(descriptor: &PluginDescriptor) -> Result<(), PluginError> {
    let mut keys = BTreeSet::new();
    for field in &descriptor.configuration {
        if field.key.trim().is_empty() {
            return Err(PluginError::InvalidConfigurationDescriptor {
                plugin: descriptor.id.clone(),
                message: "configuration field keys must not be empty".into(),
            });
        }
        if !keys.insert(field.key.clone()) {
            return Err(PluginError::InvalidConfigurationDescriptor {
                plugin: descriptor.id.clone(),
                message: format!("duplicate configuration field: {}", field.key),
            });
        }
        if field.secret && field.default.is_some() {
            return Err(PluginError::InvalidConfigurationDescriptor {
                plugin: descriptor.id.clone(),
                message: "secret configuration fields cannot have defaults".into(),
            });
        }
        if let Some(default) = &field.default {
            validate_configuration_value(&descriptor.id, field, default).map_err(|error| {
                PluginError::InvalidConfigurationDescriptor {
                    plugin: descriptor.id.clone(),
                    message: error.to_string(),
                }
            })?;
        }
    }
    Ok(())
}

fn validate_configured_descriptor(
    base: &PluginDescriptor,
    configured: &PluginDescriptor,
) -> Result<(), PluginError> {
    if configured.id != base.id
        || configured.version != base.version
        || configured.default_enabled != base.default_enabled
        || configured.configuration_namespace != base.configuration_namespace
        || configured.configuration != base.configuration
    {
        return Err(PluginError::ConfiguredDescriptorIdentityChanged {
            plugin: base.id.clone(),
        });
    }
    validate_configuration_descriptor(configured)?;

    let core_version =
        Version::parse(CORE_API_VERSION).map_err(|source| PluginError::InvalidCoreVersion {
            value: CORE_API_VERSION.to_owned(),
            source,
        })?;
    if !configured.core_compatibility.matches(&core_version) {
        return Err(PluginError::IncompatibleCore {
            plugin: configured.id.clone(),
            requirement: configured.core_compatibility.to_string(),
            actual: core_version,
        });
    }
    Ok(())
}

fn normalize_configuration(
    descriptor: &PluginDescriptor,
    raw: Option<&serde_json::Value>,
) -> Result<Option<serde_json::Value>, PluginError> {
    // Plugins that do not publish a configuration contract retain backwards-compatible
    // ownership of their raw configuration object. Official and ecosystem plugins should
    // publish fields so Minco can validate and apply defaults before installation.
    if descriptor.configuration.is_empty() {
        return Ok(raw.cloned());
    }

    let supplied = match raw {
        None => serde_json::Map::new(),
        Some(serde_json::Value::Object(values)) => values.clone(),
        Some(_) => {
            return Err(PluginError::ConfigurationMustBeObject {
                plugin: descriptor.id.clone(),
            });
        }
    };

    let fields = descriptor
        .configuration
        .iter()
        .map(|field| (field.key.as_str(), field))
        .collect::<BTreeMap<_, _>>();

    for key in supplied.keys() {
        if !fields.contains_key(key.as_str()) {
            return Err(PluginError::UnknownConfigurationField {
                plugin: descriptor.id.clone(),
                field: key.clone(),
            });
        }
    }

    let mut normalized = serde_json::Map::new();
    for field in &descriptor.configuration {
        let value = supplied
            .get(&field.key)
            .cloned()
            .filter(|value| field.required || !value.is_null())
            .or_else(|| field.default.clone());
        match value {
            Some(value) => {
                validate_configuration_value(&descriptor.id, field, &value)?;
                normalized.insert(field.key.clone(), value);
            }
            None if field.required => {
                return Err(PluginError::MissingConfigurationField {
                    plugin: descriptor.id.clone(),
                    field: field.key.clone(),
                });
            }
            None => {}
        }
    }

    Ok(Some(serde_json::Value::Object(normalized)))
}

fn validate_configuration_value(
    plugin: &PluginId,
    field: &ConfigurationField,
    value: &serde_json::Value,
) -> Result<(), PluginError> {
    let matches = match field.kind {
        ConfigurationValueKind::String => value.is_string(),
        ConfigurationValueKind::Integer => value.as_i64().is_some() || value.as_u64().is_some(),
        ConfigurationValueKind::Number => value.is_number(),
        ConfigurationValueKind::Boolean => value.is_boolean(),
        ConfigurationValueKind::StringList => value
            .as_array()
            .is_some_and(|values| values.iter().all(serde_json::Value::is_string)),
        ConfigurationValueKind::Object => value.is_object(),
    };
    if matches {
        Ok(())
    } else {
        Err(PluginError::ConfigurationTypeMismatch {
            plugin: plugin.clone(),
            field: field.key.clone(),
            expected: field.kind,
        })
    }
}

fn topological_order(
    plugins: &BTreeMap<PluginId, EffectivePlugin>,
) -> Result<Vec<PluginId>, PluginError> {
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut ordered = Vec::new();
    for id in plugins.keys() {
        visit(id, plugins, &mut visiting, &mut visited, &mut ordered)?;
    }
    Ok(ordered)
}

fn visit(
    id: &PluginId,
    plugins: &BTreeMap<PluginId, EffectivePlugin>,
    visiting: &mut BTreeSet<PluginId>,
    visited: &mut BTreeSet<PluginId>,
    ordered: &mut Vec<PluginId>,
) -> Result<(), PluginError> {
    if visited.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id.clone()) {
        return Err(PluginError::DependencyCycle(id.clone()));
    }
    let registration = plugins
        .get(id)
        .ok_or_else(|| PluginError::UnknownPlugin(id.clone()))?;
    for dependency in &registration.descriptor.plugin_dependencies {
        visit(dependency, plugins, visiting, visited, ordered)?;
    }
    visiting.remove(id);
    visited.insert(id.clone());
    ordered.push(id.clone());
    Ok(())
}

#[derive(Debug)]
pub struct ComposedApplication {
    pub graph: ApplicationGraph,
    pub services: FrozenServices,
    pub contributions: FrozenContributions,
}

impl ComposedApplication {
    /// Returns deterministic composition metadata without serializing registered values.
    pub fn registration_provenance(&self) -> RegistrationProvenance {
        RegistrationProvenance {
            services: self.services.registrations().to_vec(),
            contributions: self.contributions.registrations().to_vec(),
        }
    }
}

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("duplicate plugin registration: {0}")]
    DuplicatePlugin(PluginId),
    #[error("unknown plugin: {0}")]
    UnknownPlugin(PluginId),
    #[error("plugin is both explicitly enabled and disabled: {0}")]
    ContradictorySelection(PluginId),
    #[error("plugin {plugin} depends on unregistered plugin {dependency}")]
    MissingPluginDependency {
        plugin: PluginId,
        dependency: PluginId,
    },
    #[error("plugin {plugin} requires disabled plugin {dependency}")]
    DisabledRequiredPlugin {
        plugin: PluginId,
        dependency: PluginId,
    },
    #[error("plugin dependency cycle includes {0}")]
    DependencyCycle(PluginId),
    #[error(
        "plugin {plugin} requires Minco core {requirement}, but this application uses {actual}"
    )]
    IncompatibleCore {
        plugin: PluginId,
        requirement: String,
        actual: Version,
    },
    #[error("Minco core reported invalid version {value}: {source}")]
    InvalidCoreVersion {
        value: String,
        source: semver::Error,
    },
    #[error("invalid configuration for plugin {plugin}: {source}")]
    InvalidConfiguration {
        plugin: PluginId,
        source: serde_json::Error,
    },
    #[error("plugin {plugin} configuration must be a JSON object")]
    ConfigurationMustBeObject { plugin: PluginId },
    #[error("unknown configuration field for plugin {plugin}: {field}")]
    UnknownConfigurationField { plugin: PluginId, field: String },
    #[error("missing required configuration field for plugin {plugin}: {field}")]
    MissingConfigurationField { plugin: PluginId, field: String },
    #[error("configuration field {field} for plugin {plugin} must be {expected:?}")]
    ConfigurationTypeMismatch {
        plugin: PluginId,
        field: String,
        expected: ConfigurationValueKind,
    },
    #[error("invalid configuration descriptor for plugin {plugin}: {message}")]
    InvalidConfigurationDescriptor { plugin: PluginId, message: String },
    #[error(
        "configured descriptor for plugin {plugin} changed immutable identity, version, selection, or configuration-schema fields"
    )]
    ConfiguredDescriptorIdentityChanged { plugin: PluginId },
    #[error(transparent)]
    Service(#[from] ServiceError),
    #[error(transparent)]
    Graph(#[from] GraphError),
    #[error("plugin installation failed: {0}")]
    Installation(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::{Version, VersionReq};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct TestPlugin {
        descriptor: PluginDescriptor,
        value: Option<u64>,
        contribution: Option<String>,
    }

    impl Plugin for TestPlugin {
        fn descriptor(&self) -> PluginDescriptor {
            self.descriptor.clone()
        }

        fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
            if let Some(value) = self.value {
                context.services().insert(Arc::new(value))?;
            }
            if let Some(value) = &self.contribution {
                context.contributions().push(Arc::new(value.clone()));
            }
            Ok(())
        }
    }

    fn plugin(id: &str, default_enabled: bool, value: Option<u64>) -> TestPlugin {
        let mut descriptor =
            PluginDescriptor::new(PluginId::new(id).unwrap(), Version::new(1, 0, 0), id);
        descriptor.default_enabled = default_enabled;
        TestPlugin {
            descriptor,
            value,
            contribution: None,
        }
    }

    fn owner_ids(provenance: &RegistrationProvenance) -> Vec<String> {
        provenance
            .services
            .iter()
            .map(|registration| registration.owner.to_string())
            .collect()
    }

    #[test]
    fn default_plugins_can_be_disabled() {
        let mut manager = PluginManager::default();
        manager.register(plugin("default", true, Some(42))).unwrap();
        let mut selection = PluginSelection::default();
        selection.disabled.insert(PluginId::new("default").unwrap());
        let composed = manager.compose(&selection).unwrap();
        assert!(composed.graph.plugins.is_empty());
    }

    #[test]
    fn duplicate_registration_does_not_replace_the_original_plugin() {
        let mut manager = PluginManager::default();
        manager.register(plugin("service", true, Some(1))).unwrap();
        assert!(matches!(
            manager.register(plugin("service", true, Some(2))),
            Err(PluginError::DuplicatePlugin(_))
        ));
        let composed = manager.compose(&PluginSelection::default()).unwrap();
        assert_eq!(*composed.services.get::<u64>().unwrap(), 1);
    }

    #[test]
    fn contributions_are_multi_bound_in_installation_order() {
        let mut manager = PluginManager::default();
        let mut first = plugin("first", true, None);
        first.contribution = Some("one".into());
        let mut second = plugin("second", true, None);
        second.contribution = Some("two".into());
        manager.register(first).unwrap();
        manager.register(second).unwrap();
        let composed = manager.compose(&PluginSelection::default()).unwrap();
        let values = composed
            .contributions
            .get::<String>()
            .into_iter()
            .map(|value| (*value).clone())
            .collect::<Vec<_>>();
        assert_eq!(values, ["one", "two"]);
        let provenance = composed.registration_provenance();
        assert_eq!(provenance.contributions.len(), 1);
        assert_eq!(
            provenance.contributions[0].rust_type,
            std::any::type_name::<String>()
        );
        assert_eq!(
            provenance.contributions[0]
                .registrations
                .iter()
                .map(|registration| (
                    registration.owner.to_string(),
                    registration.installation_index
                ))
                .collect::<Vec<_>>(),
            [("plugin:first".into(), 0), ("plugin:second".into(), 1)]
        );
    }

    #[test]
    fn application_seeded_service_duplicate_names_both_owners_and_type() {
        let mut manager = PluginManager::default();
        manager
            .register(plugin("plugin-owner", true, Some(2)))
            .unwrap();
        let mut services = ServiceCollection::default();
        services.insert(Arc::new(1_u64)).unwrap();

        let error = manager
            .compose_with(
                &PluginSelection::default(),
                services,
                ContributionCollection::default(),
            )
            .unwrap_err();

        let PluginError::Service(ServiceError::Duplicate(duplicate)) = error else {
            panic!("expected duplicate service error");
        };
        assert_eq!(duplicate.rust_type, std::any::type_name::<u64>());
        assert_eq!(duplicate.first_owner.to_string(), "application");
        assert_eq!(duplicate.attempted_owner.to_string(), "plugin:plugin-owner");
        assert_eq!(
            duplicate.to_string(),
            "u64 (first owner: application, attempted owner: plugin:plugin-owner)"
        );
    }

    #[test]
    fn plugin_duplicate_names_first_and_attempted_plugin_owners() {
        let mut manager = PluginManager::default();
        manager.register(plugin("first", true, Some(1))).unwrap();
        manager.register(plugin("second", true, Some(2))).unwrap();

        let error = manager.compose(&PluginSelection::default()).unwrap_err();

        let PluginError::Service(ServiceError::Duplicate(duplicate)) = error else {
            panic!("expected duplicate service error");
        };
        assert_eq!(duplicate.rust_type, std::any::type_name::<u64>());
        assert_eq!(duplicate.first_owner.to_string(), "plugin:first");
        assert_eq!(duplicate.attempted_owner.to_string(), "plugin:second");
    }

    trait ProvenanceTrait: Send + Sync {
        fn value(&self) -> u64;
    }

    #[derive(Debug)]
    struct ProvenanceTraitValue(u64);

    impl ProvenanceTrait for ProvenanceTraitValue {
        fn value(&self) -> u64 {
            self.0
        }
    }

    #[derive(Debug)]
    struct TraitServicePlugin {
        id: &'static str,
        value: u64,
    }

    impl Plugin for TraitServicePlugin {
        fn descriptor(&self) -> PluginDescriptor {
            let mut descriptor = PluginDescriptor::new(
                PluginId::new(self.id).unwrap(),
                Version::new(1, 0, 0),
                self.id,
            );
            descriptor.default_enabled = true;
            descriptor
        }

        fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
            context
                .services()
                .insert_shared::<dyn ProvenanceTrait>(Arc::new(ProvenanceTraitValue(self.value)))?;
            Ok(())
        }
    }

    #[test]
    fn trait_object_singleton_duplicates_preserve_typed_shared_ownership() {
        let mut manager = PluginManager::default();
        manager
            .register(TraitServicePlugin {
                id: "first-trait",
                value: 1,
            })
            .unwrap();
        manager
            .register(TraitServicePlugin {
                id: "second-trait",
                value: 2,
            })
            .unwrap();

        let error = manager.compose(&PluginSelection::default()).unwrap_err();

        let PluginError::Service(ServiceError::Duplicate(duplicate)) = error else {
            panic!("expected duplicate service error");
        };
        assert_eq!(
            duplicate.rust_type,
            std::any::type_name::<crate::Shared<dyn ProvenanceTrait>>()
        );
        assert_eq!(duplicate.first_owner.to_string(), "plugin:first-trait");
        assert_eq!(duplicate.attempted_owner.to_string(), "plugin:second-trait");
    }

    #[derive(Debug)]
    struct TraitContributionPlugin {
        id: &'static str,
        value: u64,
    }

    impl Plugin for TraitContributionPlugin {
        fn descriptor(&self) -> PluginDescriptor {
            let mut descriptor = PluginDescriptor::new(
                PluginId::new(self.id).unwrap(),
                Version::new(1, 0, 0),
                self.id,
            );
            descriptor.default_enabled = true;
            descriptor
        }

        fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
            context
                .contributions()
                .push_shared::<dyn ProvenanceTrait>(Arc::new(ProvenanceTraitValue(self.value)));
            Ok(())
        }
    }

    #[test]
    fn trait_object_contribution_summaries_preserve_owner_and_global_installation_index() {
        let mut manager = PluginManager::default();
        manager
            .register(TraitContributionPlugin {
                id: "first-trait",
                value: 1,
            })
            .unwrap();
        manager
            .register(TraitContributionPlugin {
                id: "second-trait",
                value: 2,
            })
            .unwrap();
        let mut contributions = ContributionCollection::default();
        contributions.push(Arc::new(String::from("application")));

        let composed = manager
            .compose_with(
                &PluginSelection::default(),
                ServiceCollection::default(),
                contributions,
            )
            .unwrap();
        let values = composed
            .contributions
            .get_shared::<dyn ProvenanceTrait>()
            .into_iter()
            .map(|value| value.value())
            .collect::<Vec<_>>();
        assert_eq!(values, [1, 2]);

        let provenance = composed.registration_provenance();
        let trait_metadata = provenance
            .contributions
            .iter()
            .find(|registration| {
                registration.rust_type
                    == std::any::type_name::<crate::Shared<dyn ProvenanceTrait>>()
            })
            .unwrap();
        assert_eq!(
            trait_metadata
                .registrations
                .iter()
                .map(|registration| (
                    registration.owner.to_string(),
                    registration.installation_index
                ))
                .collect::<Vec<_>>(),
            [
                ("plugin:first-trait".into(), 1),
                ("plugin:second-trait".into(), 2)
            ]
        );
    }

    #[test]
    fn registration_provenance_is_deterministic_across_repeated_composition() {
        let mut manager = PluginManager::default();
        let mut first = plugin("first", true, Some(1));
        first.contribution = Some("one".into());
        manager.register(first).unwrap();

        let first = manager.compose(&PluginSelection::default()).unwrap();
        let second = manager.compose(&PluginSelection::default()).unwrap();
        assert_eq!(
            serde_json::to_string(&first.registration_provenance()).unwrap(),
            serde_json::to_string(&second.registration_provenance()).unwrap()
        );
    }

    #[test]
    fn graph_planning_has_no_registration_provenance_before_composition() {
        #[derive(Debug)]
        struct CountedInstall(Arc<AtomicUsize>);

        impl Plugin for CountedInstall {
            fn descriptor(&self) -> PluginDescriptor {
                let mut descriptor = PluginDescriptor::new(
                    PluginId::new("counted").unwrap(),
                    Version::new(1, 0, 0),
                    "counted",
                );
                descriptor.default_enabled = true;
                descriptor
            }

            fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                context.services().insert(Arc::new(7_u16))?;
                Ok(())
            }
        }

        let installs = Arc::new(AtomicUsize::new(0));
        let mut manager = PluginManager::default();
        manager
            .register(CountedInstall(Arc::clone(&installs)))
            .unwrap();
        let graph = manager.build_graph(&PluginSelection::default()).unwrap();
        assert_eq!(graph.plugins.len(), 1);
        assert_eq!(installs.load(Ordering::SeqCst), 0);

        let composed = manager.compose(&PluginSelection::default()).unwrap();
        assert_eq!(installs.load(Ordering::SeqCst), 1);
        assert_eq!(
            owner_ids(&composed.registration_provenance()),
            ["plugin:counted"]
        );
    }

    #[test]
    fn provenance_json_never_serializes_service_values_or_debug_output() {
        struct SensitiveValue(&'static str);

        impl std::fmt::Debug for SensitiveValue {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(self.0)
            }
        }

        #[derive(Debug)]
        struct SensitivePlugin;

        impl Plugin for SensitivePlugin {
            fn descriptor(&self) -> PluginDescriptor {
                let mut descriptor = PluginDescriptor::new(
                    PluginId::new("sensitive").unwrap(),
                    Version::new(1, 0, 0),
                    "sensitive",
                );
                descriptor.default_enabled = true;
                descriptor
            }

            fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
                context
                    .services()
                    .insert(Arc::new(SensitiveValue("DO_NOT_SERIALIZE")))?;
                Ok(())
            }
        }

        let mut manager = PluginManager::default();
        manager.register(SensitivePlugin).unwrap();
        let composed = manager.compose(&PluginSelection::default()).unwrap();
        let json = serde_json::to_string_pretty(&composed.registration_provenance()).unwrap();

        assert!(json.contains("SensitiveValue"));
        assert!(json.contains("\"plugin_id\": \"sensitive\""));
        assert!(!json.contains("DO_NOT_SERIALIZE"));
    }

    #[test]
    fn plugin_context_cannot_forge_registration_owner_from_service_content() {
        #[derive(Debug)]
        struct ClaimedOwner(&'static str);

        #[derive(Debug)]
        struct ClaimingPlugin;

        impl Plugin for ClaimingPlugin {
            fn descriptor(&self) -> PluginDescriptor {
                let mut descriptor = PluginDescriptor::new(
                    PluginId::new("actual-owner").unwrap(),
                    Version::new(1, 0, 0),
                    "actual owner",
                );
                descriptor.default_enabled = true;
                descriptor
            }

            fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
                context
                    .services()
                    .insert(Arc::new(ClaimedOwner("forged-owner")))?;
                Ok(())
            }
        }

        let mut manager = PluginManager::default();
        manager.register(ClaimingPlugin).unwrap();
        let composed = manager.compose(&PluginSelection::default()).unwrap();
        let provenance = composed.registration_provenance();
        let json = serde_json::to_string(&provenance).unwrap();

        assert_eq!(owner_ids(&provenance), ["plugin:actual-owner"]);
        assert!(!json.contains("forged-owner"));
        assert_eq!(
            composed.services.get::<ClaimedOwner>().unwrap().0,
            "forged-owner"
        );
    }

    #[test]
    fn disabled_plugins_produce_no_registration_provenance() {
        let mut manager = PluginManager::default();
        let mut disabled = plugin("disabled", true, Some(1));
        disabled.contribution = Some("hidden".into());
        manager.register(disabled).unwrap();
        let mut selection = PluginSelection::default();
        selection
            .disabled
            .insert(PluginId::new("disabled").unwrap());

        let composed = manager.compose(&selection).unwrap();

        assert!(composed.registration_provenance().services.is_empty());
        assert!(composed.registration_provenance().contributions.is_empty());
    }

    #[test]
    fn dependency_auto_enabled_plugin_owns_its_registrations() {
        let mut provider = plugin("provider", false, Some(1));
        provider.contribution = Some("provider".into());
        let mut consumer = plugin("consumer", true, None);
        consumer
            .descriptor
            .plugin_dependencies
            .push(PluginId::new("provider").unwrap());
        let mut manager = PluginManager::default();
        manager.register(provider).unwrap();
        manager.register(consumer).unwrap();

        let composed = manager.compose(&PluginSelection::default()).unwrap();
        let provenance = composed.registration_provenance();

        assert_eq!(owner_ids(&provenance), ["plugin:provider"]);
        assert_eq!(
            provenance.contributions[0].registrations[0]
                .owner
                .to_string(),
            "plugin:provider"
        );
    }

    #[test]
    fn failed_composition_does_not_retain_a_partially_frozen_application() {
        #[derive(Debug)]
        struct ApplicationProbe;

        let probe = Arc::new(ApplicationProbe);
        let mut services = ServiceCollection::default();
        services.insert(Arc::clone(&probe)).unwrap();
        let mut manager = PluginManager::default();
        manager.register(plugin("first", true, Some(1))).unwrap();
        manager.register(plugin("second", true, Some(2))).unwrap();

        let result = manager.compose_with(
            &PluginSelection::default(),
            services,
            ContributionCollection::default(),
        );

        assert!(result.is_err());
        assert_eq!(Arc::strong_count(&probe), 1);
    }

    #[test]
    fn unknown_runtime_selection_fails_closed() {
        let manager = PluginManager::default();
        let mut selection = PluginSelection::default();
        selection.enabled.insert(PluginId::new("missing").unwrap());
        assert!(matches!(
            manager.compose(&selection),
            Err(PluginError::UnknownPlugin(_))
        ));
    }

    #[test]
    fn contradictory_runtime_selection_fails_closed() {
        let mut manager = PluginManager::default();
        manager.register(plugin("example", false, None)).unwrap();
        let id = PluginId::new("example").unwrap();
        let mut selection = PluginSelection::default();
        selection.enabled.insert(id.clone());
        selection.disabled.insert(id);
        assert!(matches!(
            manager.compose(&selection),
            Err(PluginError::ContradictorySelection(_))
        ));
    }

    #[test]
    fn explicit_plugin_install_exposes_typed_service() {
        let mut manager = PluginManager::default();
        manager
            .register(plugin("service", false, Some(42)))
            .unwrap();
        let mut selection = PluginSelection::default();
        selection.enabled.insert(PluginId::new("service").unwrap());
        let composed = manager.compose(&selection).unwrap();
        assert_eq!(*composed.services.get::<u64>().unwrap(), 42);
    }

    #[test]
    fn application_services_and_contributions_can_be_injected_before_plugins_install() {
        #[derive(Debug)]
        struct DependsOnApplicationState;

        impl Plugin for DependsOnApplicationState {
            fn descriptor(&self) -> PluginDescriptor {
                let mut descriptor = PluginDescriptor::new(
                    PluginId::new("depends-on-app").unwrap(),
                    Version::new(1, 0, 0),
                    "depends on composition-root state",
                );
                descriptor.default_enabled = true;
                descriptor
            }

            fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
                let value = context.services().get::<String>()?;
                context
                    .contributions()
                    .push(Arc::new(format!("plugin:{value}")));
                Ok(())
            }
        }

        let mut manager = PluginManager::default();
        manager.register(DependsOnApplicationState).unwrap();
        let mut services = ServiceCollection::default();
        services.insert(Arc::new("application".to_owned())).unwrap();
        let mut contributions = ContributionCollection::default();
        contributions.push(Arc::new("application:base".to_owned()));

        let composed = manager
            .compose_with(&PluginSelection::default(), services, contributions)
            .unwrap();
        assert_eq!(
            composed
                .contributions
                .get::<String>()
                .into_iter()
                .map(|value| (*value).clone())
                .collect::<Vec<_>>(),
            ["application:base", "plugin:application"]
        );
    }

    #[test]
    fn incompatible_core_requirement_is_rejected_during_registration() {
        let mut incompatible = plugin("future", false, None);
        incompatible.descriptor.core_compatibility = VersionReq::parse(">=99").unwrap();
        assert!(matches!(
            PluginManager::default().register(incompatible),
            Err(PluginError::IncompatibleCore { .. })
        ));
    }

    #[derive(Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
    struct ExampleConfiguration {
        message: String,
    }

    #[derive(Debug)]
    struct ConfiguredPlugin;

    impl Plugin for ConfiguredPlugin {
        fn descriptor(&self) -> PluginDescriptor {
            let mut descriptor = PluginDescriptor::new(
                PluginId::new("configured").unwrap(),
                Version::new(1, 0, 0),
                "configured",
            );
            descriptor.default_enabled = true;
            descriptor
        }

        fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
            let configuration = context.configuration::<ExampleConfiguration>()?;
            context.services().insert(Arc::new(configuration))?;
            Ok(())
        }
    }

    #[test]
    fn typed_plugin_configuration_is_available_during_installation() {
        let mut manager = PluginManager::default();
        manager.register(ConfiguredPlugin).unwrap();
        let id = PluginId::new("configured").unwrap();
        let mut selection = PluginSelection::default();
        selection
            .set_configuration(
                id,
                &ExampleConfiguration {
                    message: "hello".into(),
                },
            )
            .unwrap();
        let composed = manager.compose(&selection).unwrap();
        assert_eq!(
            composed
                .services
                .get::<ExampleConfiguration>()
                .unwrap()
                .message,
            "hello"
        );
    }

    #[derive(Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
    struct SchemaConfiguration {
        name: String,
        enabled: bool,
        alias: Option<String>,
    }

    #[derive(Debug)]
    struct SchemaPlugin;

    impl Plugin for SchemaPlugin {
        fn descriptor(&self) -> PluginDescriptor {
            let mut descriptor = PluginDescriptor::new(
                PluginId::new("schema").unwrap(),
                Version::new(1, 0, 0),
                "schema",
            );
            descriptor.default_enabled = true;
            descriptor.configuration.extend([
                ConfigurationField {
                    key: "name".into(),
                    kind: ConfigurationValueKind::String,
                    required: true,
                    secret: false,
                    description: "name".into(),
                    default: None,
                },
                ConfigurationField {
                    key: "enabled".into(),
                    kind: ConfigurationValueKind::Boolean,
                    required: false,
                    secret: false,
                    description: "enabled".into(),
                    default: Some(serde_json::json!(true)),
                },
                ConfigurationField {
                    key: "alias".into(),
                    kind: ConfigurationValueKind::String,
                    required: false,
                    secret: false,
                    description: "optional alias".into(),
                    default: None,
                },
            ]);
            descriptor
        }

        fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
            let configuration = context.configuration::<SchemaConfiguration>()?;
            context.services().insert(Arc::new(configuration))?;
            Ok(())
        }
    }

    #[derive(Debug)]
    struct ConditionalDependencyPlugin;

    impl Plugin for ConditionalDependencyPlugin {
        fn descriptor(&self) -> PluginDescriptor {
            let mut descriptor = PluginDescriptor::new(
                PluginId::new("conditional").unwrap(),
                Version::new(1, 0, 0),
                "configuration-dependent dependency",
            );
            descriptor.default_enabled = true;
            descriptor.configuration.push(ConfigurationField {
                key: "use-provider".into(),
                kind: ConfigurationValueKind::Boolean,
                required: false,
                secret: false,
                description: "enable the provider dependency".into(),
                default: Some(serde_json::json!(false)),
            });
            descriptor
        }

        fn configure_descriptor(
            &self,
            descriptor: &mut PluginDescriptor,
            configuration: Option<&serde_json::Value>,
        ) -> Result<(), PluginError> {
            if configuration
                .and_then(|value| value.get("use-provider"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                descriptor
                    .plugin_dependencies
                    .push(PluginId::new("provider").unwrap());
                descriptor.requires.push(crate::CapabilityRequirement {
                    name: "conditional.provider".into(),
                    version: VersionReq::parse("^1").unwrap(),
                });
            }
            Ok(())
        }

        fn install(&self, _context: &mut PluginContext<'_>) -> Result<(), PluginError> {
            Ok(())
        }
    }

    #[test]
    fn configured_dependencies_participate_in_resolution_and_graph_validation() {
        let mut provider = plugin("provider", false, None);
        provider
            .descriptor
            .provides
            .push(crate::CapabilityProvision {
                name: "conditional.provider".into(),
                version: Version::new(1, 0, 0),
            });
        let mut manager = PluginManager::default();
        manager.register(provider).unwrap();
        manager.register(ConditionalDependencyPlugin).unwrap();

        let mut selection = PluginSelection::default();
        selection.configuration.insert(
            PluginId::new("conditional").unwrap(),
            serde_json::json!({"use-provider": true}),
        );
        let application = manager.compose(&selection).unwrap();
        let ids = application
            .graph
            .plugins
            .iter()
            .map(|plugin| plugin.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, ["provider", "conditional"]);
    }

    #[test]
    fn published_configuration_schema_applies_defaults() {
        let mut manager = PluginManager::default();
        manager.register(SchemaPlugin).unwrap();
        let mut selection = PluginSelection::default();
        selection.configuration.insert(
            PluginId::new("schema").unwrap(),
            serde_json::json!({ "name": "feedback" }),
        );

        let composed = manager.compose(&selection).unwrap();
        assert_eq!(
            *composed.services.get::<SchemaConfiguration>().unwrap(),
            SchemaConfiguration {
                name: "feedback".into(),
                enabled: true,
                alias: None,
            }
        );
    }

    #[test]
    fn typed_optional_none_is_normalized_as_an_absent_field() {
        let mut manager = PluginManager::default();
        manager.register(SchemaPlugin).unwrap();
        let id = PluginId::new("schema").unwrap();
        let mut selection = PluginSelection::default();
        selection
            .set_configuration(
                id,
                &SchemaConfiguration {
                    name: "feedback".into(),
                    enabled: true,
                    alias: None,
                },
            )
            .unwrap();

        let composed = manager.compose(&selection).unwrap();
        assert_eq!(
            *composed.services.get::<SchemaConfiguration>().unwrap(),
            SchemaConfiguration {
                name: "feedback".into(),
                enabled: true,
                alias: None,
            }
        );
    }

    #[test]
    fn published_configuration_schema_rejects_unknown_missing_and_mistyped_fields() {
        let mut manager = PluginManager::default();
        manager.register(SchemaPlugin).unwrap();
        let id = PluginId::new("schema").unwrap();

        for (configuration, expected) in [
            (
                serde_json::json!({ "name": "feedback", "unknown": true }),
                "unknown configuration field",
            ),
            (
                serde_json::json!({}),
                "missing required configuration field",
            ),
            (
                serde_json::json!({ "name": "feedback", "enabled": "yes" }),
                "must be Boolean",
            ),
        ] {
            let mut selection = PluginSelection::default();
            selection.configuration.insert(id.clone(), configuration);
            let error = manager.compose(&selection).unwrap_err().to_string();
            assert!(error.contains(expected), "{error}");
        }
    }

    #[test]
    fn graph_planning_never_installs_plugin_services() {
        #[derive(Debug)]
        struct PlanningOnly;

        impl Plugin for PlanningOnly {
            fn descriptor(&self) -> PluginDescriptor {
                let mut descriptor = PluginDescriptor::new(
                    PluginId::new("planning-only").unwrap(),
                    Version::new(1, 0, 0),
                    "planning only",
                );
                descriptor.default_enabled = true;
                descriptor
            }

            fn install(&self, _context: &mut PluginContext<'_>) -> Result<(), PluginError> {
                panic!("planning must not install services");
            }
        }

        let mut manager = PluginManager::default();
        manager.register(PlanningOnly).unwrap();

        let graph = manager.build_graph(&PluginSelection::default()).unwrap();

        assert_eq!(graph.plugins[0].id.as_str(), "planning-only");
    }

    #[test]
    fn secret_configuration_fields_cannot_publish_default_values() {
        #[derive(Debug)]
        struct UnsafeSecretDefault;

        impl Plugin for UnsafeSecretDefault {
            fn descriptor(&self) -> PluginDescriptor {
                let mut descriptor = PluginDescriptor::new(
                    PluginId::new("unsafe-secret").unwrap(),
                    Version::new(1, 0, 0),
                    "unsafe secret",
                );
                descriptor.configuration.push(ConfigurationField {
                    key: "api_token".into(),
                    kind: ConfigurationValueKind::String,
                    required: true,
                    secret: true,
                    description: "provider API token".into(),
                    default: Some(serde_json::json!("must-not-leak")),
                });
                descriptor
            }

            fn install(&self, _context: &mut PluginContext<'_>) -> Result<(), PluginError> {
                Ok(())
            }
        }

        let error = PluginManager::default()
            .register(UnsafeSecretDefault)
            .unwrap_err()
            .to_string();

        assert!(error.contains("secret configuration fields cannot have defaults"));
    }
}
