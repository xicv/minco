use crate::{ApplicationGraph, FrozenServices, GraphBuilder, GraphError, PluginDescriptor, PluginId, ServiceCollection, ServiceError};
use serde::{Deserialize, Serialize};
use std::{collections::{BTreeMap, BTreeSet}, sync::Arc};
use thiserror::Error;

pub trait Plugin: Send + Sync + 'static {
    fn descriptor(&self) -> PluginDescriptor;
    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError>;
}

#[derive(Debug)]
pub struct PluginContext<'a> {
    services: &'a mut ServiceCollection,
}

impl<'a> PluginContext<'a> {
    pub fn services(&mut self) -> &mut ServiceCollection {
        self.services
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginSelection {
    #[serde(default)]
    pub enabled: BTreeSet<PluginId>,
    #[serde(default)]
    pub disabled: BTreeSet<PluginId>,
}

impl PluginSelection {
    pub fn is_enabled(&self, descriptor: &PluginDescriptor) -> bool {
        if self.disabled.contains(&descriptor.id) {
            return false;
        }
        self.enabled.contains(&descriptor.id) || descriptor.default_enabled
    }
}

pub struct PluginManager {
    plugins: BTreeMap<PluginId, Arc<dyn Plugin>>,
}

impl std::fmt::Debug for PluginManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("PluginManager").field("plugin_ids", &self.plugins.keys()).finish()
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self { plugins: BTreeMap::new() }
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
        let id = plugin.descriptor().id;
        if self.plugins.contains_key(&id) {
            return Err(PluginError::DuplicatePlugin(id));
        }
        self.plugins.insert(id, plugin);
        Ok(())
    }

    pub fn compose(self, selection: &PluginSelection) -> Result<ComposedApplication, PluginError> {
        let enabled = self.resolve_enabled(selection)?;
        let ordered = topological_order(&enabled)?;
        let mut services = ServiceCollection::default();
        let mut graph = GraphBuilder::default();
        for id in ordered {
            let plugin = enabled.get(&id).ok_or_else(|| PluginError::UnknownPlugin(id.clone()))?;
            let descriptor = plugin.descriptor();
            plugin.install(&mut PluginContext { services: &mut services })?;
            graph.add_plugin(descriptor);
        }
        Ok(ComposedApplication {
            graph: graph.build()?,
            services: services.freeze(),
        })
    }

    fn resolve_enabled(&self, selection: &PluginSelection) -> Result<BTreeMap<PluginId, Arc<dyn Plugin>>, PluginError> {
        for selected in selection.enabled.iter().chain(&selection.disabled) {
            if !self.plugins.contains_key(selected) {
                return Err(PluginError::UnknownPlugin(selected.clone()));
            }
        }
        let mut enabled = BTreeMap::new();
        for (id, plugin) in &self.plugins {
            let descriptor = plugin.descriptor();
            if selection.is_enabled(&descriptor) {
                enabled.insert(id.clone(), Arc::clone(plugin));
            }
        }
        let mut changed = true;
        while changed {
            changed = false;
            let descriptors: Vec<_> = enabled.values().map(|plugin| plugin.descriptor()).collect();
            for descriptor in descriptors {
                for dependency in descriptor.plugin_dependencies {
                    if selection.disabled.contains(&dependency) {
                        return Err(PluginError::DisabledRequiredPlugin {
                            plugin: descriptor.id.clone(),
                            dependency,
                        });
                    }
                    if !enabled.contains_key(&dependency) {
                        let plugin = self.plugins.get(&dependency).ok_or_else(|| PluginError::MissingPluginDependency {
                            plugin: descriptor.id.clone(),
                            dependency: dependency.clone(),
                        })?;
                        enabled.insert(dependency, Arc::clone(plugin));
                        changed = true;
                    }
                }
            }
        }
        Ok(enabled)
    }
}

fn topological_order(plugins: &BTreeMap<PluginId, Arc<dyn Plugin>>) -> Result<Vec<PluginId>, PluginError> {
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
    plugins: &BTreeMap<PluginId, Arc<dyn Plugin>>,
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
    let plugin = plugins.get(id).ok_or_else(|| PluginError::UnknownPlugin(id.clone()))?;
    for dependency in plugin.descriptor().plugin_dependencies {
        visit(&dependency, plugins, visiting, visited, ordered)?;
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
}

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("duplicate plugin registration: {0}")]
    DuplicatePlugin(PluginId),
    #[error("unknown plugin: {0}")]
    UnknownPlugin(PluginId),
    #[error("plugin {plugin} depends on unregistered plugin {dependency}")]
    MissingPluginDependency { plugin: PluginId, dependency: PluginId },
    #[error("plugin {plugin} requires disabled plugin {dependency}")]
    DisabledRequiredPlugin { plugin: PluginId, dependency: PluginId },
    #[error("plugin dependency cycle includes {0}")]
    DependencyCycle(PluginId),
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
    use semver::Version;

    #[derive(Debug)]
    struct TestPlugin { descriptor: PluginDescriptor, value: Option<u64> }
    impl Plugin for TestPlugin {
        fn descriptor(&self) -> PluginDescriptor { self.descriptor.clone() }
        fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
            if let Some(value) = self.value { context.services().insert(Arc::new(value))?; }
            Ok(())
        }
    }
    fn plugin(id: &str, default_enabled: bool, value: Option<u64>) -> TestPlugin {
        let mut descriptor = PluginDescriptor::new(PluginId::new(id).unwrap(), Version::new(1,0,0), id);
        descriptor.default_enabled = default_enabled;
        TestPlugin { descriptor, value }
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
    fn explicit_plugin_install_exposes_typed_service() {
        let mut manager = PluginManager::default();
        manager.register(plugin("service", false, Some(42))).unwrap();
        let mut selection = PluginSelection::default();
        selection.enabled.insert(PluginId::new("service").unwrap());
        let composed = manager.compose(&selection).unwrap();
        assert_eq!(*composed.services.get::<u64>().unwrap(), 42);
    }
}
