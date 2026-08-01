use minco_core::{
    ConfigurationValueKind, DistributionOperation, DistributionResource, Plugin, PluginDescriptor,
    PluginDistributionKind, PluginDistributionManifest, PluginManager, PluginSelection,
    ResourceIntent, WakeSource,
};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
};

pub const PLUGIN_CONFORMANCE_PROFILE: &str = "minco-plugin-v1";
pub const ADAPTER_CONFORMANCE_PROFILE: &str = "minco-adapter-v1";
pub const RUNTIME_CONFORMANCE_PROFILE: &str = "minco-runtime-v1";
const MAX_DISTRIBUTION_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConformanceStatus {
    Passed,
    Failed,
    NotAssessed,
    NotRun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConformanceAssurance {
    pub plugin_contract: ConformanceStatus,
    pub plugin_lifecycle: ConformanceStatus,
    pub application_readiness: ConformanceStatus,
    pub provider_live: ConformanceStatus,
    pub production_readiness: ConformanceStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConformanceDiagnostic {
    pub code: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginConformanceReport {
    pub profile: String,
    pub plugin_id: String,
    pub status: ConformanceStatus,
    pub assurance: ConformanceAssurance,
    pub diagnostics: Vec<ConformanceDiagnostic>,
}

impl PluginConformanceReport {
    #[must_use]
    pub const fn is_passed(&self) -> bool {
        matches!(self.status, ConformanceStatus::Passed)
    }

    pub fn assert_passed(&self) {
        assert!(
            self.is_passed(),
            "plugin conformance failed: {}",
            serde_json::to_string_pretty(self).expect("serialize conformance report")
        );
    }
}

pub struct PluginConformance {
    package_root: PathBuf,
    target_descriptor: Option<PluginDescriptor>,
    target_plugin: Option<Arc<dyn Plugin>>,
    supporting_plugins: Vec<Arc<dyn Plugin>>,
    configuration: Option<serde_json::Value>,
}

impl std::fmt::Debug for PluginConformance {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginConformance")
            .field("package_root", &self.package_root)
            .field(
                "target_plugin",
                &self
                    .target_descriptor
                    .as_ref()
                    .map(|descriptor| &descriptor.id),
            )
            .field("supporting_plugin_count", &self.supporting_plugins.len())
            .field("configuration_present", &self.configuration.is_some())
            .finish()
    }
}

impl PluginConformance {
    #[must_use]
    pub fn for_package(package_root: impl Into<PathBuf>) -> Self {
        Self {
            package_root: package_root.into(),
            target_descriptor: None,
            target_plugin: None,
            supporting_plugins: Vec::new(),
            configuration: None,
        }
    }

    #[must_use]
    pub fn with_descriptor(mut self, descriptor: PluginDescriptor) -> Self {
        self.target_descriptor = Some(descriptor);
        self
    }

    #[must_use]
    pub fn with_plugin<P>(mut self, plugin: P) -> Self
    where
        P: Plugin,
    {
        self.target_descriptor = Some(plugin.descriptor());
        self.target_plugin = Some(Arc::new(plugin));
        self
    }

    #[must_use]
    pub fn with_supporting_plugin<P>(mut self, plugin: P) -> Self
    where
        P: Plugin,
    {
        self.supporting_plugins.push(Arc::new(plugin));
        self
    }

    #[must_use]
    pub fn with_configuration(mut self, configuration: serde_json::Value) -> Self {
        self.configuration = Some(configuration);
        self
    }

    #[must_use]
    pub fn run(self) -> PluginConformanceReport {
        let mut diagnostics = Vec::new();
        let cargo_path = self.package_root.join("Cargo.toml");
        let cargo_source = match std::fs::read_to_string(&cargo_path) {
            Ok(source) => source,
            Err(error) => {
                diagnostics.push(diagnostic(
                    "package_manifest_unreadable",
                    "Cargo.toml",
                    format!("cannot read package manifest: {error}"),
                ));
                return report("unknown", "unknown", diagnostics);
            }
        };
        let cargo: toml::Value = match toml::from_str(&cargo_source) {
            Ok(cargo) => cargo,
            Err(error) => {
                diagnostics.push(diagnostic(
                    "package_manifest_invalid",
                    "Cargo.toml",
                    format!("package manifest is invalid TOML: {error}"),
                ));
                return report("unknown", "unknown", diagnostics);
            }
        };
        let package_name = cargo
            .get("package")
            .and_then(|value| value.get("name"))
            .and_then(toml::Value::as_str)
            .unwrap_or("unknown");
        let Some(distribution_file) = cargo
            .get("package")
            .and_then(|value| value.get("metadata"))
            .and_then(|value| value.get("minco"))
            .and_then(|value| value.get("plugin"))
            .and_then(toml::Value::as_str)
        else {
            diagnostics.push(diagnostic(
                "distribution_pointer_missing",
                "package.metadata.minco.plugin",
                "package metadata must name one distribution record",
            ));
            return report("unknown", package_name, diagnostics);
        };

        let distribution_path = Path::new(distribution_file);
        if distribution_path.components().count() != 1
            || distribution_path.file_name().and_then(|name| name.to_str())
                != Some(distribution_file)
            || !distribution_path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        {
            diagnostics.push(diagnostic(
                "distribution_pointer_invalid",
                "package.metadata.minco.plugin",
                "distribution pointer must be one package-root JSON filename",
            ));
            return report("unknown", package_name, diagnostics);
        }

        let included = cargo
            .get("package")
            .and_then(|value| value.get("include"))
            .and_then(toml::Value::as_array)
            .is_some_and(|entries| {
                entries.iter().any(|entry| {
                    entry
                        .as_str()
                        .is_some_and(|entry| package_include_covers(entry, distribution_file))
                })
            });
        if !included {
            diagnostics.push(diagnostic(
                "distribution_not_packaged",
                "package.include",
                format!("package include list omits {distribution_file}"),
            ));
        }

        let distribution_path = self.package_root.join(distribution_path);
        let distribution_metadata = match std::fs::symlink_metadata(&distribution_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                diagnostics.push(diagnostic(
                    "distribution_unreadable",
                    distribution_file,
                    format!("cannot inspect distribution record: {error}"),
                ));
                return report("unknown", package_name, diagnostics);
            }
        };
        if !distribution_metadata.file_type().is_file() {
            diagnostics.push(diagnostic(
                "distribution_not_regular_file",
                distribution_file,
                "distribution record must be a regular package-root file",
            ));
            return report("unknown", package_name, diagnostics);
        }
        if distribution_metadata.len() > MAX_DISTRIBUTION_BYTES {
            diagnostics.push(diagnostic(
                "distribution_too_large",
                distribution_file,
                format!("distribution record exceeds {MAX_DISTRIBUTION_BYTES} bytes"),
            ));
            return report("unknown", package_name, diagnostics);
        }
        let distribution_source = match std::fs::read_to_string(&distribution_path) {
            Ok(source) => source,
            Err(error) => {
                diagnostics.push(diagnostic(
                    "distribution_unreadable",
                    distribution_file,
                    format!("cannot read distribution record: {error}"),
                ));
                return report("unknown", package_name, diagnostics);
            }
        };
        let distribution: PluginDistributionManifest =
            match serde_json::from_str(&distribution_source) {
                Ok(distribution) => distribution,
                Err(error) => {
                    diagnostics.push(diagnostic(
                        "distribution_invalid",
                        distribution_file,
                        format!("distribution record is invalid: {error}"),
                    ));
                    return report("unknown", package_name, diagnostics);
                }
            };

        validate_distribution(&distribution, &mut diagnostics);
        validate_provider_dependencies(&cargo, distribution.kind, &mut diagnostics);
        validate_package_assets(&self.package_root, &cargo, &distribution, &mut diagnostics);
        if let Some(descriptor) = &self.target_descriptor {
            validate_linked_descriptor(&distribution, descriptor, &mut diagnostics);
        }
        let contract_status = status_for(&diagnostics);
        let lifecycle = if contract_status == ConformanceStatus::Passed {
            self.target_plugin
                .as_ref()
                .map_or(ConformanceStatus::NotAssessed, |target| {
                    validate_lifecycle(
                        Arc::clone(target),
                        &self.supporting_plugins,
                        self.configuration.as_ref(),
                        &mut diagnostics,
                    )
                })
        } else {
            ConformanceStatus::NotAssessed
        };
        report_with_assurance(
            &distribution.conformance.profile,
            distribution.id.as_str(),
            diagnostics,
            contract_status,
            lifecycle,
        )
    }
}

fn validate_provider_dependencies(
    cargo: &toml::Value,
    kind: PluginDistributionKind,
    diagnostics: &mut Vec<ConformanceDiagnostic>,
) {
    if kind != PluginDistributionKind::Plugin {
        return;
    }
    for section in ["dependencies", "build-dependencies"] {
        validate_dependency_table(cargo.get(section), section, diagnostics);
    }
    for (target, configuration) in cargo
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|targets| targets.iter())
    {
        for section in ["dependencies", "build-dependencies"] {
            validate_dependency_table(
                configuration.get(section),
                &format!("target.{target}.{section}"),
                diagnostics,
            );
        }
    }
}

