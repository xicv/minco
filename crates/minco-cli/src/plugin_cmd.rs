use anyhow::{Context, Result, bail};
use minco_core::{
    DistributionOperation, PluginDescriptor, PluginDistributionKind, PluginDistributionManifest,
    PluginStability, ResourceIntent,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

const MAX_DISTRIBUTION_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginCatalog {
    pub schema: u32,
    #[serde(default)]
    pub plugin: Vec<PluginCatalogEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginCatalogEntry {
    pub id: String,
    #[serde(rename = "crate")]
    pub crate_name: String,
    /// Repository-relative path for a workspace plugin. Omit for a registry dependency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub kind: PluginKind,
    pub feature: String,
    pub default_enabled: bool,
    pub stability: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distribution: Option<PluginDistributionManifest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    Plugin,
    Adapter,
    Runtime,
}

pub fn load_catalog(root: &Path, relative: &Path) -> Result<PluginCatalog> {
    let path = root.join(relative);
    let mut catalog: PluginCatalog = toml::from_str(
        &std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?,
    )?;
    if catalog.schema != 1 {
        bail!("unsupported plugin catalog schema {}", catalog.schema);
    }
    for plugin in &mut catalog.plugin {
        plugin.distribution = load_distribution(root, plugin)?;
    }
    Ok(catalog)
}

fn load_distribution(
    root: &Path,
    plugin: &PluginCatalogEntry,
) -> Result<Option<PluginDistributionManifest>> {
    let Some(relative) = &plugin.path else {
        return Ok(None);
    };
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || !relative
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        bail!(
            "plugin {} local package path must be project-relative and normalized",
            plugin.id
        );
    }
    let package = root.join(relative);
    if !package.exists() {
        return Ok(None);
    }
    let canonical_root = std::fs::canonicalize(root)
        .with_context(|| format!("resolve project root {}", root.display()))?;
    let canonical_package = std::fs::canonicalize(&package)
        .with_context(|| format!("resolve plugin package {}", package.display()))?;
    if !canonical_package.starts_with(&canonical_root) {
        bail!(
            "plugin {} local package path resolves outside the project",
            plugin.id
        );
    }
    let manifest = canonical_package.join("Cargo.toml");
    if !manifest.is_file() {
        return Ok(None);
    }
    let source = std::fs::read_to_string(&manifest)
        .with_context(|| format!("read {}", manifest.display()))?;
    let document: toml::Value =
        toml::from_str(&source).with_context(|| format!("parse {}", manifest.display()))?;
    let Some(metadata) = document
        .get("package")
        .and_then(|value| value.get("metadata"))
        .and_then(|value| value.get("minco"))
        .and_then(|value| value.get("plugin"))
    else {
        return Ok(None);
    };
    let Some(distribution_file) = metadata.as_str() else {
        bail!(
            "package.metadata.minco.plugin in {} must name one package-root JSON file",
            manifest.display()
        );
    };
    let distribution_path = Path::new(distribution_file);
    if distribution_path.components().count() != 1
        || distribution_path.file_name().and_then(|name| name.to_str()) != Some(distribution_file)
        || !distribution_path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        bail!(
            "package.metadata.minco.plugin in {} must be one package-root JSON filename",
            manifest.display()
        );
    }
    let distribution_path = canonical_package.join(distribution_path);
    let distribution_metadata = std::fs::symlink_metadata(&distribution_path)
        .with_context(|| format!("read metadata for {}", distribution_path.display()))?;
    if !distribution_metadata.file_type().is_file() {
        bail!(
            "plugin distribution record {} must be a regular file",
            distribution_path.display()
        );
    }
    if distribution_metadata.len() > MAX_DISTRIBUTION_BYTES {
        bail!(
            "plugin distribution record {} exceeds {} bytes",
            distribution_path.display(),
            MAX_DISTRIBUTION_BYTES
        );
    }
    serde_json::from_str(
        &std::fs::read_to_string(&distribution_path)
            .with_context(|| format!("read {}", distribution_path.display()))?,
    )
    .map(Some)
    .with_context(|| format!("parse {}", distribution_path.display()))
}

