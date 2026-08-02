use crate::config::MincoManifest;
use anyhow::{Context, Result, bail};
use minco_core::{
    DistributionOperation, PluginDescriptor, PluginDistributionKind, PluginDistributionManifest,
    PluginStability, ResourceIntent,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
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

#[derive(Debug, Clone, Serialize)]
pub struct PluginWorkflowPlan {
    schema_version: u32,
    operation: &'static str,
    plugin: ResolvedPlugin,
    dry_run: bool,
    applied: bool,
    registration: PluginRegistration,
    changes: Vec<PluginChange>,
    #[serde(skip)]
    edits: Vec<PluginEdit>,
}

#[derive(Debug, Clone, Serialize)]
struct ResolvedPlugin {
    id: String,
    #[serde(rename = "crate")]
    crate_name: String,
    feature: String,
    resolved_version: String,
}

#[derive(Debug, Clone, Serialize)]
struct PluginRegistration {
    strategy: &'static str,
    composition_root: String,
    verified: bool,
}

#[derive(Debug, Clone, Serialize)]
struct PluginChange {
    path: String,
    action: &'static str,
    format: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginDoctorReport {
    schema_version: u32,
    status: DoctorStatus,
    resolved_minco_version: String,
    composition_root: String,
    checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginRemovalPlan {
    schema_version: u32,
    operation: &'static str,
    plugin: ResolvedPlugin,
    dry_run: bool,
    applied: bool,
    safe: bool,
    blockers: Vec<RemovalBlocker>,
    registration: PluginRegistration,
    changes: Vec<PluginChange>,
    #[serde(skip)]
    edits: Vec<PluginEdit>,
}

impl PluginRemovalPlan {
    pub const fn is_safe(&self) -> bool {
        self.safe
    }
}

#[derive(Debug, Clone, Serialize)]
struct RemovalBlocker {
    kind: &'static str,
    id: String,
    detail: String,
}

impl PluginDoctorReport {
    pub const fn is_passed(&self) -> bool {
        matches!(self.status, DoctorStatus::Passed)
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum DoctorStatus {
    Passed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
struct DoctorCheck {
    code: &'static str,
    status: DoctorStatus,
    findings: Vec<String>,
}

#[derive(Debug, Clone)]
struct PluginEdit {
    path: PathBuf,
    before: Vec<u8>,
    after: Vec<u8>,
}

impl PluginEdit {
    fn update(root: &Path, path: impl Into<PathBuf>, after: Vec<u8>) -> Result<Option<Self>> {
        let path = path.into();
        if path.is_absolute()
            || !path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)))
        {
            bail!("plugin workflow target must be a normalized project-relative path");
        }
        reject_symlinked_parents(root, &path)?;
        let target = root.join(&path);
        let metadata = fs::symlink_metadata(&target)
            .with_context(|| format!("inspect plugin workflow input {}", path.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            bail!(
                "plugin workflow input {} must be a regular file",
                path.display()
            );
        }
        let before = fs::read(&target)
            .with_context(|| format!("read plugin workflow input {}", path.display()))?;
        if before == after {
            return Ok(None);
        }
        Ok(Some(Self {
            path,
            before,
            after,
        }))
    }

    fn summary(&self) -> PluginChange {
        PluginChange {
            path: self.path.to_string_lossy().replace('\\', "/"),
            action: "update",
            format: "toml",
        }
    }
}

pub fn add_plugin(
    root: &Path,
    manifest: &MincoManifest,
    requested: &str,
    dry_run: bool,
) -> Result<PluginWorkflowPlan> {
    let catalog = load_catalog(root, &manifest.plugin_catalog)?;
    let matches = catalog
        .plugin
        .iter()
        .filter(|plugin| plugin.id == requested || plugin.crate_name == requested)
        .collect::<Vec<_>>();
    let plugin = match matches.as_slice() {
        [plugin] => *plugin,
        [] => bail!(
            "plugin {requested} is not present in {}; initialize its reviewed catalog metadata first",
            manifest.plugin_catalog.display()
        ),
        _ => bail!("plugin reference {requested} is ambiguous in the catalog"),
    };
    require_composable_plugin(plugin)?;
    ensure_facade_feature(root, plugin)?;
    if !minco::default_plugin_manager()?
        .descriptors()
        .iter()
        .any(|descriptor| descriptor.id.as_str() == plugin.id)
    {
        bail!(
            "Minco facade feature {} does not expose a linked constructor for plugin {}",
            plugin.feature,
            plugin.id
        );
    }
    let resolved_version = resolve_minco_version(root)?;
    ensure_cli_version_match(&resolved_version)?;
    let composition_root = locate_composition_root(root)?;

    let distribution_findings = validate_distribution_contracts(&catalog, &[]);
    if !distribution_findings.is_empty() {
        bail!(
            "plugin {} distribution is incompatible: {}",
            plugin.id,
            distribution_findings.join("; ")
        );
    }

    let mut edits = Vec::new();
    // The framework workspace owns the facade feature declaration itself. Consumer
    // applications select that feature on workspace.dependencies.minco.
    if !root.join("crates/minco/Cargo.toml").is_file() {
        let cargo_path = Path::new("Cargo.toml");
        let cargo_source =
            fs::read_to_string(root.join(cargo_path)).context("read workspace Cargo.toml")?;
        let cargo_after = update_minco_feature(&cargo_source, &plugin.feature, true)?;
        if let Some(edit) = PluginEdit::update(root, cargo_path, cargo_after.into_bytes())? {
            edits.push(edit);
        }
    }

    let manifest_path = Path::new("minco.toml");
    let manifest_after = render_plugin_selection(root, &plugin.id, true)?;
    if let Some(edit) = PluginEdit::update(root, manifest_path, manifest_after.into_bytes())? {
        edits.push(edit);
    }
    edits.sort_by(|left, right| left.path.cmp(&right.path));

    // The first supported registration strategy is deliberately narrow: official
    // catalog features are compiled into Minco's facade and selected in minco.toml.
    // No package lookup, constructor scanning, or runtime loading occurs here.
    let mut plan = PluginWorkflowPlan {
        schema_version: 1,
        operation: "add",
        plugin: ResolvedPlugin {
            id: plugin.id.clone(),
            crate_name: plugin.crate_name.clone(),
            feature: plugin.feature.clone(),
            resolved_version,
        },
        dry_run,
        applied: false,
        registration: PluginRegistration {
            strategy: "minco_facade_static_registration",
            composition_root,
            verified: true,
        },
        changes: edits.iter().map(PluginEdit::summary).collect(),
        edits,
    };
    if !dry_run {
        apply_plugin_edits(root, &plan.edits)?;
        plan.applied = true;
    }
    Ok(plan)
}

fn ensure_facade_feature(root: &Path, plugin: &PluginCatalogEntry) -> Result<()> {
    let facade_manifest = root.join("crates/minco/Cargo.toml");
    let declared = if facade_manifest.is_file() {
        let document: toml::Value = toml::from_str(
            &fs::read_to_string(&facade_manifest)
                .with_context(|| format!("read {}", facade_manifest.display()))?,
        )?;
        document
            .get("features")
            .and_then(toml::Value::as_table)
            .is_some_and(|features| features.contains_key(&plugin.feature))
    } else {
        matches!(
            plugin.feature.as_str(),
            "plugin-health"
                | "plugin-observability"
                | "plugin-idempotency"
                | "plugin-sessions"
                | "plugin-identity"
                | "plugin-object-storage"
                | "plugin-events"
                | "plugin-notifications"
                | "plugin-audit"
                | "plugin-feedback"
                | "plugin-static-site"
                | "sqlx-postgres"
                | "sqlx-sqlite"
                | "aws-adapters"
                | "aws-lambda"
                | "aws-worker"
        ) && plugin.path.is_none()
    };
    if !declared {
        bail!(
            "{} is not a Minco facade feature; register its typed constructor explicitly in the application composition root before enabling it",
            plugin.feature
        );
    }
    Ok(())
}

fn require_composable_plugin(plugin: &PluginCatalogEntry) -> Result<()> {
    if plugin.kind != PluginKind::Plugin {
        bail!(
            "{} is a {}, not a composable plugin; select adapters and runtimes through their explicit application or deployment configuration",
            plugin.id,
            plugin_kind_name(plugin.kind)
        );
    }
    Ok(())
}

fn ensure_cli_version_match(application_version: &str) -> Result<()> {
    let cli_version = env!("CARGO_PKG_VERSION");
    if application_version != cli_version {
        bail!(
            "application Minco version {application_version} does not match cargo-minco {cli_version}; use the version-matched CLI before planning plugin edits"
        );
    }
    Ok(())
}

pub fn explain_plugin(
    root: &Path,
    manifest: &MincoManifest,
    requested: &str,
) -> Result<serde_json::Value> {
    let catalog = load_catalog(root, &manifest.plugin_catalog)?;
    let matches = catalog
        .plugin
        .iter()
        .filter(|plugin| plugin.id == requested || plugin.crate_name == requested)
        .collect::<Vec<_>>();
    let plugin = match matches.as_slice() {
        [plugin] => *plugin,
        [] => bail!("unknown plugin {requested}"),
        _ => bail!("plugin reference {requested} is ambiguous in the catalog"),
    };
    let distribution = plugin.distribution.as_ref().with_context(|| {
        format!(
            "plugin {} has no locally inspectable distribution record; inspect its downloaded crate archive first",
            plugin.id
        )
    })?;
    let cost_resources = distribution
        .resources
        .iter()
        .map(|resource| {
            serde_json::json!({
                "id": resource.id,
                "idle_cost": resource.idle_cost,
                "wake_sources": resource.wake_sources,
            })
        })
        .collect::<Vec<_>>();
    let no_fixed_capacity_declared = distribution
        .resources
        .iter()
        .all(|resource| !matches!(resource.idle_cost, minco_core::IdleCostClass::FixedCapacity));
    let all_resources_zero_compute = distribution
        .resources
        .iter()
        .all(|resource| matches!(resource.idle_cost, minco_core::IdleCostClass::ZeroCompute));

    Ok(serde_json::json!({
        "schema_version": 1,
        "plugin": {
            "id": plugin.id,
            "crate": plugin.crate_name,
            "kind": plugin.kind,
            "feature": plugin.feature,
            "stability": plugin.stability,
            "plugin_version": distribution.plugin_version,
            "core_compatibility": distribution.core_compatibility,
            "runtimes": distribution.runtimes,
            "databases": distribution.databases,
            "retention": distribution.retention,
            "failure_policy": distribution.failure_policy,
            "documentation": distribution.documentation,
        },
        "capabilities": {
            "requires": distribution.requires,
            "provides": distribution.provides,
        },
        "dependencies": distribution.plugin_dependencies,
        "operations": distribution.operations,
        "migrations": distribution.migrations,
        "seeds": distribution.seeds,
        "data_classes": distribution.data_classes,
        "resources": distribution.resources,
        "cost": {
            "no_fixed_capacity_declared": no_fixed_capacity_declared,
            "all_resources_zero_compute": all_resources_zero_compute,
            "resources": cost_resources,
        },
        "configuration": distribution.configuration,
        "health_checks": distribution.health_checks,
        "conformance": distribution.conformance,
    }))
}

pub fn doctor_plugins(
    root: &Path,
    manifest: &MincoManifest,
    linked_descriptors: &[PluginDescriptor],
) -> Result<PluginDoctorReport> {
    let catalog = load_catalog(root, &manifest.plugin_catalog)?;
    let catalog_findings = validate_catalog(root, &catalog)?;
    let distribution_findings = validate_distribution_contracts(&catalog, linked_descriptors);
    let known = catalog
        .plugin
        .iter()
        .map(|plugin| plugin.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut selection_findings = manifest
        .plugins
        .enabled
        .iter()
        .chain(&manifest.plugins.disabled)
        .filter(|id| !known.contains(id.as_str()))
        .map(|id| format!("selection references unknown plugin {id}"))
        .collect::<Vec<_>>();
    selection_findings.extend(
        manifest
            .plugins
            .enabled
            .iter()
            .filter(|id| manifest.plugins.disabled.contains(*id))
            .map(|id| format!("plugin is both enabled and disabled: {id}")),
    );
    selection_findings.sort();
    selection_findings.dedup();
    let composition_findings = manifest
        .plugins
        .enabled
        .iter()
        .filter_map(|id| catalog.plugin.iter().find(|plugin| plugin.id == *id))
        .filter_map(|plugin| {
            require_composable_plugin(plugin)
                .and_then(|()| ensure_facade_feature(root, plugin))
                .and_then(|()| {
                    if linked_descriptors
                        .iter()
                        .any(|descriptor| descriptor.id.as_str() == plugin.id)
                    {
                        Ok(())
                    } else {
                        bail!(
                            "Minco facade feature {} has no linked constructor for plugin {}",
                            plugin.feature,
                            plugin.id
                        )
                    }
                })
                .err()
                .map(|error| error.to_string())
        })
        .collect::<Vec<_>>();
    let resolved_minco_version = resolve_minco_version(root)?;
    let version_findings = ensure_cli_version_match(&resolved_minco_version)
        .err()
        .map(|error| vec![error.to_string()])
        .unwrap_or_default();
    let cargo_feature_findings = selected_feature_findings(root, manifest, &catalog)?;
    let composition_root = locate_composition_root(root)?;
    let checks = vec![
        doctor_check("catalog.valid", catalog_findings),
        doctor_check("distribution.compatible", distribution_findings),
        doctor_check("selection.known", selection_findings),
        doctor_check("cargo.version_exact", version_findings),
        doctor_check("cargo.feature_selected", cargo_feature_findings),
        doctor_check("composition.static", composition_findings),
    ];
    let status = if checks
        .iter()
        .all(|check| matches!(check.status, DoctorStatus::Passed))
    {
        DoctorStatus::Passed
    } else {
        DoctorStatus::Failed
    };
    Ok(PluginDoctorReport {
        schema_version: 1,
        status,
        resolved_minco_version,
        composition_root,
        checks,
    })
}

fn selected_feature_findings(
    root: &Path,
    manifest: &MincoManifest,
    catalog: &PluginCatalog,
) -> Result<Vec<String>> {
    if root.join("crates/minco/Cargo.toml").is_file() {
        return Ok(Vec::new());
    }
    let cargo: toml::Value = toml::from_str(
        &fs::read_to_string(root.join("Cargo.toml")).context("read workspace Cargo.toml")?,
    )
    .context("parse workspace Cargo.toml")?;
    let dependency = cargo
        .get("workspace")
        .and_then(|value| value.get("dependencies"))
        .and_then(|value| value.get("minco"))
        .context("workspace.dependencies.minco is missing")?;
    let (default_features, features) = match dependency {
        toml::Value::String(_) => (true, BTreeSet::new()),
        toml::Value::Table(dependency) => {
            let default_features = dependency
                .get("default-features")
                .and_then(toml::Value::as_bool)
                .unwrap_or(true);
            let features = read_string_set(dependency.get("features"))?;
            (default_features, features)
        }
        _ => bail!("workspace.dependencies.minco must be a string or table"),
    };
    let defaults = ["health", "observability", "idempotency"]
        .into_iter()
        .collect::<BTreeSet<_>>();
    Ok(manifest
        .plugins
        .enabled
        .iter()
        .filter_map(|id| catalog.plugin.iter().find(|plugin| plugin.id == *id))
        .filter(|plugin| {
            !(features.contains(&plugin.feature)
                || default_features && defaults.contains(plugin.id.as_str()))
        })
        .map(|plugin| {
            format!(
                "enabled plugin {} requires Minco Cargo feature {}",
                plugin.id, plugin.feature
            )
        })
        .collect())
}

pub fn init_plugin(
    root: &Path,
    manifest: &MincoManifest,
    package_path: &Path,
    dry_run: bool,
) -> Result<PluginWorkflowPlan> {
    if package_path.as_os_str().is_empty()
        || package_path.is_absolute()
        || !package_path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        bail!("plugin init path must be normalized and project-relative");
    }
    let canonical_root = fs::canonicalize(root)
        .with_context(|| format!("resolve project root {}", root.display()))?;
    let package = root.join(package_path);
    let canonical_package = fs::canonicalize(&package)
        .with_context(|| format!("resolve plugin package {}", package_path.display()))?;
    if !canonical_package.starts_with(&canonical_root) {
        bail!("plugin init path resolves outside the project");
    }
    let cargo_path = canonical_package.join("Cargo.toml");
    let cargo_source = fs::read_to_string(&cargo_path)
        .with_context(|| format!("read {}", cargo_path.display()))?;
    let cargo: toml::Value =
        toml::from_str(&cargo_source).with_context(|| format!("parse {}", cargo_path.display()))?;
    let package_table = cargo
        .get("package")
        .and_then(toml::Value::as_table)
        .context("plugin Cargo.toml requires a package table")?;
    let crate_name = package_table
        .get("name")
        .and_then(toml::Value::as_str)
        .context("plugin package requires an explicit name")?;
    let package_version = resolve_package_version(root, package_table)?;
    validate_exact_version(&package_version, "plugin package")?;
    let application_version = resolve_minco_version(root)?;
    ensure_cli_version_match(&application_version)?;
    let description = package_table
        .get("description")
        .and_then(toml::Value::as_str)
        .unwrap_or("Application-owned Minco plugin.");
    let distribution_file = package_table
        .get("metadata")
        .and_then(|value| value.get("minco"))
        .and_then(|value| value.get("plugin"))
        .and_then(toml::Value::as_str)
        .context("plugin package requires package.metadata.minco.plugin")?;
    let distribution_relative = Path::new(distribution_file);
    if distribution_relative.components().count() != 1
        || distribution_relative
            .file_name()
            .and_then(|name| name.to_str())
            != Some(distribution_file)
        || !distribution_relative
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        bail!("package.metadata.minco.plugin must be one package-root JSON filename");
    }
    let distribution_path = canonical_package.join(distribution_relative);
    let metadata = fs::symlink_metadata(&distribution_path)
        .with_context(|| format!("inspect {}", distribution_path.display()))?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_DISTRIBUTION_BYTES {
        bail!("plugin distribution record must be a regular file no larger than 1 MiB");
    }
    let distribution: PluginDistributionManifest = serde_json::from_str(
        &fs::read_to_string(&distribution_path)
            .with_context(|| format!("read {}", distribution_path.display()))?,
    )
    .with_context(|| format!("parse {}", distribution_path.display()))?;
    let id = distribution.id.as_str();
    validate_plugin_id(id)?;

    let catalog = load_catalog(root, &manifest.plugin_catalog)?;
    if catalog
        .plugin
        .iter()
        .any(|plugin| plugin.id == id || plugin.crate_name == crate_name)
    {
        bail!("plugin catalog already contains {id} or crate {crate_name}");
    }
    let entry = PluginCatalogEntry {
        id: id.to_owned(),
        crate_name: crate_name.to_owned(),
        path: Some(package_path.to_owned()),
        kind: match distribution.kind {
            PluginDistributionKind::Plugin => PluginKind::Plugin,
            PluginDistributionKind::Adapter => PluginKind::Adapter,
            PluginDistributionKind::Runtime => PluginKind::Runtime,
        },
        feature: distribution.feature.clone(),
        default_enabled: distribution.default_enabled,
        stability: stability_name(distribution.stability).to_owned(),
        description: description.to_owned(),
        distribution: Some(distribution.clone()),
    };
    let candidate = PluginCatalog {
        schema: 1,
        plugin: vec![entry.clone()],
    };
    let mut findings = validate_catalog(root, &candidate)?;
    findings.extend(validate_distribution_contracts(&candidate, &[]));
    if !findings.is_empty() {
        bail!(
            "plugin {id} distribution is incompatible: {}",
            findings.join("; ")
        );
    }

    let catalog_path = &manifest.plugin_catalog;
    let catalog_source = fs::read_to_string(root.join(catalog_path))
        .with_context(|| format!("read {}", catalog_path.display()))?;
    let mut catalog_document: toml::Value = toml::from_str(&catalog_source)
        .with_context(|| format!("parse {}", catalog_path.display()))?;
    let plugins = catalog_document
        .as_table_mut()
        .context("plugin catalog root must be a table")?
        .entry("plugin")
        .or_insert_with(|| toml::Value::Array(Vec::new()))
        .as_array_mut()
        .context("plugin catalog entries must be an array")?;
    plugins.push(toml::Value::Table(toml::map::Map::from_iter([
        ("id".into(), toml::Value::String(entry.id.clone())),
        (
            "crate".into(),
            toml::Value::String(entry.crate_name.clone()),
        ),
        (
            "path".into(),
            toml::Value::String(package_path.to_string_lossy().replace('\\', "/")),
        ),
        (
            "kind".into(),
            toml::Value::String(plugin_kind_name(entry.kind).into()),
        ),
        ("feature".into(), toml::Value::String(entry.feature.clone())),
        (
            "default_enabled".into(),
            toml::Value::Boolean(entry.default_enabled),
        ),
        (
            "stability".into(),
            toml::Value::String(entry.stability.clone()),
        ),
        (
            "description".into(),
            toml::Value::String(entry.description.clone()),
        ),
    ])));
    plugins.sort_by(|left, right| {
        left.get("id")
            .and_then(toml::Value::as_str)
            .cmp(&right.get("id").and_then(toml::Value::as_str))
    });
    let catalog_after = toml::to_string_pretty(&catalog_document)
        .context("render initialized plugin catalog")?
        .into_bytes();
    let edit = PluginEdit::update(root, catalog_path, catalog_after)?
        .context("plugin init produced no catalog change")?;
    let mut plan = PluginWorkflowPlan {
        schema_version: 1,
        operation: "init",
        plugin: ResolvedPlugin {
            id: entry.id,
            crate_name: entry.crate_name,
            feature: entry.feature,
            resolved_version: package_version,
        },
        dry_run,
        applied: false,
        registration: PluginRegistration {
            strategy: "catalog_metadata_only",
            composition_root: locate_composition_root(root)?,
            verified: false,
        },
        changes: vec![edit.summary()],
        edits: vec![edit],
    };
    if !dry_run {
        apply_plugin_edits(root, &plan.edits)?;
        plan.applied = true;
    }
    Ok(plan)
}

pub fn remove_plugin(
    root: &Path,
    manifest: &MincoManifest,
    requested: &str,
    dry_run: bool,
) -> Result<PluginRemovalPlan> {
    let catalog = load_catalog(root, &manifest.plugin_catalog)?;
    let matches = catalog
        .plugin
        .iter()
        .filter(|plugin| plugin.id == requested || plugin.crate_name == requested)
        .collect::<Vec<_>>();
    let plugin = match matches.as_slice() {
        [plugin] => *plugin,
        [] => bail!("unknown plugin {requested}"),
        _ => bail!("plugin reference {requested} is ambiguous in the catalog"),
    };
    require_composable_plugin(plugin)?;
    let mut blockers = Vec::new();
    if let Some(distribution) = &plugin.distribution {
        let candidate = PluginCatalog {
            schema: catalog.schema,
            plugin: vec![plugin.clone()],
        };
        for finding in validate_distribution_contracts(&candidate, &[]) {
            blockers.push(RemovalBlocker {
                kind: "distribution_metadata",
                id: plugin.id.clone(),
                detail: finding,
            });
        }
        for dependent in catalog.plugin.iter().filter(|candidate| {
            manifest.plugins.enabled.contains(&candidate.id)
                && candidate.distribution.as_ref().is_some_and(|distribution| {
                    distribution
                        .plugin_dependencies
                        .iter()
                        .any(|dependency| dependency.as_str() == plugin.id)
                })
        }) {
            blockers.push(RemovalBlocker {
                kind: "dependent_plugin",
                id: dependent.id.clone(),
                detail: format!(
                    "enabled plugin {} declares {} as a static dependency",
                    dependent.id, plugin.id
                ),
            });
        }
        for operation in &distribution.operations {
            if manifest.operations.contains_key(&operation.operation_id) {
                blockers.push(RemovalBlocker {
                    kind: "application_operation",
                    id: operation.operation_id.clone(),
                    detail: format!(
                        "minco.toml still traces plugin operation {} {}",
                        operation.method, operation.path
                    ),
                });
            }
        }
        for migration in &distribution.migrations {
            blockers.push(RemovalBlocker {
                kind: "migration",
                id: migration.id.clone(),
                detail: format!(
                    "migration set {} may own persisted {} data",
                    migration.id, migration.database
                ),
            });
        }
        for seed in &distribution.seeds {
            blockers.push(RemovalBlocker {
                kind: "data_seed",
                id: seed.id.clone(),
                detail: format!("seed {} may have created application data", seed.id),
            });
        }
        for data_class in &distribution.data_classes {
            let id = serde_json::to_value(data_class)?
                .as_str()
                .context("data class must serialize as a string")?
                .to_owned();
            blockers.push(RemovalBlocker {
                kind: "data_class",
                id: id.clone(),
                detail: format!(
                    "plugin declares {id} data; prove retention/export/deletion before removal"
                ),
            });
        }
        for resource in &distribution.resources {
            blockers.push(RemovalBlocker {
                kind: "resource",
                id: resource.id.clone(),
                detail: format!(
                    "declared resource {} requires explicit infrastructure teardown or retention evidence before removal",
                    resource.id
                ),
            });
        }
    } else {
        blockers.push(RemovalBlocker {
            kind: "distribution_metadata",
            id: plugin.id.clone(),
            detail: "archive-visible operations, migrations, and data classes are unavailable"
                .into(),
        });
    }
    blockers
        .sort_by(|left, right| (left.kind, left.id.as_str()).cmp(&(right.kind, right.id.as_str())));

    let mut edits = Vec::new();
    if !root.join("crates/minco/Cargo.toml").is_file() {
        let cargo_source =
            fs::read_to_string(root.join("Cargo.toml")).context("read workspace Cargo.toml")?;
        let cargo_after = update_minco_feature(&cargo_source, &plugin.feature, false)?;
        if let Some(edit) = PluginEdit::update(root, "Cargo.toml", cargo_after.into_bytes())? {
            edits.push(edit);
        }
    }
    let manifest_after = render_plugin_selection(root, &plugin.id, false)?;
    if let Some(edit) = PluginEdit::update(root, "minco.toml", manifest_after.into_bytes())? {
        edits.push(edit);
    }
    edits.sort_by(|left, right| left.path.cmp(&right.path));
    let safe = blockers.is_empty();
    let resolved_version = resolve_minco_version(root)?;
    ensure_cli_version_match(&resolved_version)?;
    let facade_verified = ensure_facade_feature(root, plugin).is_ok()
        && minco::default_plugin_manager()?
            .descriptors()
            .iter()
            .any(|descriptor| descriptor.id.as_str() == plugin.id);
    let mut plan = PluginRemovalPlan {
        schema_version: 1,
        operation: "remove",
        plugin: ResolvedPlugin {
            id: plugin.id.clone(),
            crate_name: plugin.crate_name.clone(),
            feature: plugin.feature.clone(),
            resolved_version,
        },
        dry_run,
        applied: false,
        safe,
        blockers,
        registration: PluginRegistration {
            strategy: if facade_verified {
                "minco_facade_static_registration"
            } else {
                "application_explicit_registration"
            },
            composition_root: locate_composition_root(root)?,
            verified: facade_verified,
        },
        changes: edits.iter().map(PluginEdit::summary).collect(),
        edits,
    };
    if !dry_run && safe {
        apply_plugin_edits(root, &plan.edits)?;
        plan.applied = true;
    }
    Ok(plan)
}

pub fn set_plugin_state_workflow(
    root: &Path,
    manifest: &MincoManifest,
    requested: &str,
    enabled: bool,
    operation: &'static str,
    dry_run: bool,
) -> Result<PluginWorkflowPlan> {
    let catalog = load_catalog(root, &manifest.plugin_catalog)?;
    let matches = catalog
        .plugin
        .iter()
        .filter(|plugin| plugin.id == requested || plugin.crate_name == requested)
        .collect::<Vec<_>>();
    let plugin = match matches.as_slice() {
        [plugin] => *plugin,
        [] => bail!("unknown plugin {requested}"),
        _ => bail!("plugin reference {requested} is ambiguous in the catalog"),
    };
    require_composable_plugin(plugin)?;
    let resolved_version = resolve_minco_version(root)?;
    ensure_cli_version_match(&resolved_version)?;
    let facade_verified = ensure_facade_feature(root, plugin).is_ok()
        && minco::default_plugin_manager()?
            .descriptors()
            .iter()
            .any(|descriptor| descriptor.id.as_str() == plugin.id);
    let after = render_plugin_selection(root, &plugin.id, enabled)?.into_bytes();
    let edits = PluginEdit::update(root, "minco.toml", after)?
        .into_iter()
        .collect::<Vec<_>>();
    let mut plan = PluginWorkflowPlan {
        schema_version: 1,
        operation,
        plugin: ResolvedPlugin {
            id: plugin.id.clone(),
            crate_name: plugin.crate_name.clone(),
            feature: plugin.feature.clone(),
            resolved_version,
        },
        dry_run,
        applied: false,
        registration: PluginRegistration {
            strategy: if facade_verified {
                "minco_facade_static_registration"
            } else {
                "application_explicit_registration"
            },
            composition_root: locate_composition_root(root)?,
            verified: facade_verified,
        },
        changes: edits.iter().map(PluginEdit::summary).collect(),
        edits,
    };
    if !dry_run {
        apply_plugin_edits(root, &plan.edits)?;
        plan.applied = true;
    }
    Ok(plan)
}

fn resolve_package_version(
    root: &Path,
    package: &toml::map::Map<String, toml::Value>,
) -> Result<String> {
    match package.get("version") {
        Some(toml::Value::String(version)) => Ok(version.clone()),
        Some(toml::Value::Table(value))
            if value.get("workspace").and_then(toml::Value::as_bool) == Some(true) =>
        {
            let workspace: toml::Value = toml::from_str(
                &fs::read_to_string(root.join("Cargo.toml"))
                    .context("read workspace Cargo.toml")?,
            )?;
            workspace
                .get("workspace")
                .and_then(|value| value.get("package"))
                .and_then(|value| value.get("version"))
                .and_then(toml::Value::as_str)
                .map(str::to_owned)
                .context("workspace.package.version is required for version.workspace")
        }
        _ => bail!("plugin package requires an explicit version or version.workspace = true"),
    }
}

const fn doctor_check(code: &'static str, findings: Vec<String>) -> DoctorCheck {
    DoctorCheck {
        code,
        status: if findings.is_empty() {
            DoctorStatus::Passed
        } else {
            DoctorStatus::Failed
        },
        findings,
    }
}

fn update_minco_feature(source: &str, feature: &str, add: bool) -> Result<String> {
    let mut in_workspace_dependencies = false;
    let mut matched = None;
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_workspace_dependencies = trimmed == "[workspace.dependencies]";
        } else if in_workspace_dependencies
            && trimmed
                .split_once('=')
                .is_some_and(|(name, _)| name.trim() == "minco")
        {
            if matched.is_some() {
                bail!("workspace.dependencies contains multiple minco entries");
            }
            matched = Some((offset, offset + line.len(), line));
        }
        offset += line.len();
    }
    let (start, end, line) = matched.context("workspace.dependencies.minco is missing")?;
    let newline = if line.ends_with("\r\n") {
        "\r\n"
    } else if line.ends_with('\n') {
        "\n"
    } else {
        ""
    };
    let content = line.trim_end_matches(['\r', '\n']);
    if content.contains('#') {
        bail!(
            "workspace.dependencies.minco has an inline comment; refusing an ambiguous Cargo.toml edit"
        );
    }
    let (_name, raw_value) = content
        .split_once('=')
        .context("workspace.dependencies.minco must use a standard dependency declaration")?;
    let value = raw_value.trim();
    let rendered_value = update_dependency_feature_value(value, feature, add)?;
    let indentation = &content[..content.len() - content.trim_start().len()];
    let replacement = format!("{indentation}minco = {rendered_value}{newline}");
    let mut rendered = String::with_capacity(source.len() + feature.len() + 20);
    rendered.push_str(&source[..start]);
    rendered.push_str(&replacement);
    rendered.push_str(&source[end..]);
    Ok(rendered)
}

fn update_dependency_feature_value(value: &str, feature: &str, add: bool) -> Result<String> {
    if value.starts_with('"') {
        if !add {
            return Ok(value.to_owned());
        }
        return Ok(format!(
            "{{ version = {value}, features = [\"{feature}\"] }}"
        ));
    }
    if !(value.starts_with('{') && value.ends_with('}')) {
        bail!("workspace.dependencies.minco must be a version string or one-line inline table");
    }
    let features_start = value.match_indices("features").find_map(|(index, _)| {
        let valid_prefix = index == 0
            || value[..index]
                .bytes()
                .next_back()
                .is_some_and(|byte| matches!(byte, b'{' | b',' | b' ' | b'\t'));
        let suffix = value[index + "features".len()..].trim_start();
        (valid_prefix && suffix.starts_with('=')).then_some(index)
    });
    let Some(features_start) = features_start else {
        if !add {
            return Ok(value.to_owned());
        }
        let body = value[..value.len() - 1].trim_end();
        let separator = if body.ends_with('{') { " " } else { ", " };
        return Ok(format!("{body}{separator}features = [\"{feature}\"] }}"));
    };
    let relative_open = value[features_start..]
        .find('[')
        .context("workspace.dependencies.minco features must be an array")?;
    let open = features_start + relative_open;
    let close = value[open..]
        .find(']')
        .map(|index| open + index)
        .context("workspace.dependencies.minco features array is not closed")?;
    let array_source = &value[open..=close];
    let mut features: Vec<String> =
        toml::from_str::<toml::Value>(&format!("features = {array_source}"))?
            .get("features")
            .and_then(toml::Value::as_array)
            .context("workspace.dependencies.minco features must be an array")?
            .iter()
            .map(|entry| {
                entry
                    .as_str()
                    .map(str::to_owned)
                    .context("workspace.dependencies.minco features must contain strings")
            })
            .collect::<Result<_>>()?;
    if add {
        if !features.iter().any(|candidate| candidate == feature) {
            features.push(feature.to_owned());
        }
    } else {
        features.retain(|candidate| candidate != feature);
    }
    features.sort();
    features.dedup();
    let rendered_array = format!(
        "[{}]",
        features
            .iter()
            .map(|feature| format!("\"{feature}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(format!(
        "{}{}{}",
        &value[..open],
        rendered_array,
        &value[close + 1..]
    ))
}

fn render_plugin_selection(root: &Path, plugin_id: &str, enabled: bool) -> Result<String> {
    let path = root.join("minco.toml");
    let source = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let mut document: toml::Value =
        toml::from_str(&source).with_context(|| format!("parse {}", path.display()))?;
    let plugins = document
        .as_table_mut()
        .context("minco.toml root must be a table")?
        .entry("plugins")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .context("plugins must be a table")?;
    let mut enabled_set = read_string_set(plugins.get("enabled"))?;
    let mut disabled_set = read_string_set(plugins.get("disabled"))?;
    let already_selected = if enabled {
        enabled_set.contains(plugin_id) && !disabled_set.contains(plugin_id)
    } else {
        disabled_set.contains(plugin_id) && !enabled_set.contains(plugin_id)
    };
    if already_selected {
        return Ok(source);
    }
    if enabled {
        enabled_set.insert(plugin_id.into());
        disabled_set.remove(plugin_id);
    } else {
        disabled_set.insert(plugin_id.into());
        enabled_set.remove(plugin_id);
    }
    plugins.insert("enabled".into(), string_array(enabled_set));
    plugins.insert("disabled".into(), string_array(disabled_set));
    toml::to_string_pretty(&document).context("render minco.toml plugin selection")
}

fn apply_plugin_edits(root: &Path, edits: &[PluginEdit]) -> Result<()> {
    for edit in edits {
        reject_symlinked_parents(root, &edit.path)?;
        let target = root.join(&edit.path);
        let metadata = fs::symlink_metadata(&target)
            .with_context(|| format!("preflight plugin workflow target {}", edit.path.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            bail!(
                "plugin workflow target {} changed type before apply",
                edit.path.display()
            );
        }
        if fs::read(&target)? != edit.before {
            bail!(
                "plugin workflow target {} changed after planning; no files were written",
                edit.path.display()
            );
        }
    }
    for edit in edits {
        fs::write(root.join(&edit.path), &edit.after)
            .with_context(|| format!("write plugin workflow target {}", edit.path.display()))?;
    }
    Ok(())
}

fn reject_symlinked_parents(root: &Path, relative: &Path) -> Result<()> {
    let Some(parent) = relative.parent() else {
        return Ok(());
    };
    let mut current = root.to_owned();
    for component in parent.components() {
        let std::path::Component::Normal(component) = component else {
            bail!("plugin workflow target must be normalized");
        };
        current.push(component);
        let metadata = fs::symlink_metadata(&current)
            .with_context(|| format!("inspect plugin workflow parent {}", current.display()))?;
        if metadata.file_type().is_symlink() {
            bail!(
                "plugin workflow refuses symlinked parent {}",
                current.display()
            );
        }
        if !metadata.is_dir() {
            bail!(
                "plugin workflow parent {} must be a directory",
                current.display()
            );
        }
    }
    Ok(())
}

fn resolve_minco_version(root: &Path) -> Result<String> {
    let path = root.join("Cargo.toml");
    let document: toml::Value = toml::from_str(
        &std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?,
    )
    .with_context(|| format!("parse {}", path.display()))?;
    let version = document
        .get("workspace")
        .and_then(|value| value.get("dependencies"))
        .and_then(|value| value.get("minco"))
        .and_then(|value| match value {
            toml::Value::String(version) => Some(version.as_str()),
            toml::Value::Table(dependency) => dependency.get("version")?.as_str(),
            _ => None,
        })
        .context("workspace.dependencies.minco must declare an explicit version")?;
    validate_exact_version(version, "workspace.dependencies.minco")?;
    Ok(version.to_owned())
}

fn validate_exact_version(version: &str, owner: &str) -> Result<()> {
    let parsed = version
        .parse()
        .with_context(|| format!("{owner} version {version:?} must be an exact SemVer version"))?;
    // The constructor's typed version parameter lets this crate use the exact
    // semver parser already exposed by minco-core without a second dependency.
    let id = minco_core::PluginId::new("version-check")
        .context("internal exact-version parser ID must remain valid")?;
    let _descriptor = PluginDescriptor::new(id, parsed, "Cargo version syntax check");
    Ok(())
}

fn locate_composition_root(root: &Path) -> Result<String> {
    let supported = [
        ("services/app/src/lib.rs", "minco::compose_defaults"),
        ("crates/minco/src/lib.rs", "register_enabled_plugins"),
        (
            "examples/orders/service/src/lib.rs",
            "PluginManager::default",
        ),
    ];
    for (relative, marker) in supported {
        let path = root.join(relative);
        if path.is_file() {
            let source = fs::read_to_string(&path)
                .with_context(|| format!("read composition root {relative}"))?;
            if !source.contains(marker) {
                bail!(
                    "composition root {relative} does not contain the expected static registration marker {marker}"
                );
            }
            return Ok(relative.to_owned());
        }
    }
    bail!("could not verify an explicit Minco composition root; expected services/app/src/lib.rs")
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
    fn exact_cargo_versions_use_the_semver_parser() {
        for version in ["0.6.0", "0.6.0-beta.1", "0.6.0+build.7"] {
            validate_exact_version(version, "fixture").expect("exact SemVer version");
        }
        for version in ["0.6", "^0.6.0", "01.6.0", "0.6.0-!", "0.6.0 beta"] {
            assert!(
                validate_exact_version(version, "fixture").is_err(),
                "accepted invalid or non-exact version {version}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn plugin_edits_reject_symlinked_parent_directories() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("temporary project");
        fs::create_dir(root.path().join("real")).expect("real directory");
        fs::write(root.path().join("real/catalog.toml"), "schema = 1\n").expect("catalog fixture");
        symlink(root.path().join("real"), root.path().join("linked"))
            .expect("symlinked catalog parent");

        let error =
            PluginEdit::update(root.path(), "linked/catalog.toml", b"schema = 2\n".to_vec())
                .expect_err("symlinked parent must fail before planning");

        assert!(error.to_string().contains("symlinked parent"));
        assert_eq!(
            fs::read_to_string(root.path().join("real/catalog.toml")).expect("unchanged catalog"),
            "schema = 1\n"
        );
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