fn validate_dependency_table(
    dependencies: Option<&toml::Value>,
    path: &str,
    diagnostics: &mut Vec<ConformanceDiagnostic>,
) {
    for (dependency, specification) in dependencies
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(toml::Table::iter)
    {
        let package = specification
            .as_table()
            .and_then(|specification| specification.get("package"))
            .and_then(toml::Value::as_str)
            .unwrap_or(dependency);
        if is_provider_runtime_dependency(package) {
            diagnostics.push(diagnostic(
                "provider_dependency_leakage",
                format!("{path}.{dependency}"),
                format!(
                    "provider-neutral plugins must place {package} in an explicit adapter or runtime crate"
                ),
            ));
        }
    }
}

fn validate_package_assets(
    package_root: &Path,
    cargo: &toml::Value,
    distribution: &PluginDistributionManifest,
    diagnostics: &mut Vec<ConformanceDiagnostic>,
) {
    for migration in &distribution.migrations {
        validate_package_asset(
            package_root,
            cargo,
            "migration",
            &migration.id,
            &migration.path,
            diagnostics,
        );
    }
    for seed in &distribution.seeds {
        validate_package_asset(
            package_root,
            cargo,
            "seed",
            &seed.id,
            &seed.path,
            diagnostics,
        );
    }
    let features = cargo.get("features").and_then(toml::Value::as_table);
    for resource in &distribution.resources {
        if let Some(feature) = &resource.feature
            && !features.is_some_and(|features| features.contains_key(feature))
        {
            diagnostics.push(diagnostic(
                "resource_feature_unknown",
                format!("resources.{}.feature", resource.id),
                format!("resource feature {feature} is absent from Cargo.toml"),
            ));
        }
    }
}

