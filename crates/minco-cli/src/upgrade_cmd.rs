use crate::{UpgradeCommand, print_value};
use anyhow::{Context, Result, bail};
use minco_config::ConfigurationSchema;
use minco_contract::load_contract_source;
use serde::Serialize;
use std::{
    fs,
    path::{Component, Path},
};

const SUPPORTED_MANIFEST_SCHEMA: u32 = 1;
const SUPPORTED_PLUGIN_CATALOG_SCHEMA: u32 = 1;
const SUPPORTED_DEPLOYMENT_SCHEMAS: [u32; 2] = [1, 2];

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum UpgradeAssessment {
    ReviewRequired,
}

#[derive(Debug, Serialize)]
struct UpgradeReport {
    schema_version: u32,
    application: String,
    assessment: UpgradeAssessment,
    rust: RustBoundary,
    cli: CliBoundary,
    cargo_features: CargoFeatureBoundary,
    configuration: ConfigurationBoundary,
    plugins: PluginBoundary,
    serialized: SerializedBoundary,
    diagnostics: Vec<UpgradeDiagnostic>,
    limitations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RustBoundary {
    application_minimum: Option<String>,
    cli_minimum: String,
    public_api_assessment: &'static str,
}

#[derive(Debug, Serialize)]
struct CliBoundary {
    version: String,
    interface_assessment: &'static str,
    reviewed_commands: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct CargoFeatureBoundary {
    minco_dependency_requirement: Option<String>,
    default_features: Option<bool>,
    selected: Vec<String>,
    assessment: &'static str,
}

#[derive(Debug, Serialize)]
struct ConfigurationBoundary {
    supported_schema_version: u32,
    fields: Vec<ConfigurationFieldBoundary>,
    assessment: &'static str,
}

#[derive(Debug, Serialize)]
struct ConfigurationFieldBoundary {
    key: String,
    kind: Option<String>,
    required: Option<bool>,
    secret: Option<bool>,
}

#[derive(Debug, Serialize)]
struct PluginBoundary {
    catalog_schema_version: Option<u32>,
    enabled: Vec<String>,
    disabled: Vec<String>,
    linked: Vec<LinkedPluginBoundary>,
    assessment: &'static str,
}

#[derive(Debug, Serialize)]
struct LinkedPluginBoundary {
    id: String,
    version: String,
    core_compatibility: String,
}

#[derive(Debug, Default, Serialize)]
struct SerializedBoundary {
    manifest_schema_version: Option<u32>,
    deployment_plan_schema_version: Option<u32>,
    contract_openapi_version: Option<String>,
    contract_info_version: Option<String>,
    contract_sha256: Option<String>,
}

#[derive(Debug, Serialize)]
struct UpgradeDiagnostic {
    code: String,
    severity: &'static str,
    boundary: &'static str,
    message: String,
}

pub fn execute(root: &Path, command: UpgradeCommand, as_json: bool) -> Result<()> {
    match command {
        UpgradeCommand::Report => print_value(&build_report(root)?, as_json),
    }
}

fn build_report(root: &Path) -> Result<UpgradeReport> {
    let cargo: toml::Value = parse_toml(root.join("Cargo.toml"), "Cargo workspace manifest")?;
    let manifest: toml::Value = parse_toml(root.join("minco.toml"), "Minco manifest")?;
    let mut diagnostics = Vec::new();

    let manifest_schema = manifest.get("schema").and_then(toml_u32);
    if manifest_schema != Some(SUPPORTED_MANIFEST_SCHEMA) {
        diagnostics.push(UpgradeDiagnostic {
            code: "upgrade.manifest_schema.unsupported".into(),
            severity: "warning",
            boundary: "serialized",
            message: format!(
                "minco.toml declares schema {manifest_schema:?}; this CLI supports schema {SUPPORTED_MANIFEST_SCHEMA}"
            ),
        });
    }

    let application = manifest
        .get("name")
        .and_then(toml::Value::as_str)
        .unwrap_or("<unknown>")
        .to_owned();
    let application_rust = cargo
        .get("workspace")
        .and_then(|value| value.get("package"))
        .and_then(|value| value.get("rust-version"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned);
    let minco_dependency = cargo
        .get("workspace")
        .and_then(|value| value.get("dependencies"))
        .and_then(|value| value.get("minco"));
    let (minco_requirement, default_features, mut selected_features) =
        cargo_feature_boundary(minco_dependency);
    selected_features.sort();
    selected_features.dedup();

    let mut fields = configuration_fields(&manifest);
    fields.sort_by(|left, right| left.key.cmp(&right.key));
    let enabled = string_array_at(&manifest, &["plugins", "enabled"]);
    let disabled = string_array_at(&manifest, &["plugins", "disabled"]);

    let mut serialized = SerializedBoundary {
        manifest_schema_version: manifest_schema,
        ..SerializedBoundary::default()
    };
    let catalog_schema = observe_plugin_catalog(root, &manifest, &mut diagnostics);
    observe_contract(root, &manifest, &mut serialized, &mut diagnostics);
    observe_deployment(root, &manifest, &mut serialized, &mut diagnostics);

    let mut linked = minco::default_plugin_manager()?
        .descriptors()
        .into_iter()
        .map(|descriptor| LinkedPluginBoundary {
            id: descriptor.id.to_string(),
            version: descriptor.version.to_string(),
            core_compatibility: descriptor.core_compatibility.to_string(),
        })
        .collect::<Vec<_>>();
    linked.sort_by(|left, right| left.id.cmp(&right.id));
    diagnostics.sort_by(|left, right| left.code.cmp(&right.code));

    Ok(UpgradeReport {
        schema_version: 1,
        application,
        assessment: UpgradeAssessment::ReviewRequired,
        rust: RustBoundary {
            application_minimum: application_rust,
            cli_minimum: env!("CARGO_PKG_RUST_VERSION").into(),
            public_api_assessment: "review_required",
        },
        cli: CliBoundary {
            version: env!("CARGO_PKG_VERSION").into(),
            interface_assessment: "review_required",
            reviewed_commands: vec!["cargo minco contract diff", "cargo minco upgrade report"],
        },
        cargo_features: CargoFeatureBoundary {
            minco_dependency_requirement: minco_requirement,
            default_features,
            selected: selected_features,
            assessment: "review_required",
        },
        configuration: ConfigurationBoundary {
            supported_schema_version: ConfigurationSchema::SCHEMA_VERSION,
            fields,
            assessment: "review_required",
        },
        plugins: PluginBoundary {
            catalog_schema_version: catalog_schema,
            enabled,
            disabled,
            linked,
            assessment: "review_required",
        },
        serialized,
        diagnostics,
        limitations: vec![
            "This inventory compares declared application boundaries with the running Minco CLI; it does not prove Rust, CLI, or semantic API compatibility.".into(),
            "Configuration defaults and values are intentionally excluded; secret references and secret values are never reported.".into(),
            "Deployment behavior, persisted data, migrations, and runtime compatibility require separate evidence.".into(),
        ],
    })
}

fn parse_toml(path: impl AsRef<Path>, label: &str) -> Result<toml::Value> {
    let path = path.as_ref();
    toml::from_str(
        &fs::read_to_string(path).with_context(|| format!("read {label} {}", path.display()))?,
    )
    .with_context(|| format!("parse {label} {}", path.display()))
}

fn cargo_feature_boundary(
    dependency: Option<&toml::Value>,
) -> (Option<String>, Option<bool>, Vec<String>) {
    match dependency {
        Some(toml::Value::String(version)) => (Some(version.clone()), Some(true), Vec::new()),
        Some(toml::Value::Table(table)) => (
            table
                .get("version")
                .and_then(toml::Value::as_str)
                .map(str::to_owned),
            table
                .get("default-features")
                .and_then(toml::Value::as_bool)
                .or(Some(true)),
            table
                .get("features")
                .and_then(toml::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(toml::Value::as_str)
                .map(str::to_owned)
                .collect(),
        ),
        _ => (None, None, Vec::new()),
    }
}

fn configuration_fields(manifest: &toml::Value) -> Vec<ConfigurationFieldBoundary> {
    manifest
        .get("configuration")
        .and_then(|value| value.get("fields"))
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_table)
        .filter_map(|field| {
            Some(ConfigurationFieldBoundary {
                key: field.get("key")?.as_str()?.to_owned(),
                kind: field
                    .get("kind")
                    .and_then(toml::Value::as_str)
                    .map(str::to_owned),
                required: field.get("required").and_then(toml::Value::as_bool),
                secret: field.get("secret").and_then(toml::Value::as_bool),
            })
        })
        .collect()
}

fn string_array_at(document: &toml::Value, path: &[&str]) -> Vec<String> {
    let mut value = Some(document);
    for segment in path {
        value = value.and_then(|current| current.get(*segment));
    }
    let mut output = value
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    output.sort();
    output.dedup();
    output
}

fn observe_plugin_catalog(
    root: &Path,
    manifest: &toml::Value,
    diagnostics: &mut Vec<UpgradeDiagnostic>,
) -> Option<u32> {
    let path = manifest
        .get("plugin_catalog")
        .and_then(toml::Value::as_str)?;
    let source = match read_project_file(root, path, "plugin catalog") {
        Ok(source) => source,
        Err(error) => {
            diagnostics.push(observation_error(
                "upgrade.plugin_catalog.unavailable",
                "plugins",
                &error,
            ));
            return None;
        }
    };
    let catalog: toml::Value = if let Ok(catalog) = toml::from_str(&source) {
        catalog
    } else {
        diagnostics.push(UpgradeDiagnostic {
            code: "upgrade.plugin_catalog.invalid".into(),
            severity: "warning",
            boundary: "plugins",
            message: "the declared plugin catalog is not valid TOML".into(),
        });
        return None;
    };
    let schema = catalog.get("schema").and_then(toml_u32);
    if schema != Some(SUPPORTED_PLUGIN_CATALOG_SCHEMA) {
        diagnostics.push(UpgradeDiagnostic {
            code: "upgrade.plugin_catalog_schema.unsupported".into(),
            severity: "warning",
            boundary: "plugins",
            message: format!(
                "plugin catalog declares schema {schema:?}; this CLI supports schema {SUPPORTED_PLUGIN_CATALOG_SCHEMA}"
            ),
        });
    }
    schema
}

fn observe_contract(
    root: &Path,
    manifest: &toml::Value,
    serialized: &mut SerializedBoundary,
    diagnostics: &mut Vec<UpgradeDiagnostic>,
) {
    let Some(path) = manifest.get("contract").and_then(toml::Value::as_str) else {
        return;
    };
    let source = match read_project_file(root, path, "OpenAPI contract") {
        Ok(source) => source,
        Err(error) => {
            diagnostics.push(observation_error(
                "upgrade.contract.unavailable",
                "serialized",
                &error,
            ));
            return;
        }
    };
    match load_contract_source(path, &source) {
        Ok(report) => {
            let valid = report.is_valid();
            serialized.contract_openapi_version = Some(report.document.openapi_version);
            serialized.contract_info_version = Some(report.document.version);
            serialized.contract_sha256 = Some(report.document.sha256);
            if !valid {
                diagnostics.push(UpgradeDiagnostic {
                    code: "upgrade.contract.invalid".into(),
                    severity: "warning",
                    boundary: "serialized",
                    message: "the declared OpenAPI contract has validation findings".into(),
                });
            }
        }
        Err(error) => diagnostics.push(observation_error(
            "upgrade.contract.invalid",
            "serialized",
            &error,
        )),
    }
}

fn observe_deployment(
    root: &Path,
    manifest: &toml::Value,
    serialized: &mut SerializedBoundary,
    diagnostics: &mut Vec<UpgradeDiagnostic>,
) {
    let Some(path) = manifest
        .get("deployment_config")
        .and_then(toml::Value::as_str)
    else {
        return;
    };
    let source = match read_project_file(root, path, "deployment config") {
        Ok(source) => source,
        Err(error) => {
            diagnostics.push(observation_error(
                "upgrade.deployment.unavailable",
                "serialized",
                &error,
            ));
            return;
        }
    };
    let deployment: toml::Value = if let Ok(deployment) = toml::from_str(&source) {
        deployment
    } else {
        diagnostics.push(UpgradeDiagnostic {
            code: "upgrade.deployment.invalid".into(),
            severity: "warning",
            boundary: "serialized",
            message: "the declared deployment config is not valid TOML".into(),
        });
        return;
    };
    let schema = deployment.get("schema_version").and_then(toml_u32);
    serialized.deployment_plan_schema_version = schema;
    if schema.is_some_and(|value| !SUPPORTED_DEPLOYMENT_SCHEMAS.contains(&value)) {
        diagnostics.push(UpgradeDiagnostic {
            code: "upgrade.deployment_schema.unsupported".into(),
            severity: "warning",
            boundary: "serialized",
            message: format!(
                "deployment config declares schema {schema:?}; this CLI supports schemas {SUPPORTED_DEPLOYMENT_SCHEMAS:?}"
            ),
        });
    }
}

fn read_project_file(root: &Path, relative: &str, label: &str) -> Result<String> {
    let relative = Path::new(relative);
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        bail!("{label} must be a project-relative path");
    }
    let path = fs::canonicalize(root.join(relative))
        .with_context(|| format!("resolve {label} {}", relative.display()))?;
    if !path.starts_with(root) || !path.is_file() {
        bail!("{label} must resolve to a file inside the project");
    }
    fs::read_to_string(&path).with_context(|| format!("read {label} {}", path.display()))
}

fn observation_error(
    code: &str,
    boundary: &'static str,
    error: &dyn std::fmt::Display,
) -> UpgradeDiagnostic {
    UpgradeDiagnostic {
        code: code.into(),
        severity: "warning",
        boundary,
        message: error.to_string(),
    }
}

fn toml_u32(value: &toml::Value) -> Option<u32> {
    u32::try_from(value.as_integer()?).ok()
}