pub fn set_plugin_state(root: &Path, plugin_id: &str, enabled: bool) -> Result<()> {
    validate_plugin_id(plugin_id)?;
    let path = root.join("minco.toml");
    let mut document: toml::Value = toml::from_str(&std::fs::read_to_string(&path)?)?;
    let plugins = document
        .as_table_mut()
        .context("minco.toml root must be a table")?
        .entry("plugins")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .context("plugins must be a table")?;
    let mut enabled_set = read_string_set(plugins.get("enabled"))?;
    let mut disabled_set = read_string_set(plugins.get("disabled"))?;
    if enabled {
        enabled_set.insert(plugin_id.into());
        disabled_set.remove(plugin_id);
    } else {
        disabled_set.insert(plugin_id.into());
        enabled_set.remove(plugin_id);
    }
    plugins.insert("enabled".into(), string_array(enabled_set));
    plugins.insert("disabled".into(), string_array(disabled_set));
    std::fs::write(path, toml::to_string_pretty(&document)?)?;
    Ok(())
}

pub fn scaffold_plugin(root: &Path, plugin_id: &str) -> Result<()> {
    validate_plugin_id(plugin_id)?;
    let crate_name = format!("minco-plugin-{plugin_id}");
    let directory = root.join("plugins").join(&crate_name);
    if directory.exists() {
        bail!("plugin directory {} already exists", directory.display());
    }
    std::fs::create_dir_all(directory.join("src"))?;
    std::fs::write(
        directory.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{crate_name}\"\nversion.workspace = true\nedition.workspace = true\nrust-version.workspace = true\nlicense.workspace = true\nrepository.workspace = true\npublish = false\ninclude = [\"src/**\", \"Cargo.toml\", \"minco-plugin.json\"]\n\n[package.metadata.minco]\nplugin = \"minco-plugin.json\"\n\n[dependencies]\nminco-core.workspace = true\nsemver.workspace = true\n\n[dev-dependencies]\nminco-test.workspace = true\n\n[lints]\nworkspace = true\n"
        ),
    )?;
    std::fs::write(
        directory.join("minco-plugin.json"),
        default_distribution_record(plugin_id, &crate_name)?,
    )?;
    let type_name = plugin_id
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect::<String>();
    std::fs::write(
        directory.join("src/lib.rs"),
        format!(
            "//! Minco plugin `{plugin_id}`.\n#![forbid(unsafe_code)]\n\nuse minco_core::{{Plugin, PluginContext, PluginDescriptor, PluginError, PluginId}};\nuse semver::Version;\n\n#[derive(Debug, Clone, Default)]\npub struct {type_name}Plugin;\n\nimpl Plugin for {type_name}Plugin {{\n    fn descriptor(&self) -> PluginDescriptor {{\n        PluginDescriptor::new(\n            PluginId::new(\"{plugin_id}\").expect(\"static plugin ID\"),\n            Version::new(0, 1, 0),\n            \"Describe the plugin capability\",\n        )\n    }}\n\n    fn install(&self, _context: &mut PluginContext<'_>) -> Result<(), PluginError> {{\n        Ok(())\n    }}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    use minco_test::PluginConformance;\n\n    #[test]\n    fn passes_the_public_plugin_conformance_kit() {{\n        PluginConformance::for_package(env!(\"CARGO_MANIFEST_DIR\"))\n            .with_plugin({type_name}Plugin)\n            .run()\n            .assert_passed();\n    }}\n}}\n"
        ),
    )?;
    add_workspace_member(root, &format!("plugins/{crate_name}"), &crate_name)?;
    add_catalog_entry(root, plugin_id, &crate_name)?;
    Ok(())
}

pub fn default_distribution_record(plugin_id: &str, crate_name: &str) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec_pretty(&serde_json::json!({
        "schema": 1,
        "id": plugin_id,
        "kind": "plugin",
        "plugin_version": "0.1.0",
        "core_compatibility": "*",
        "stability": "experimental",
        "default_enabled": false,
        "feature": format!("plugin-{plugin_id}"),
        "runtimes": ["native"],
        "retention": "none",
        "failure_policy": {
            "mode": "fail_closed",
            "description": "Plugin operations return explicit errors until an application defines a narrower policy."
        },
        "documentation": {
            "reference": format!("https://docs.rs/{crate_name}")
        },
        "conformance": {
            "profile": "minco-plugin-v1",
            "evidence": [format!("cargo test -p {crate_name} --all-features --locked")]
        }
    }))?)
}