fn validate_package_asset(
    package_root: &Path,
    cargo: &toml::Value,
    kind: &str,
    id: &str,
    relative: &str,
    diagnostics: &mut Vec<ConformanceDiagnostic>,
) {
    let field_path = format!("{kind}s.{id}.path");
    let relative_path = Path::new(relative);
    if relative.is_empty()
        || relative_path.is_absolute()
        || !relative_path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        diagnostics.push(diagnostic(
            format!("{kind}_path_invalid"),
            field_path,
            "asset path must be normalized and package-relative",
        ));
        return;
    }
    let asset = package_root.join(relative_path);
    if !asset.exists() {
        diagnostics.push(diagnostic(
            format!("{kind}_path_missing"),
            field_path,
            format!("declared asset {relative} does not exist"),
        ));
        return;
    }
    let inside_package = std::fs::canonicalize(package_root)
        .and_then(|root| std::fs::canonicalize(&asset).map(|asset| asset.starts_with(root)))
        .unwrap_or(false);
    if !inside_package {
        diagnostics.push(diagnostic(
            format!("{kind}_path_escapes_package"),
            field_path,
            "declared asset resolves outside the package root",
        ));
        return;
    }
    let included = cargo
        .get("package")
        .and_then(|value| value.get("include"))
        .and_then(toml::Value::as_array)
        .is_some_and(|entries| {
            entries.iter().any(|entry| {
                entry
                    .as_str()
                    .is_some_and(|entry| package_include_covers(entry, relative))
            })
        });
    if !included {
        diagnostics.push(diagnostic(
            format!("{kind}_not_packaged"),
            field_path,
            format!("package include list omits {relative}"),
        ));
    }
}

