use crate::middleware::{HttpRuntimeConfig, apply_standard_middleware};
use axum::Router;
use minco_core::{ApplicationGraph, FrozenContributions, PluginContext, PluginId};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use thiserror::Error;

/// One fully state-bound Axum router contributed by a statically linked plugin.
///
/// `minco-core` remains independent of Axum. HTTP-aware plugins contribute this
/// type through [`PluginContext::contributions`], and the application composition
/// root validates and merges all modules after plugin installation.
#[derive(Clone)]
pub struct HttpModule {
    pub plugin_id: PluginId,
    pub router: Router,
    /// OpenAPI/descriptor operation IDs implemented by this router fragment.
    ///
    /// Minco validates the union of these IDs against the operations declared by
    /// the plugin descriptor. The field does not attempt to introspect Axum's
    /// router internals; ownership remains explicit and machine-readable.
    pub operation_ids: BTreeSet<String>,
    /// Largest request body accepted by any route in this module.
    ///
    /// The application composition root uses this value to prevent a global
    /// Tower body limit from accidentally rejecting a route-specific upload
    /// limit. Individual routes must still configure their own smaller limits.
    pub max_request_body_bytes: Option<usize>,
}

impl HttpModule {
    pub const fn new(plugin_id: PluginId, router: Router) -> Self {
        Self {
            plugin_id,
            router,
            operation_ids: BTreeSet::new(),
            max_request_body_bytes: None,
        }
    }

    /// Declares the contract operations implemented by this module.
    #[must_use]
    pub fn with_operations<I, S>(mut self, operation_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.operation_ids = operation_ids.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub const fn with_max_request_body_bytes(mut self, maximum: usize) -> Self {
        self.max_request_body_bytes = Some(maximum);
        self
    }

    /// Registers this module in deterministic plugin-installation order.
    pub fn contribute(self, context: &mut PluginContext<'_>) {
        context.contributions().push(Arc::new(self));
    }
}

impl std::fmt::Debug for HttpModule {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HttpModule")
            .field("plugin_id", &self.plugin_id)
            .field("operation_ids", &self.operation_ids)
            .field("max_request_body_bytes", &self.max_request_body_bytes)
            .finish_non_exhaustive()
    }
}