pub fn validate_catalog(root: &Path, catalog: &PluginCatalog) -> Result<Vec<String>> {
    let mut findings = Vec::new();
    let mut ids = BTreeSet::new();
    for plugin in &catalog.plugin {
        validate_plugin_id(&plugin.id)?;
        if !ids.insert(&plugin.id) {
            findings.push(format!("duplicate plugin ID {}", plugin.id));
        }
        // Registry-backed packages are not present beneath the application root. Their
        // distribution record remains inspectable in the downloaded crate archive, while
        // this local validation pass can only prove records for path dependencies.
        if plugin.path.is_some() && plugin.distribution.is_none() {
            findings.push(format!(
                "plugin {} has no [package.metadata.minco.plugin] distribution record",
                plugin.id
            ));
        }
        if let Some(relative) = &plugin.path {
            let manifest = root.join(relative).join("Cargo.toml");
            if !manifest.is_file() {
                findings.push(format!(
                    "plugin {} references missing local manifest {}",
                    plugin.id,
                    manifest.display()
                ));
            } else if let Ok(source) = std::fs::read_to_string(&manifest) {
                match toml::from_str::<toml::Value>(&source) {
                    Ok(document) => {
                        let package_name = document
                            .get("package")
                            .and_then(|value| value.get("name"))
                            .and_then(toml::Value::as_str);
                        if package_name != Some(plugin.crate_name.as_str()) {
                            findings.push(format!(
                                "plugin {} path {} declares package {:?}, expected {}",
                                plugin.id,
                                relative.display(),
                                package_name,
                                plugin.crate_name
                            ));
                        }
                        if plugin.distribution.is_some() {
                            let distribution_file = document
                                .get("package")
                                .and_then(|value| value.get("metadata"))
                                .and_then(|value| value.get("minco"))
                                .and_then(|value| value.get("plugin"))
                                .and_then(toml::Value::as_str);
                            let included = document
                                .get("package")
                                .and_then(|value| value.get("include"))
                                .and_then(toml::Value::as_array)
                                .is_some_and(|entries| {
                                    entries.iter().any(|entry| {
                                        entry.as_str().is_some_and(|entry| {
                                            Some(entry.trim_start_matches('/')) == distribution_file
                                        })
                                    })
                                });
                            if !included {
                                findings.push(format!(
                                    "plugin {} package include list omits {}",
                                    plugin.id,
                                    distribution_file.unwrap_or("distribution record")
                                ));
                            }
                        }
                    }
                    Err(error) => findings.push(format!(
                        "plugin {} local manifest {} is invalid TOML: {error}",
                        plugin.id,
                        manifest.display()
                    )),
                }
            }
        }
    }
    Ok(findings)
}

/// Validates archive metadata against the catalog and any explicitly linked descriptors.
///
/// Evidence commands are treated as inert display strings. Validation never invokes them
/// or loads code named by an archive-visible distribution record.
pub fn validate_distribution_contracts(
    catalog: &PluginCatalog,
    linked_descriptors: &[PluginDescriptor],
) -> Vec<String> {
    let mut findings = Vec::new();
    let linked = linked_descriptors
        .iter()
        .map(|descriptor| (descriptor.id.as_str(), descriptor))
        .collect::<std::collections::BTreeMap<_, _>>();

    for plugin in &catalog.plugin {
        let Some(distribution) = &plugin.distribution else {
            continue;
        };
        validate_static_distribution(plugin, distribution, &mut findings);

        if plugin.kind == PluginKind::Plugin
            && let Some(descriptor) = linked.get(plugin.id.as_str())
        {
            validate_linked_descriptor(plugin, distribution, descriptor, &mut findings);
        }
    }
    findings
}