fn package_include_covers(pattern: &str, relative: &str) -> bool {
    let pattern = pattern.trim_start_matches('/').trim_end_matches('/');
    let relative = relative.trim_start_matches('/').trim_end_matches('/');
    if let Some(prefix) = pattern.strip_suffix("/**") {
        relative == prefix || relative.starts_with(&format!("{prefix}/"))
    } else {
        pattern == relative
    }
}

fn is_provider_runtime_dependency(name: &str) -> bool {
    name == "aws-config"
        || name.starts_with("aws-sdk-")
        || name.starts_with("lambda-")
        || name.starts_with("lambda_")
        || name == "aws-lambda-events"
        || name == "aws_lambda_events"
}

fn validate_linked_descriptor(
    distribution: &PluginDistributionManifest,
    descriptor: &PluginDescriptor,
    diagnostics: &mut Vec<ConformanceDiagnostic>,
) {
    if distribution.id != descriptor.id {
        diagnostics.push(diagnostic(
            "descriptor_id_mismatch",
            "descriptor.id",
            format!(
                "linked descriptor ID {} does not match distribution ID {}",
                descriptor.id, distribution.id
            ),
        ));
    }
    if distribution.plugin_version != descriptor.version {
        diagnostics.push(diagnostic(
            "descriptor_version_mismatch",
            "descriptor.version",
            format!(
                "linked descriptor version {} does not match distribution version {}",
                descriptor.version, distribution.plugin_version
            ),
        ));
    }
    if distribution.core_compatibility != descriptor.core_compatibility {
        diagnostics.push(diagnostic(
            "descriptor_core_compatibility_mismatch",
            "descriptor.core_compatibility",
            format!(
                "linked descriptor core compatibility {} does not match distribution {}",
                descriptor.core_compatibility, distribution.core_compatibility
            ),
        ));
    }
    if distribution.stability != descriptor.stability {
        diagnostics.push(diagnostic(
            "descriptor_stability_mismatch",
            "descriptor.stability",
            "linked descriptor stability does not match the distribution record",
        ));
    }
    if distribution.default_enabled != descriptor.default_enabled {
        diagnostics.push(diagnostic(
            "descriptor_default_selection_mismatch",
            "descriptor.default_enabled",
            "linked descriptor default selection does not match the distribution record",
        ));
    }
    compare_descriptor_field(
        "plugin_dependencies",
        &distribution.plugin_dependencies,
        &descriptor.plugin_dependencies,
        diagnostics,
    );
    compare_descriptor_field(
        "requires",
        &distribution.requires,
        &descriptor.requires,
        diagnostics,
    );
    compare_descriptor_field(
        "provides",
        &distribution.provides,
        &descriptor.provides,
        diagnostics,
    );
    compare_descriptor_field(
        "configuration",
        &distribution.configuration,
        &descriptor.configuration,
        diagnostics,
    );
    compare_descriptor_field(
        "health_checks",
        &distribution.health_checks,
        &descriptor.health_checks,
        diagnostics,
    );
    compare_descriptor_field(
        "data_classes",
        &distribution.data_classes,
        &descriptor.data_classes,
        diagnostics,
    );
    let distribution_operations = distribution
        .operations
        .iter()
        .cloned()
        .map(|mut operation| {
            operation.headers.clear();
            operation
        })
        .collect::<Vec<_>>();
    let descriptor_operations = descriptor
        .operations
        .iter()
        .map(|operation| DistributionOperation {
            operation_id: operation.operation_id.clone(),
            method: operation.method.clone(),
            path: operation.path.clone(),
            public: operation.public,
            idempotent: operation.idempotent,
            headers: Vec::new(),
        })
        .collect::<Vec<_>>();
    compare_descriptor_field(
        "operations",
        &distribution_operations,
        &descriptor_operations,
        diagnostics,
    );
    if descriptor
        .documentation
        .as_deref()
        .is_some_and(|reference| reference != distribution.documentation.reference)
    {
        diagnostics.push(diagnostic(
            "descriptor_documentation_mismatch",
            "descriptor.documentation",
            "linked descriptor documentation does not match the distribution reference",
        ));
    }
    for migration in &descriptor.migrations {
        if !distribution.migrations.contains(migration) {
            diagnostics.push(diagnostic(
                "descriptor_migration_missing",
                format!("descriptor.migrations.{}", migration.id),
                "linked migration is absent from the distribution union",
            ));
        }
    }
    for resource in &descriptor.resources {
        if !distribution
            .resources
            .iter()
            .any(|candidate| resource_matches(candidate, resource))
        {
            diagnostics.push(diagnostic(
                "descriptor_resource_missing",
                format!("descriptor.resources.{}", resource.id),
                "linked resource is absent from the distribution union",
            ));
        }
    }
}

