use crate::{
    CapabilityRequirement, HealthCheckDescriptor, MigrationSet, OperationDescriptor,
    PluginDescriptor, PluginId, ResourceIntent,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationGraph {
    pub plugins: Vec<PluginDescriptor>,
    pub capabilities: BTreeMap<String, Version>,
    pub operations: BTreeMap<String, OperationDescriptor>,
    pub migrations: BTreeMap<String, MigrationSet>,
    pub health_checks: BTreeMap<String, HealthCheckDescriptor>,
    pub resources: BTreeMap<String, ResourceIntent>,
}

#[derive(Debug, Default)]
pub struct GraphBuilder {
    plugins: Vec<PluginDescriptor>,
}

impl GraphBuilder {
    pub fn add_plugin(&mut self, descriptor: PluginDescriptor) {
        self.plugins.push(descriptor);
    }

    pub fn build(self) -> Result<ApplicationGraph, GraphError> {
        let mut ids = BTreeSet::new();
        let mut capabilities = BTreeMap::<String, Version>::new();
        let mut operations = BTreeMap::new();
        let mut routes = BTreeMap::<(String, String), String>::new();
        let mut migrations = BTreeMap::new();
        let mut health_checks = BTreeMap::new();
        let mut resources = BTreeMap::new();

        for plugin in &self.plugins {
            if !ids.insert(plugin.id.clone()) {
                return Err(GraphError::DuplicatePlugin(plugin.id.clone()));
            }
            for capability in &plugin.provides {
                if capabilities
                    .insert(capability.name.clone(), capability.version.clone())
                    .is_some()
                {
                    return Err(GraphError::DuplicateCapability(capability.name.clone()));
                }
            }
            for operation in &plugin.operations {
                if operations
                    .insert(operation.operation_id.clone(), operation.clone())
                    .is_some()
                {
                    return Err(GraphError::DuplicateOperation(
                        operation.operation_id.clone(),
                    ));
                }
                let route = (
                    operation.method.to_ascii_uppercase(),
                    operation.path.clone(),
                );
                if let Some(existing) = routes.insert(route.clone(), operation.operation_id.clone())
                {
                    return Err(GraphError::DuplicateRoute {
                        method: route.0,
                        path: route.1,
                        first_operation: existing,
                        second_operation: operation.operation_id.clone(),
                    });
                }
            }
            for migration in &plugin.migrations {
                if migrations
                    .insert(migration.id.clone(), migration.clone())
                    .is_some()
                {
                    return Err(GraphError::DuplicateMigration(migration.id.clone()));
                }
            }
            for check in &plugin.health_checks {
                if health_checks
                    .insert(check.id.clone(), check.clone())
                    .is_some()
                {
                    return Err(GraphError::DuplicateHealthCheck(check.id.clone()));
                }
            }
            for resource in &plugin.resources {
                if resources
                    .insert(resource.id.clone(), resource.clone())
                    .is_some()
                {
                    return Err(GraphError::DuplicateResource(resource.id.clone()));
                }
            }
        }

        for plugin in &self.plugins {
            validate_plugin_dependencies(plugin, &ids)?;
            validate_capability_requirements(plugin, &capabilities)?;
        }
        validate_plugin_cycles(&self.plugins)?;
        validate_resource_dependencies(&resources)?;
        validate_resource_cycles(&resources)?;

        Ok(ApplicationGraph {
            plugins: self.plugins,
            capabilities,
            operations,
            migrations,
            health_checks,
            resources,
        })
    }
}

fn validate_plugin_dependencies(
    plugin: &PluginDescriptor,
    ids: &BTreeSet<PluginId>,
) -> Result<(), GraphError> {
    for dependency in &plugin.plugin_dependencies {
        if !ids.contains(dependency) {
            return Err(GraphError::MissingPluginDependency {
                plugin: plugin.id.clone(),
                dependency: dependency.clone(),
            });
        }
    }
    Ok(())
}

fn validate_capability_requirements(
    plugin: &PluginDescriptor,
    capabilities: &BTreeMap<String, Version>,
) -> Result<(), GraphError> {
    for CapabilityRequirement { name, version } in &plugin.requires {
        let provided = capabilities
            .get(name)
            .ok_or_else(|| GraphError::MissingCapability {
                plugin: plugin.id.clone(),
                capability: name.clone(),
            })?;
        if !version.matches(provided) {
            return Err(GraphError::CapabilityVersionMismatch {
                plugin: plugin.id.clone(),
                capability: name.clone(),
                required: version.to_string(),
                provided: provided.to_string(),
            });
        }
    }
    Ok(())
}

fn validate_plugin_cycles(plugins: &[PluginDescriptor]) -> Result<(), GraphError> {
    let graph: BTreeMap<PluginId, Vec<PluginId>> = plugins
        .iter()
        .map(|plugin| (plugin.id.clone(), plugin.plugin_dependencies.clone()))
        .collect();
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for id in graph.keys() {
        visit_plugin(id, &graph, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn visit_plugin(
    id: &PluginId,
    graph: &BTreeMap<PluginId, Vec<PluginId>>,
    visiting: &mut BTreeSet<PluginId>,
    visited: &mut BTreeSet<PluginId>,
) -> Result<(), GraphError> {
    if visited.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id.clone()) {
        return Err(GraphError::PluginCycle(id.clone()));
    }
    for dependency in graph.get(id).into_iter().flatten() {
        visit_plugin(dependency, graph, visiting, visited)?;
    }
    visiting.remove(id);
    visited.insert(id.clone());
    Ok(())
}

fn validate_resource_dependencies(
    resources: &BTreeMap<String, ResourceIntent>,
) -> Result<(), GraphError> {
    for resource in resources.values() {
        for dependency in &resource.dependencies {
            if !resources.contains_key(dependency) {
                return Err(GraphError::MissingResourceDependency {
                    resource: resource.id.clone(),
                    dependency: dependency.clone(),
                });
            }
        }
    }
    Ok(())
}

fn validate_resource_cycles(
    resources: &BTreeMap<String, ResourceIntent>,
) -> Result<(), GraphError> {
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for id in resources.keys() {
        visit_resource(id, resources, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn visit_resource(
    id: &str,
    resources: &BTreeMap<String, ResourceIntent>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
) -> Result<(), GraphError> {
    if visited.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id.to_owned()) {
        return Err(GraphError::ResourceCycle(id.to_owned()));
    }
    if let Some(resource) = resources.get(id) {
        for dependency in &resource.dependencies {
            visit_resource(dependency, resources, visiting, visited)?;
        }
    }
    visiting.remove(id);
    visited.insert(id.to_owned());
    Ok(())
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GraphError {
    #[error("duplicate plugin id: {0}")]
    DuplicatePlugin(PluginId),
    #[error("duplicate provided capability: {0}")]
    DuplicateCapability(String),
    #[error("duplicate operation id: {0}")]
    DuplicateOperation(String),
    #[error("route {method} {path} is bound by both {first_operation} and {second_operation}")]
    DuplicateRoute {
        method: String,
        path: String,
        first_operation: String,
        second_operation: String,
    },
    #[error("duplicate migration id: {0}")]
    DuplicateMigration(String),
    #[error("duplicate health check id: {0}")]
    DuplicateHealthCheck(String),
    #[error("duplicate resource id: {0}")]
    DuplicateResource(String),
    #[error("plugin {plugin} depends on missing plugin {dependency}")]
    MissingPluginDependency {
        plugin: PluginId,
        dependency: PluginId,
    },
    #[error("plugin dependency cycle includes {0}")]
    PluginCycle(PluginId),
    #[error("plugin {plugin} requires missing capability {capability}")]
    MissingCapability {
        plugin: PluginId,
        capability: String,
    },
    #[error("plugin {plugin} requires {capability} {required}, but {provided} is provided")]
    CapabilityVersionMismatch {
        plugin: PluginId,
        capability: String,
        required: String,
        provided: String,
    },
    #[error("resource {resource} depends on missing resource {dependency}")]
    MissingResourceDependency {
        resource: String,
        dependency: String,
    },
    #[error("resource dependency cycle includes {0}")]
    ResourceCycle(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CapabilityProvision, PluginDescriptor, PluginId};
    use semver::{Version, VersionReq};

    fn descriptor(id: &str) -> PluginDescriptor {
        PluginDescriptor::new(PluginId::new(id).unwrap(), Version::new(1, 0, 0), id)
    }

    #[test]
    fn validates_capabilities_and_dependencies() {
        let mut provider = descriptor("provider");
        provider.provides.push(CapabilityProvision {
            name: "clock".into(),
            version: Version::new(1, 2, 0),
        });
        let mut consumer = descriptor("consumer");
        consumer.plugin_dependencies.push(provider.id.clone());
        consumer.requires.push(CapabilityRequirement {
            name: "clock".into(),
            version: VersionReq::parse("^1.0").unwrap(),
        });
        let mut builder = GraphBuilder::default();
        builder.add_plugin(provider);
        builder.add_plugin(consumer);
        assert!(builder.build().is_ok());
    }

    #[test]
    fn rejects_duplicate_method_and_path_bindings() {
        let mut first = descriptor("first");
        first.operations.push(OperationDescriptor {
            operation_id: "firstOperation".into(),
            method: "GET".into(),
            path: "/items".into(),
            public: true,
            idempotent: false,
        });
        let mut second = descriptor("second");
        second.operations.push(OperationDescriptor {
            operation_id: "secondOperation".into(),
            method: "get".into(),
            path: "/items".into(),
            public: true,
            idempotent: false,
        });
        let mut builder = GraphBuilder::default();
        builder.add_plugin(first);
        builder.add_plugin(second);
        assert!(matches!(
            builder.build(),
            Err(GraphError::DuplicateRoute { .. })
        ));
    }

    #[test]
    fn rejects_resource_dependency_cycles() {
        let mut plugin = descriptor("resources");
        plugin.resources.push(ResourceIntent {
            id: "first".into(),
            kind: crate::ResourceKind::Custom("first".into()),
            idle_cost: crate::IdleCostClass::ZeroCompute,
            wake_sources: Vec::new(),
            dependencies: vec!["second".into()],
        });
        plugin.resources.push(ResourceIntent {
            id: "second".into(),
            kind: crate::ResourceKind::Custom("second".into()),
            idle_cost: crate::IdleCostClass::ZeroCompute,
            wake_sources: Vec::new(),
            dependencies: vec!["first".into()],
        });
        let mut builder = GraphBuilder::default();
        builder.add_plugin(plugin);
        assert!(matches!(builder.build(), Err(GraphError::ResourceCycle(_))));
    }

    #[test]
    fn rejects_missing_capability() {
        let mut consumer = descriptor("consumer");
        consumer.requires.push(CapabilityRequirement {
            name: "clock".into(),
            version: VersionReq::STAR,
        });
        let mut builder = GraphBuilder::default();
        builder.add_plugin(consumer);
        assert!(matches!(
            builder.build(),
            Err(GraphError::MissingCapability { .. })
        ));
    }
}