fn validate_static_distribution(
    plugin: &PluginCatalogEntry,
    distribution: &PluginDistributionManifest,
    findings: &mut Vec<String>,
) {
    if distribution.schema != 1 {
        findings.push(format!(
            "plugin {} has unsupported distribution schema {}",
            plugin.id, distribution.schema
        ));
    }
    if distribution.id.as_str() != plugin.id {
        findings.push(format!(
            "plugin {} distribution ID {} does not match the catalog",
            plugin.id, distribution.id
        ));
    }
    if distribution.kind != distribution_kind(plugin.kind) {
        findings.push(format!(
            "plugin {} distribution kind {} does not match catalog kind {}",
            plugin.id,
            distribution_kind_name(distribution.kind),
            plugin_kind_name(plugin.kind)
        ));
    }
    if distribution.feature != plugin.feature {
        findings.push(format!(
            "plugin {} distribution feature {} does not match catalog feature {}",
            plugin.id, distribution.feature, plugin.feature
        ));
    }
    if distribution.default_enabled != plugin.default_enabled {
        findings.push(format!(
            "plugin {} distribution default_enabled {} does not match catalog value {}",
            plugin.id, distribution.default_enabled, plugin.default_enabled
        ));
    }
    if stability_name(distribution.stability) != plugin.stability {
        findings.push(format!(
            "plugin {} distribution stability {} does not match catalog stability {}",
            plugin.id,
            stability_name(distribution.stability),
            plugin.stability
        ));
    }
    match minco_core::CORE_API_VERSION.parse() {
        Ok(core_version) if !distribution.core_compatibility.matches(&core_version) => {
            findings.push(format!(
                "plugin {} distribution core compatibility {} excludes Minco core {}",
                plugin.id, distribution.core_compatibility, core_version
            ));
        }
        Ok(_) => {}
        Err(error) => findings.push(format!(
            "Minco core version {} is invalid: {error}",
            minco_core::CORE_API_VERSION
        )),
    }
    if distribution.runtimes.is_empty() {
        findings.push(format!(
            "plugin {} distribution must declare at least one runtime",
            plugin.id
        ));
    }
    if !distribution.documentation.reference.starts_with("https://") {
        findings.push(format!(
            "plugin {} distribution reference documentation must use HTTPS",
            plugin.id
        ));
    }
    if distribution.failure_policy.description.trim().is_empty() {
        findings.push(format!(
            "plugin {} distribution failure policy needs a description",
            plugin.id
        ));
    }
    if distribution.conformance.profile.trim().is_empty()
        || distribution.conformance.evidence.is_empty()
        || distribution
            .conformance
            .evidence
            .iter()
            .any(|item| item.trim().is_empty())
    {
        findings.push(format!(
            "plugin {} distribution needs a conformance profile and evidence",
            plugin.id
        ));
    }

    let mut configuration_keys = BTreeSet::new();
    for field in &distribution.configuration {
        if !configuration_keys.insert(field.key.as_str()) {
            findings.push(format!(
                "plugin {} has duplicate configuration field {}",
                plugin.id, field.key
            ));
        }
        if field.secret && field.default.is_some() {
            findings.push(format!(
                "plugin {} secret configuration {} must not declare a default",
                plugin.id, field.key
            ));
        }
    }

    let mut operation_ids = BTreeSet::new();
    for operation in &distribution.operations {
        if !operation_ids.insert(operation.operation_id.as_str()) {
            findings.push(format!(
                "plugin {} has duplicate operation {}",
                plugin.id, operation.operation_id
            ));
        }
        for header in &operation.headers {
            if !is_http_token(header) {
                findings.push(format!(
                    "plugin {} operation {} has invalid header name {}",
                    plugin.id, operation.operation_id, header
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
            findings.push(format!(
                "plugin {} has duplicate migration set {}",
                plugin.id, migration.id
            ));
        }
        if !databases.contains(migration.database.as_str()) {
            findings.push(format!(
                "plugin {} migration {} references undeclared database {}",
                plugin.id, migration.id, migration.database
            ));
        }
    }
    for seed in &distribution.seeds {
        if !databases.contains(seed.database.as_str()) {
            findings.push(format!(
                "plugin {} seed {} references undeclared database {}",
                plugin.id, seed.id, seed.database
            ));
        }
    }

    let resource_ids = distribution
        .resources
        .iter()
        .map(|resource| resource.id.as_str())
        .collect::<BTreeSet<_>>();
    if resource_ids.len() != distribution.resources.len() {
        findings.push(format!("plugin {} has duplicate resource IDs", plugin.id));
    }
    for resource in &distribution.resources {
        for dependency in &resource.dependencies {
            if !resource_ids.contains(dependency.as_str()) {
                findings.push(format!(
                    "plugin {} resource {} references unknown resource {}",
                    plugin.id, resource.id, dependency
                ));
            }
        }
        for action in &resource.iam_actions {
            if !is_iam_action(action) {
                findings.push(format!(
                    "plugin {} resource {} has invalid IAM action {}",
                    plugin.id, resource.id, action
                ));
            }
        }
    }
}

fn validate_linked_descriptor(
    plugin: &PluginCatalogEntry,
    distribution: &PluginDistributionManifest,
    descriptor: &PluginDescriptor,
    findings: &mut Vec<String>,
) {
    if distribution.plugin_version != descriptor.version {
        findings.push(format!(
            "plugin {} distribution version {} does not match linked descriptor version {}",
            plugin.id, distribution.plugin_version, descriptor.version
        ));
    }
    if distribution.core_compatibility != descriptor.core_compatibility {
        findings.push(format!(
            "plugin {} distribution core compatibility {} does not match linked descriptor {}",
            plugin.id, distribution.core_compatibility, descriptor.core_compatibility
        ));
    }
    if distribution.stability != descriptor.stability {
        findings.push(format!(
            "plugin {} distribution stability {} does not match linked descriptor {}",
            plugin.id,
            stability_name(distribution.stability),
            stability_name(descriptor.stability)
        ));
    }
    if distribution.default_enabled != descriptor.default_enabled {
        findings.push(format!(
            "plugin {} distribution default_enabled does not match the linked descriptor",
            plugin.id
        ));
    }
    compare_field(
        plugin,
        "plugin dependencies",
        &distribution.plugin_dependencies,
        &descriptor.plugin_dependencies,
        findings,
    );
    compare_field(
        plugin,
        "required capabilities",
        &distribution.requires,
        &descriptor.requires,
        findings,
    );
    compare_field(
        plugin,
        "provided capabilities",
        &distribution.provides,
        &descriptor.provides,
        findings,
    );
    compare_field(
        plugin,
        "configuration",
        &distribution.configuration,
        &descriptor.configuration,
        findings,
    );
    compare_field(
        plugin,
        "health checks",
        &distribution.health_checks,
        &descriptor.health_checks,
        findings,
    );
    compare_field(
        plugin,
        "data classes",
        &distribution.data_classes,
        &descriptor.data_classes,
        findings,
    );
    let operations = descriptor
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
    let distribution_operations = distribution
        .operations
        .iter()
        .cloned()
        .map(|mut operation| {
            operation.headers.clear();
            operation
        })
        .collect::<Vec<_>>();
    compare_field(
        plugin,
        "operations",
        &distribution_operations,
        &operations,
        findings,
    );
    if descriptor
        .documentation
        .as_deref()
        .is_some_and(|documentation| documentation != distribution.documentation.reference)
    {
        findings.push(format!(
            "plugin {} distribution reference documentation does not match the linked descriptor",
            plugin.id
        ));
    }
    for migration in &descriptor.migrations {
        if !distribution.migrations.contains(migration) {
            findings.push(format!(
                "plugin {} linked migration {} is absent from the distribution record",
                plugin.id, migration.id
            ));
        }
    }
    for resource in &descriptor.resources {
        if !distribution
            .resources
            .iter()
            .any(|candidate| resource_matches(candidate, resource))
        {
            findings.push(format!(
                "plugin {} linked resource {} is absent from the distribution record",
                plugin.id, resource.id
            ));
        }
    }
}

fn compare_field<T: PartialEq>(
    plugin: &PluginCatalogEntry,
    label: &str,
    distribution: &[T],
    descriptor: &[T],
    findings: &mut Vec<String>,
) {
    if distribution != descriptor {
        findings.push(format!(
            "plugin {} distribution {label} do not match the linked descriptor",
            plugin.id
        ));
    }
}

fn resource_matches(
    distribution: &minco_core::DistributionResource,
    runtime: &ResourceIntent,
) -> bool {
    distribution.id == runtime.id
        && distribution.kind == runtime.kind
        && distribution.idle_cost == runtime.idle_cost
        && distribution.wake_sources == runtime.wake_sources
        && distribution.dependencies == runtime.dependencies
}

const fn distribution_kind(kind: PluginKind) -> PluginDistributionKind {
    match kind {
        PluginKind::Plugin => PluginDistributionKind::Plugin,
        PluginKind::Adapter => PluginDistributionKind::Adapter,
        PluginKind::Runtime => PluginDistributionKind::Runtime,
    }
}

const fn plugin_kind_name(kind: PluginKind) -> &'static str {
    match kind {
        PluginKind::Plugin => "plugin",
        PluginKind::Adapter => "adapter",
        PluginKind::Runtime => "runtime",
    }
}

const fn distribution_kind_name(kind: PluginDistributionKind) -> &'static str {
    match kind {
        PluginDistributionKind::Plugin => "plugin",
        PluginDistributionKind::Adapter => "adapter",
        PluginDistributionKind::Runtime => "runtime",
    }
}

const fn stability_name(stability: PluginStability) -> &'static str {
    match stability {
        PluginStability::Experimental => "experimental",
        PluginStability::Beta => "beta",
        PluginStability::Stable => "stable",
        PluginStability::Deprecated => "deprecated",
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

fn add_workspace_member(root: &Path, member: &str, crate_name: &str) -> Result<()> {
    let path = root.join("Cargo.toml");
    let mut document: toml::Value = toml::from_str(&std::fs::read_to_string(&path)?)?;
    let root_table = document.as_table_mut().context("Cargo root")?;
    let workspace = root_table
        .get_mut("workspace")
        .and_then(toml::Value::as_table_mut)
        .context("workspace table")?;
    for key in ["members", "default-members"] {
        let values = workspace
            .get_mut(key)
            .and_then(toml::Value::as_array_mut)
            .context("workspace member list")?;
        if !values.iter().any(|value| value.as_str() == Some(member)) {
            values.push(toml::Value::String(member.into()));
        }
    }
    let dependencies = workspace
        .entry("dependencies")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .context("workspace.dependencies table")?;
    dependencies.insert(
        crate_name.into(),
        toml::Value::Table(toml::map::Map::from_iter([(
            String::from("path"),
            toml::Value::String(member.into()),
        )])),
    );
    std::fs::write(path, toml::to_string_pretty(&document)?)?;
    Ok(())
}

fn add_catalog_entry(root: &Path, id: &str, crate_name: &str) -> Result<()> {
    let path = root.join("plugins/catalog.toml");
    let mut catalog: PluginCatalog = toml::from_str(&std::fs::read_to_string(&path)?)?;
    catalog.plugin.push(PluginCatalogEntry {
        id: id.into(),
        crate_name: crate_name.into(),
        path: Some(std::path::PathBuf::from(format!("plugins/{crate_name}"))),
        kind: PluginKind::Plugin,
        feature: format!("plugin-{id}"),
        default_enabled: false,
        stability: "experimental".into(),
        description: "User-defined Minco plugin.".into(),
        distribution: None,
    });
    catalog.plugin.sort_by(|left, right| left.id.cmp(&right.id));
    std::fs::write(path, toml::to_string_pretty(&catalog)?)?;
    Ok(())
}

fn read_string_set(value: Option<&toml::Value>) -> Result<BTreeSet<String>> {
    value
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .context("plugin selection values must be strings")
        })
        .collect()
}