fn compare_descriptor_field<T: PartialEq>(
    field: &str,
    distribution: &[T],
    descriptor: &[T],
    diagnostics: &mut Vec<ConformanceDiagnostic>,
) {
    if distribution != descriptor {
        diagnostics.push(diagnostic(
            format!("descriptor_{field}_mismatch"),
            format!("descriptor.{field}"),
            format!("linked descriptor {field} do not match the distribution record"),
        ));
    }
}

fn resource_matches(distribution: &DistributionResource, runtime: &ResourceIntent) -> bool {
    distribution.id == runtime.id
        && distribution.kind == runtime.kind
        && distribution.idle_cost == runtime.idle_cost
        && distribution.wake_sources == runtime.wake_sources
        && distribution.dependencies == runtime.dependencies
}

fn validate_lifecycle(
    target: Arc<dyn Plugin>,
    supporting_plugins: &[Arc<dyn Plugin>],
    configuration: Option<&serde_json::Value>,
    diagnostics: &mut Vec<ConformanceDiagnostic>,
) -> ConformanceStatus {
    let target_descriptor = target.descriptor();
    let target_id = target_descriptor.id.clone();
    let mut manager = PluginManager::default();
    for plugin in supporting_plugins {
        if let Err(error) = manager.register_arc(Arc::clone(plugin)) {
            diagnostics.push(diagnostic(
                "supporting_plugin_registration_failed",
                "lifecycle.registration",
                format!("supporting plugin registration failed: {error}"),
            ));
            return ConformanceStatus::Failed;
        }
    }
    if let Err(error) = manager.register_arc(target) {
        diagnostics.push(diagnostic(
            "plugin_registration_failed",
            "lifecycle.registration",
            format!("plugin registration failed: {error}"),
        ));
        return ConformanceStatus::Failed;
    }
    let mut selection = PluginSelection::default();
    selection.enabled.insert(target_id.clone());
    if let Some(configuration) = configuration {
        selection
            .configuration
            .insert(target_id, configuration.clone());
    }
    match manager.compose(&selection) {
        Ok(application) => {
            let first_provenance = match serde_json::to_value(application.registration_provenance())
            {
                Ok(provenance) => provenance,
                Err(error) => {
                    diagnostics.push(diagnostic(
                        "registration_provenance_unserializable",
                        "lifecycle.provenance",
                        format!("registration provenance is not serializable: {error}"),
                    ));
                    return ConformanceStatus::Failed;
                }
            };
            let second_application = match manager.compose(&selection) {
                Ok(application) => application,
                Err(error) => {
                    diagnostics.push(diagnostic(
                        "plugin_recomposition_failed",
                        "lifecycle.composition",
                        format!("repeated plugin composition failed: {error}"),
                    ));
                    return ConformanceStatus::Failed;
                }
            };
            let second_provenance =
                match serde_json::to_value(second_application.registration_provenance()) {
                    Ok(provenance) => provenance,
                    Err(error) => {
                        diagnostics.push(diagnostic(
                            "registration_provenance_unserializable",
                            "lifecycle.provenance",
                            format!("registration provenance is not serializable: {error}"),
                        ));
                        return ConformanceStatus::Failed;
                    }
                };
            if first_provenance != second_provenance {
                diagnostics.push(diagnostic(
                    "registration_provenance_nondeterministic",
                    "lifecycle.provenance",
                    "repeated composition produced different registration provenance",
                ));
                return ConformanceStatus::Failed;
            }
            if !target_descriptor.configuration.is_empty()
                && !rejects_unknown_configuration(&manager, &selection, &target_descriptor)
            {
                diagnostics.push(diagnostic(
                    "unknown_configuration_accepted",
                    "lifecycle.configuration",
                    "plugin composition accepted a field absent from the descriptor schema",
                ));
                ConformanceStatus::Failed
            } else {
                ConformanceStatus::Passed
            }
        }
        Err(error) => {
            diagnostics.push(diagnostic(
                "plugin_composition_failed",
                "lifecycle.composition",
                format!("plugin composition failed: {error}"),
            ));
            ConformanceStatus::Failed
        }
    }
}