/// Verifies that HTTP contributions and plugin descriptors have an exact
/// operation-ID correspondence.
///
/// This closes the drift boundary between a plugin's contract/deployment graph
/// and its delivery module. Route method/path duplication is validated by
/// `minco-core`; this function proves that every declared operation is owned by
/// exactly one installed HTTP module and that no undeclared operation is exposed.
pub fn validate_plugin_http_modules(
    graph: &ApplicationGraph,
    contributions: &FrozenContributions,
) -> Result<(), HttpCompositionError> {
    let expected = graph
        .plugins
        .iter()
        .map(|plugin| {
            (
                plugin.id.clone(),
                plugin
                    .operations
                    .iter()
                    .map(|operation| operation.operation_id.clone())
                    .collect::<BTreeSet<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let mut actual = BTreeMap::<PluginId, BTreeSet<String>>::new();
    let mut owners = BTreeMap::<String, PluginId>::new();

    for module in contributions.get::<HttpModule>() {
        if !expected.contains_key(&module.plugin_id) {
            return Err(HttpCompositionError::UnknownPlugin(
                module.plugin_id.clone(),
            ));
        }
        let plugin_operations = actual.entry(module.plugin_id.clone()).or_default();
        for operation_id in &module.operation_ids {
            if !plugin_operations.insert(operation_id.clone()) {
                return Err(HttpCompositionError::DuplicateModuleOperation {
                    plugin: module.plugin_id.clone(),
                    operation_id: operation_id.clone(),
                });
            }
            if let Some(first) = owners.insert(operation_id.clone(), module.plugin_id.clone()) {
                return Err(HttpCompositionError::OperationOwnedByMultiplePlugins {
                    operation_id: operation_id.clone(),
                    first,
                    second: module.plugin_id.clone(),
                });
            }
        }
    }

    for (plugin, expected_operations) in expected {
        let actual_operations = actual.remove(&plugin).unwrap_or_default();
        if expected_operations != actual_operations {
            let missing = expected_operations
                .difference(&actual_operations)
                .cloned()
                .collect();
            let undeclared = actual_operations
                .difference(&expected_operations)
                .cloned()
                .collect();
            return Err(HttpCompositionError::OperationMismatch {
                plugin,
                missing,
                undeclared,
            });
        }
    }

    Ok(())
}

/// Merges every plugin-contributed router in deterministic installation order.
///
/// Call [`validate_plugin_http_modules`] first, or use [`compose_plugin_http`],
/// when the modules expose contract operations.
pub fn merge_plugin_http_modules(
    mut router: Router,
    contributions: &FrozenContributions,
) -> Router {
    for module in contributions.get::<HttpModule>() {
        router = router.merge(module.router.clone());
    }
    router
}

/// Returns the global request-body ceiling required by all installed HTTP modules.
///
/// This is intentionally the maximum, not a replacement for route-level limits.
/// It keeps the global middleware compatible with upload-capable plugins while
/// allowing ordinary JSON routes to retain stricter extractor limits.
#[must_use]
pub fn required_request_body_bytes(baseline: usize, contributions: &FrozenContributions) -> usize {
    contributions
        .get::<HttpModule>()
        .into_iter()
        .filter_map(|module| module.max_request_body_bytes)
        .fold(baseline, usize::max)
}

/// Validates and merges plugin routes, then applies Minco's standard middleware
/// with an automatically expanded global body ceiling.
pub fn compose_plugin_http(
    router: Router,
    configuration: &HttpRuntimeConfig,
    graph: &ApplicationGraph,
    contributions: &FrozenContributions,
) -> Result<Router, HttpCompositionError> {
    validate_plugin_http_modules(graph, contributions)?;
    let mut effective = configuration.clone();
    effective.max_request_body_bytes =
        required_request_body_bytes(configuration.max_request_body_bytes, contributions);
    apply_standard_middleware(merge_plugin_http_modules(router, contributions), &effective)
        .map_err(HttpCompositionError::InvalidHeaderValue)
}

#[derive(Debug, Error)]
pub enum HttpCompositionError {
    #[error("HTTP module references plugin that is not in the application graph: {0}")]
    UnknownPlugin(PluginId),
    #[error("plugin {plugin} contributes operation {operation_id} more than once")]
    DuplicateModuleOperation {
        plugin: PluginId,
        operation_id: String,
    },
    #[error("operation {operation_id} is contributed by both plugin {first} and plugin {second}")]
    OperationOwnedByMultiplePlugins {
        operation_id: String,
        first: PluginId,
        second: PluginId,
    },
    #[error(
        "HTTP operations for plugin {plugin} do not match its descriptor; missing={missing:?}, undeclared={undeclared:?}"
    )]
    OperationMismatch {
        plugin: PluginId,
        missing: BTreeSet<String>,
        undeclared: BTreeSet<String>,
    },
    #[error("invalid HTTP middleware header configuration: {0}")]
    InvalidHeaderValue(#[source] http::header::InvalidHeaderValue),
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, routing::get};
    use minco_core::{ContributionCollection, GraphBuilder, OperationDescriptor, PluginDescriptor};
    use semver::Version;
    use tower::ServiceExt;

    fn graph_with_operations(plugin_id: &str, operation_ids: &[&str]) -> ApplicationGraph {
        let id = PluginId::new(plugin_id).unwrap();
        let mut descriptor = PluginDescriptor::new(id, Version::new(1, 0, 0), "test HTTP plugin");
        descriptor
            .operations
            .extend(
                operation_ids
                    .iter()
                    .map(|operation_id| OperationDescriptor {
                        operation_id: (*operation_id).to_owned(),
                        method: "GET".to_owned(),
                        path: format!("/{operation_id}"),
                        public: true,
                        idempotent: false,
                    }),
            );
        let mut builder = GraphBuilder::default();
        builder.add_plugin(descriptor);
        builder.build().unwrap()
    }

    #[tokio::test]
    async fn plugin_routers_are_merged_from_ordered_contributions() {
        let mut contributions = ContributionCollection::default();
        contributions.push(Arc::new(
            HttpModule::new(
                PluginId::new("first").unwrap(),
                Router::new().route("/first", get(|| async { "first" })),
            )
            .with_operations(["firstOperation"]),
        ));
        contributions.push(Arc::new(
            HttpModule::new(
                PluginId::new("second").unwrap(),
                Router::new().route("/second", get(|| async { "second" })),
            )
            .with_operations(["secondOperation"]),
        ));
        let router = merge_plugin_http_modules(Router::new(), &contributions.freeze());

        for path in ["/first", "/second"] {
            let response = router
                .clone()
                .oneshot(http::Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert!(response.status().is_success(), "{path}");
        }
    }

    #[test]
    fn operation_inventory_must_match_the_plugin_descriptor() {
        let graph = graph_with_operations("feedback", &["createFeedback", "getFeedback"]);
        let mut contributions = ContributionCollection::default();
        contributions.push(Arc::new(
            HttpModule::new(PluginId::new("feedback").unwrap(), Router::new())
                .with_operations(["createFeedback"]),
        ));
        let error = validate_plugin_http_modules(&graph, &contributions.freeze()).unwrap_err();
        assert!(matches!(
            error,
            HttpCompositionError::OperationMismatch { .. }
        ));
    }

    #[test]
    fn exact_operation_inventory_is_accepted() {
        let graph = graph_with_operations("feedback", &["createFeedback", "getFeedback"]);
        let mut contributions = ContributionCollection::default();
        contributions.push(Arc::new(
            HttpModule::new(PluginId::new("feedback").unwrap(), Router::new())
                .with_operations(["createFeedback", "getFeedback"]),
        ));
        assert!(validate_plugin_http_modules(&graph, &contributions.freeze()).is_ok());
    }

    #[test]
    fn upload_capable_modules_raise_only_the_global_ceiling() {
        let mut contributions = ContributionCollection::default();
        contributions.push(Arc::new(
            HttpModule::new(PluginId::new("uploads").unwrap(), Router::new())
                .with_max_request_body_bytes(8 * 1024 * 1024),
        ));
        let frozen = contributions.freeze();
        assert_eq!(
            required_request_body_bytes(1024 * 1024, &frozen),
            8 * 1024 * 1024
        );
        assert_eq!(
            required_request_body_bytes(16 * 1024 * 1024, &frozen),
            16 * 1024 * 1024
        );
    }
}