fn string_array(values: BTreeSet<String>) -> toml::Value {
    toml::Value::Array(values.into_iter().map(toml::Value::String).collect())
}

fn validate_plugin_id(value: &str) -> Result<()> {
    if value.is_empty()
        || !value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        || value.starts_with('-')
        || value.ends_with('-')
        || value.contains("--")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        bail!("plugin ID must be lower-kebab-case");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_core::{PluginDescriptor, PluginId};
    use std::fs;

    fn distribution(value: serde_json::Value) -> PluginDistributionManifest {
        serde_json::from_value(value).expect("valid distribution fixture")
    }

    fn catalog_entry(distribution: PluginDistributionManifest) -> PluginCatalogEntry {
        PluginCatalogEntry {
            id: "example".into(),
            crate_name: "minco-plugin-example".into(),
            path: None,
            kind: PluginKind::Plugin,
            feature: "plugin-example".into(),
            default_enabled: false,
            stability: "experimental".into(),
            description: "Example plugin".into(),
            distribution: Some(distribution),
        }
    }

    fn valid_distribution() -> PluginDistributionManifest {
        distribution(serde_json::json!({
            "schema": 1,
            "id": "example",
            "kind": "plugin",
            "plugin_version": "0.1.0",
            "core_compatibility": "*",
            "stability": "experimental",
            "default_enabled": false,
            "feature": "plugin-example",
            "runtimes": ["native"],
            "retention": "none",
            "failure_policy": {
                "mode": "fail_closed",
                "description": "The example operation reports failure."
            },
            "documentation": {"reference": "https://docs.rs/minco-plugin-example"},
            "conformance": {"profile": "minco-plugin-v1", "evidence": ["cargo test -p minco-plugin-example --locked"]}
        }))
    }

    #[test]
    fn distribution_validation_reports_catalog_drift_and_secret_defaults_deterministically() {
        let mut manifest = valid_distribution();
        manifest.id = PluginId::new("different").expect("fixture ID");
        manifest.feature = "other-feature".into();
        manifest.configuration.push(minco_core::ConfigurationField {
            key: "token".into(),
            kind: minco_core::ConfigurationValueKind::String,
            required: true,
            secret: true,
            description: "Secret token".into(),
            default: Some(serde_json::json!("must-not-ship")),
        });
        let catalog = PluginCatalog {
            schema: 1,
            plugin: vec![catalog_entry(manifest)],
        };

        let findings = validate_distribution_contracts(&catalog, &[]);

        assert_eq!(
            findings,
            [
                "plugin example distribution ID different does not match the catalog",
                "plugin example distribution feature other-feature does not match catalog feature plugin-example",
                "plugin example secret configuration token must not declare a default",
            ]
        );
    }

    #[test]
    fn distribution_validation_reports_runtime_descriptor_drift() {
        let catalog = PluginCatalog {
            schema: 1,
            plugin: vec![catalog_entry(valid_distribution())],
        };
        let descriptor = PluginDescriptor::new(
            PluginId::new("example").expect("fixture ID"),
            "0.2.0".parse().expect("fixture version"),
            "Example plugin",
        );

        let findings = validate_distribution_contracts(&catalog, &[descriptor]);

        assert_eq!(
            findings,
            [
                "plugin example distribution version 0.1.0 does not match linked descriptor version 0.2.0"
            ]
        );
    }

    #[test]
    fn distribution_validation_rejects_an_incompatible_current_core() {
        let mut manifest = valid_distribution();
        manifest.core_compatibility = ">=99.0.0".parse().expect("fixture requirement");
        let catalog = PluginCatalog {
            schema: 1,
            plugin: vec![catalog_entry(manifest)],
        };

        let findings = validate_distribution_contracts(&catalog, &[]);

        assert_eq!(
            findings,
            [format!(
                "plugin example distribution core compatibility >=99.0.0 excludes Minco core {}",
                minco_core::CORE_API_VERSION
            )]
        );
    }

    #[test]
    fn scaffold_creates_an_archive_visible_distribution_record() {
        let root = tempfile::tempdir().expect("temporary plugin repository");
        fs::create_dir_all(root.path().join("plugins")).expect("plugins directory");
        fs::write(
            root.path().join("Cargo.toml"),
            r"[workspace]
members = []
default-members = []

[workspace.dependencies]
",
        )
        .expect("workspace manifest");
        fs::write(root.path().join("plugins/catalog.toml"), "schema = 1\n")
            .expect("plugin catalog");

        scaffold_plugin(root.path(), "example").expect("scaffold plugin");

        let manifest =
            fs::read_to_string(root.path().join("plugins/minco-plugin-example/Cargo.toml"))
                .expect("generated Cargo manifest");
        assert!(manifest.contains("[package.metadata.minco]"));
        assert!(manifest.contains("plugin = \"minco-plugin.json\""));
        assert!(manifest.contains("include = [\"src/**\", \"Cargo.toml\", \"minco-plugin.json\"]"));
        assert!(manifest.contains("[dev-dependencies]"));
        assert!(manifest.contains("minco-test.workspace = true"));
        let source =
            fs::read_to_string(root.path().join("plugins/minco-plugin-example/src/lib.rs"))
                .expect("generated plugin source");
        assert!(source.contains("PluginConformance::for_package"));
        assert!(source.contains(".with_plugin(ExamplePlugin)"));
        let distribution: PluginDistributionManifest = serde_json::from_str(
            &fs::read_to_string(
                root.path()
                    .join("plugins/minco-plugin-example/minco-plugin.json"),
            )
            .expect("generated distribution record"),
        )
        .expect("valid generated distribution record");
        assert_eq!(distribution.id.as_str(), "example");
        assert_eq!(distribution.plugin_version.to_string(), "0.1.0");
    }

    #[test]
    fn validation_reports_missing_archive_distribution_metadata() {
        let root = tempfile::tempdir().expect("temporary plugin repository");
        fs::create_dir_all(root.path().join("plugins/minco-plugin-example"))
            .expect("plugin directory");
        fs::write(
            root.path().join("plugins/catalog.toml"),
            r#"schema = 1

[[plugin]]
id = "example"
crate = "minco-plugin-example"
path = "plugins/minco-plugin-example"
kind = "plugin"
feature = "plugin-example"
default_enabled = false
stability = "experimental"
description = "Example plugin."
"#,
        )
        .expect("plugin catalog");
        fs::write(
            root.path().join("plugins/minco-plugin-example/Cargo.toml"),
            r#"[package]
name = "minco-plugin-example"
version = "0.1.0"
"#,
        )
        .expect("plugin manifest");

        let catalog =
            load_catalog(root.path(), Path::new("plugins/catalog.toml")).expect("load catalog");
        let findings = validate_catalog(root.path(), &catalog).expect("validate catalog");

        assert_eq!(
            findings,
            ["plugin example has no [package.metadata.minco.plugin] distribution record"]
        );
    }

    #[test]
    fn validation_reports_distribution_omitted_from_package_include() {
        let root = tempfile::tempdir().expect("temporary plugin repository");
        let package = root.path().join("plugins/minco-plugin-example");
        fs::create_dir_all(&package).expect("plugin directory");
        fs::write(
            root.path().join("plugins/catalog.toml"),
            r#"schema = 1

[[plugin]]
id = "example"
crate = "minco-plugin-example"
path = "plugins/minco-plugin-example"
kind = "plugin"
feature = "plugin-example"
default_enabled = false
stability = "experimental"
description = "Example plugin."
"#,
        )
        .expect("plugin catalog");
        fs::write(
            package.join("Cargo.toml"),
            r#"[package]
name = "minco-plugin-example"
version = "0.1.0"
include = ["src/**", "Cargo.toml"]

[package.metadata.minco]
plugin = "minco-plugin.json"
"#,
        )
        .expect("plugin manifest");
        fs::write(
            package.join("minco-plugin.json"),
            serde_json::to_string(&valid_distribution()).expect("distribution JSON"),
        )
        .expect("distribution record");

        let catalog =
            load_catalog(root.path(), Path::new("plugins/catalog.toml")).expect("load catalog");
        let findings = validate_catalog(root.path(), &catalog).expect("validate catalog");

        assert_eq!(
            findings,
            ["plugin example package include list omits minco-plugin.json"]
        );
    }

    #[test]
    fn catalog_rejects_local_package_paths_that_escape_the_project() {
        let root = tempfile::tempdir().expect("temporary plugin repository");
        fs::create_dir_all(root.path().join("plugins")).expect("plugins directory");
        fs::write(
            root.path().join("plugins/catalog.toml"),
            r#"schema = 1

[[plugin]]
id = "example"
crate = "minco-plugin-example"
path = "../outside"
kind = "plugin"
feature = "plugin-example"
default_enabled = false
stability = "experimental"
description = "Example plugin."
"#,
        )
        .expect("plugin catalog");

        let error = load_catalog(root.path(), Path::new("plugins/catalog.toml"))
            .expect_err("escaping package path must fail");

        assert!(
            error
                .to_string()
                .contains("plugin example local package path must be project-relative")
        );
    }
}