fn rejects_unknown_configuration(
    manager: &PluginManager,
    selection: &PluginSelection,
    descriptor: &PluginDescriptor,
) -> bool {
    let mut probe = selection.clone();
    let mut supplied = probe
        .configuration
        .get(&descriptor.id)
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut unknown = "__minco_conformance_unknown".to_owned();
    while descriptor
        .configuration
        .iter()
        .any(|field| field.key == unknown)
    {
        unknown.push('_');
    }
    supplied.insert(unknown, serde_json::Value::Bool(true));
    probe
        .configuration
        .insert(descriptor.id.clone(), serde_json::Value::Object(supplied));
    matches!(
        manager.build_graph(&probe),
        Err(minco_core::PluginError::UnknownConfigurationField { .. })
    )
}

fn validate_distribution(
    distribution: &PluginDistributionManifest,
    diagnostics: &mut Vec<ConformanceDiagnostic>,
) {
    if distribution.schema != 1 {
        diagnostics.push(diagnostic(
            "distribution_schema_unsupported",
            "schema",
            format!("unsupported distribution schema {}", distribution.schema),
        ));
    }
    match minco_core::CORE_API_VERSION.parse() {
        Ok(current) if !distribution.core_compatibility.matches(&current) => {
            diagnostics.push(diagnostic(
                "core_compatibility_excludes_current",
                "core_compatibility",
                format!(
                    "{} excludes current Minco core {}",
                    distribution.core_compatibility,
                    minco_core::CORE_API_VERSION
                ),
            ));
        }
        Ok(_) => {}
        Err(error) => diagnostics.push(diagnostic(
            "current_core_version_invalid",
            "core_compatibility",
            format!(
                "Minco core version {} is invalid: {error}",
                minco_core::CORE_API_VERSION
            ),
        )),
    }
    let expected_profile = match distribution.kind {
        PluginDistributionKind::Plugin => PLUGIN_CONFORMANCE_PROFILE,
        PluginDistributionKind::Adapter => ADAPTER_CONFORMANCE_PROFILE,
        PluginDistributionKind::Runtime => RUNTIME_CONFORMANCE_PROFILE,
    };
    if distribution.conformance.profile != expected_profile {
        diagnostics.push(diagnostic(
            "conformance_profile_mismatch",
            "conformance.profile",
            format!(
                "{} components require conformance profile {expected_profile}",
                kind_name(distribution.kind)
            ),
        ));
    }
    if distribution.runtimes.is_empty() {
        diagnostics.push(diagnostic(
            "runtime_missing",
            "runtimes",
            "at least one supported runtime is required",
        ));
    }
    push_duplicate_strings(
        "runtime_duplicate",
        "runtimes",
        &distribution.runtimes,
        diagnostics,
    );
    push_duplicate_strings(
        "database_duplicate",
        "databases",
        &distribution.databases,
        diagnostics,
    );
    if !distribution.documentation.reference.starts_with("https://") {
        diagnostics.push(diagnostic(
            "documentation_reference_insecure",
            "documentation.reference",
            "reference documentation must use HTTPS",
        ));
    }
    if distribution.failure_policy.description.trim().is_empty() {
        diagnostics.push(diagnostic(
            "failure_policy_undocumented",
            "failure_policy.description",
            "failure policy requires a non-empty description",
        ));
    }
    if distribution.conformance.evidence.is_empty()
        || distribution
            .conformance
            .evidence
            .iter()
            .any(|item| item.trim().is_empty())
    {
        diagnostics.push(diagnostic(
            "conformance_evidence_missing",
            "conformance.evidence",
            "at least one inert conformance evidence label is required",
        ));
    }
    let mut capability_provisions = BTreeSet::new();
    for provision in &distribution.provides {
        if !capability_provisions.insert(provision.name.as_str()) {
            diagnostics.push(diagnostic(
                "capability_provision_duplicate",
                format!("provides.{}", provision.name),
                "provided capability names must be unique",
            ));
        }
    }
    let mut capability_requirements = BTreeSet::new();
    for requirement in &distribution.requires {
        if !capability_requirements.insert(requirement.name.as_str()) {
            diagnostics.push(diagnostic(
                "capability_requirement_duplicate",
                format!("requires.{}", requirement.name),
                "required capability names must be unique",
            ));
        }
    }
    let mut configuration_keys = BTreeSet::new();
    for field in &distribution.configuration {
        if !configuration_keys.insert(field.key.as_str()) {
            diagnostics.push(diagnostic(
                "configuration_key_duplicate",
                format!("configuration.{}", field.key),
                "configuration field keys must be unique",
            ));
        }
        if field.secret && field.default.is_some() {
            diagnostics.push(diagnostic(
                "secret_configuration_default",
                format!("configuration.{}.default", field.key),
                "secret configuration fields must not publish default values",
            ));
        }
        if let Some(default) = &field.default
            && !configuration_value_matches(field.kind, default)
        {
            diagnostics.push(diagnostic(
                "configuration_default_type_mismatch",
                format!("configuration.{}.default", field.key),
                format!("default value must match declared type {:?}", field.kind),
            ));
        }
    }
    let mut operation_ids = BTreeSet::new();
    let mut operation_routes = BTreeSet::new();
    for operation in &distribution.operations {
        if !operation_ids.insert(operation.operation_id.as_str()) {
            diagnostics.push(diagnostic(
                "operation_id_duplicate",
                format!("operations.{}", operation.operation_id),
                "operation IDs must be unique",
            ));
        }
        let route = (
            operation.method.to_ascii_uppercase(),
            operation.path.as_str(),
        );
        if !operation_routes.insert(route) {
            diagnostics.push(diagnostic(
                "operation_route_duplicate",
                format!("operations.{}", operation.operation_id),
                format!(
                    "route {} {} is declared more than once",
                    operation.method, operation.path
                ),
            ));
        }
        for header in &operation.headers {
            if !is_http_token(header) {
                diagnostics.push(diagnostic(
                    "operation_header_invalid",
                    format!("operations.{}.headers", operation.operation_id),
                    format!("{header:?} is not a valid HTTP field name"),
                ));
            }
        }
    }
    let databases = distribution
        .databases
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut migration_ids = BTreeSet::new();
    for migration in &distribution.migrations {
        if !migration_ids.insert(migration.id.as_str()) {
            diagnostics.push(diagnostic(
                "migration_id_duplicate",
                format!("migrations.{}", migration.id),
                "migration set IDs must be unique",
            ));
        }
        if !databases.contains(migration.database.as_str()) {
            diagnostics.push(diagnostic(
                "migration_database_undeclared",
                format!("migrations.{}.database", migration.id),
                format!("database {} is not declared", migration.database),
            ));
        }
    }
    let mut seed_ids = BTreeSet::new();
    for seed in &distribution.seeds {
        if !seed_ids.insert(seed.id.as_str()) {
            diagnostics.push(diagnostic(
                "seed_id_duplicate",
                format!("seeds.{}", seed.id),
                "seed IDs must be unique",
            ));
        }
        if !databases.contains(seed.database.as_str()) {
            diagnostics.push(diagnostic(
                "seed_database_undeclared",
                format!("seeds.{}.database", seed.id),
                format!("database {} is not declared", seed.database),
            ));
        }
    }
    let resource_ids = distribution
        .resources
        .iter()
        .map(|resource| resource.id.as_str())
        .collect::<BTreeSet<_>>();
    if resource_ids.len() != distribution.resources.len() {
        diagnostics.push(diagnostic(
            "resource_id_duplicate",
            "resources",
            "resource IDs must be unique",
        ));
    }
    for resource in &distribution.resources {
        for dependency in &resource.dependencies {
            if !resource_ids.contains(dependency.as_str()) {
                diagnostics.push(diagnostic(
                    "resource_dependency_unknown",
                    format!("resources.{}.dependencies", resource.id),
                    format!("resource dependency {dependency} is not declared"),
                ));
            }
        }
        for action in &resource.iam_actions {
            if !is_iam_action(action) {
                diagnostics.push(diagnostic(
                    "resource_iam_action_invalid",
                    format!("resources.{}.iam_actions", resource.id),
                    format!("{action:?} is not a valid IAM action"),
                ));
            }
        }
        for wake_source in &resource.wake_sources {
            if matches!(wake_source, WakeSource::Schedule { expression } if expression.trim().is_empty())
            {
                diagnostics.push(diagnostic(
                    "schedule_expression_empty",
                    format!("resources.{}.wake_sources", resource.id),
                    "scheduled wake sources require a non-empty expression",
                ));
            }
        }
    }
    let mut health_check_ids = BTreeSet::new();
    for check in &distribution.health_checks {
        if !health_check_ids.insert(check.id.as_str()) {
            diagnostics.push(diagnostic(
                "health_check_id_duplicate",
                format!("health_checks.{}", check.id),
                "health check IDs must be unique",
            ));
        }
    }
    let mut data_classes = BTreeSet::new();
    for class in &distribution.data_classes {
        if !data_classes.insert(*class) {
            diagnostics.push(diagnostic(
                "data_class_duplicate",
                "data_classes",
                "data classes must be unique",
            ));
        }
    }
}

fn push_duplicate_strings(
    code: &str,
    path: &str,
    values: &[String],
    diagnostics: &mut Vec<ConformanceDiagnostic>,
) {
    let mut unique = BTreeSet::new();
    for value in values {
        if !unique.insert(value.as_str()) {
            diagnostics.push(diagnostic(
                code,
                path,
                format!("{value:?} is declared more than once"),
            ));
        }
    }
}

fn configuration_value_matches(kind: ConfigurationValueKind, value: &serde_json::Value) -> bool {
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

fn is_http_token(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

fn is_iam_action(value: &str) -> bool {
    let Some((service, action)) = value.split_once(':') else {
        return false;
    };
    !service.is_empty()
        && !action.is_empty()
        && !action.contains(':')
        && service
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && action
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'*' | b'-' | b'_'))
}

const fn kind_name(kind: PluginDistributionKind) -> &'static str {
    match kind {
        PluginDistributionKind::Plugin => "plugin",
        PluginDistributionKind::Adapter => "adapter",
        PluginDistributionKind::Runtime => "runtime",
    }
}

fn diagnostic(
    code: impl Into<String>,
    path: impl Into<String>,
    message: impl Into<String>,
) -> ConformanceDiagnostic {
    ConformanceDiagnostic {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

fn report(
    profile: &str,
    plugin_id: &str,
    diagnostics: Vec<ConformanceDiagnostic>,
) -> PluginConformanceReport {
    let plugin_contract = status_for(&diagnostics);
    report_with_assurance(
        profile,
        plugin_id,
        diagnostics,
        plugin_contract,
        ConformanceStatus::NotAssessed,
    )
}

fn report_with_assurance(
    profile: &str,
    plugin_id: &str,
    mut diagnostics: Vec<ConformanceDiagnostic>,
    plugin_contract: ConformanceStatus,
    plugin_lifecycle: ConformanceStatus,
) -> PluginConformanceReport {
    diagnostics.sort_by(|left, right| {
        (&left.code, &left.path, &left.message).cmp(&(&right.code, &right.path, &right.message))
    });
    let status = if plugin_contract == ConformanceStatus::Failed
        || plugin_lifecycle == ConformanceStatus::Failed
    {
        ConformanceStatus::Failed
    } else {
        ConformanceStatus::Passed
    };
    PluginConformanceReport {
        profile: profile.to_owned(),
        plugin_id: plugin_id.to_owned(),
        status,
        assurance: ConformanceAssurance {
            plugin_contract,
            plugin_lifecycle,
            application_readiness: ConformanceStatus::NotAssessed,
            provider_live: ConformanceStatus::NotRun,
            production_readiness: ConformanceStatus::NotAssessed,
        },
        diagnostics,
    }
}

const fn status_for(diagnostics: &[ConformanceDiagnostic]) -> ConformanceStatus {
    if diagnostics.is_empty() {
        ConformanceStatus::Passed
    } else {
        ConformanceStatus::Failed
    }
}
