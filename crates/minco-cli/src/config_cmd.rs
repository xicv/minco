use crate::{config::MincoManifest, load_plugin_selection, print_value};
use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use minco_config::{
    ConfigDiagnostic, ConfigLayer, ConfigSourceKind, ConfigurationError, ConfigurationGraph,
    ConfigurationSchema, Environment,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Component, Path},
};

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Validate one effective environment and print its deterministic digest.
    Check(ConfigEnvironmentArgs),
    /// Explain one field's redacted value and complete override provenance.
    Explain {
        path: String,
        #[command(flatten)]
        input: ConfigEnvironmentArgs,
    },
    /// Compare two validated environment graphs without exposing secrets.
    Diff {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },
    /// Print the application and statically linked plugin schema.
    Schema,
}

#[derive(Debug, Clone, Args)]
pub struct ConfigEnvironmentArgs {
    #[arg(long, default_value = "dev")]
    pub environment: String,
    /// Highest-precedence typed override in KEY=JSON-or-string form.
    #[arg(long = "set")]
    pub overrides: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ConfigCheckReport {
    schema: u32,
    valid: bool,
    environment: String,
    environment_class: minco_config::EnvironmentClass,
    digest: String,
    diagnostics: Vec<ConfigDiagnostic>,
}

#[derive(Debug, Serialize)]
struct ConfigFailureReport {
    schema: u32,
    valid: bool,
    environment: String,
    diagnostics: Vec<ConfigDiagnostic>,
}

pub fn execute(
    root: &Path,
    manifest: &MincoManifest,
    command: ConfigCommand,
    as_json: bool,
) -> Result<()> {
    match command {
        ConfigCommand::Check(input) => {
            let environment = input.environment.clone();
            match load_graph(root, manifest, &input.environment, &input.overrides) {
                Ok(graph) => print_value(
                    &ConfigCheckReport {
                        schema: ConfigurationSchema::SCHEMA_VERSION,
                        valid: true,
                        environment: graph.environment().name.clone(),
                        environment_class: graph.environment().class,
                        digest: graph.digest().into(),
                        diagnostics: Vec::new(),
                    },
                    as_json,
                ),
                Err(diagnostics) => fail(environment, diagnostics, as_json),
            }
        }
        ConfigCommand::Explain { path, input } => {
            let environment = input.environment.clone();
            let graph = match load_graph(root, manifest, &input.environment, &input.overrides) {
                Ok(graph) => graph,
                Err(diagnostics) => return fail(environment, diagnostics, as_json),
            };
            let Some(explanation) = graph.explain(&path) else {
                return fail(
                    environment,
                    vec![ConfigDiagnostic {
                        code: "config.explain.unknown_field".into(),
                        message: "field is absent from the effective configuration".into(),
                        path: Some(path),
                        source: None,
                    }],
                    as_json,
                );
            };
            print_value(
                &json!({
                    "schema": ConfigurationSchema::SCHEMA_VERSION,
                    "environment": graph.environment(),
                    "digest": graph.digest(),
                    "explanation": explanation,
                }),
                as_json,
            )
        }
        ConfigCommand::Diff { from, to } => {
            let from_graph = match load_graph(root, manifest, &from, &[]) {
                Ok(graph) => graph,
                Err(diagnostics) => return fail(from, diagnostics, as_json),
            };
            let to_graph = match load_graph(root, manifest, &to, &[]) {
                Ok(graph) => graph,
                Err(diagnostics) => return fail(to, diagnostics, as_json),
            };
            print_value(&from_graph.diff(&to_graph), as_json)
        }
        ConfigCommand::Schema => match application_schema(manifest, true) {
            Ok(schema) => print_value(&schema, as_json),
            Err(error) => fail("schema".into(), error.diagnostics().to_vec(), as_json),
        },
    }
}

pub fn load_graph(
    root: &Path,
    manifest: &MincoManifest,
    environment_name: &str,
    cli_overrides: &[String],
) -> Result<ConfigurationGraph, Vec<ConfigDiagnostic>> {
    if !valid_environment_name(environment_name) {
        return Err(vec![ConfigDiagnostic {
            code: "config.invalid_environment".into(),
            message: "environment names must be lowercase identifiers".into(),
            path: None,
            source: None,
        }]);
    }
    validate_profile_layout(manifest)?;
    let schema =
        application_schema(manifest, false).map_err(|error| error.diagnostics().to_vec())?;
    let configuration_root = root.join(&manifest.configuration.root);
    let default_path = configuration_root.join(&manifest.configuration.default_file);
    let environment_path = configuration_root.join(format!("{environment_name}.toml"));
    let default = load_layer(root, &default_path, ConfigSourceKind::DefaultFile)?;
    let environment_layer = load_layer(root, &environment_path, ConfigSourceKind::EnvironmentFile)?;
    let environment_class = environment_layer.environment_class().ok_or_else(|| {
        vec![ConfigDiagnostic {
            code: "config.environment_class_missing".into(),
            message: "environment files must declare environment_class".into(),
            path: None,
            source: Some(display_path(root, &environment_path)),
        }]
    })?;
    if let Some(expected) = canonical_environment_class(environment_name)
        && environment_class != expected
    {
        return Err(vec![ConfigDiagnostic {
            code: "config.environment_class_mismatch".into(),
            message: format!(
                "profile {environment_name} requires {expected:?}, found {environment_class:?}"
            ),
            path: None,
            source: Some(display_path(root, &environment_path)),
        }]);
    }
    let mut layers = vec![default, environment_layer];

    let local_path = configuration_root.join(&manifest.configuration.local_override);
    if local_path.is_file() {
        layers.push(load_layer(
            root,
            &local_path,
            ConfigSourceKind::LocalOverride,
        )?);
    }
    let environment_values =
        read_environment_overrides(&manifest.configuration.environment_prefix)?;
    if !environment_values.is_empty() {
        layers.push(
            ConfigLayer::from_pairs(
                ConfigSourceKind::EnvironmentVariables,
                format!("{}*", manifest.configuration.environment_prefix),
                environment_values,
            )
            .map_err(|error| layer_diagnostic(&error))?,
        );
    }
    if !cli_overrides.is_empty() {
        layers.push(
            ConfigLayer::from_pairs(
                ConfigSourceKind::CliOverride,
                "command line",
                parse_overrides(cli_overrides)?,
            )
            .map_err(|error| layer_diagnostic(&error))?,
        );
    }

    ConfigurationGraph::compile(
        &schema,
        Environment::new(environment_name, environment_class),
        layers,
    )
    .map_err(|error| error.diagnostics().to_vec())
}

fn application_schema(
    manifest: &MincoManifest,
    include_all_linked_plugins: bool,
) -> Result<ConfigurationSchema, ConfigurationError> {
    let application = ConfigurationSchema::try_from_fields(manifest.configuration.fields.clone())?;
    let manager = minco::default_plugin_manager().map_err(|error| {
        ConfigurationError::from_diagnostic(ConfigDiagnostic {
            code: "config.plugin_schema".into(),
            message: error.to_string(),
            path: None,
            source: None,
        })
    })?;
    if include_all_linked_plugins {
        return application.with_plugin_descriptors(manager.descriptors());
    }
    let selection = load_plugin_selection(manifest, &manager).map_err(|error| {
        ConfigurationError::from_diagnostic(ConfigDiagnostic {
            code: "config.plugin_selection".into(),
            message: error.to_string(),
            path: None,
            source: None,
        })
    })?;
    let descriptors = manager.enabled_descriptors(&selection).map_err(|error| {
        ConfigurationError::from_diagnostic(ConfigDiagnostic {
            code: "config.plugin_selection".into(),
            message: error.to_string(),
            path: None,
            source: None,
        })
    })?;
    application.with_plugin_descriptors(descriptors)
}

fn load_layer(
    root: &Path,
    path: &Path,
    source_kind: ConfigSourceKind,
) -> Result<ConfigLayer, Vec<ConfigDiagnostic>> {
    let source = display_path(root, path);
    let canonical = fs::canonicalize(path).map_err(|error| {
        vec![ConfigDiagnostic {
            code: "config.file_read".into(),
            message: error.to_string(),
            path: None,
            source: Some(source.clone()),
        }]
    })?;
    if !canonical.starts_with(root) {
        return Err(vec![ConfigDiagnostic {
            code: "config.profile_path".into(),
            message: "configuration files must remain inside the project root".into(),
            path: None,
            source: Some(source),
        }]);
    }
    let document = fs::read_to_string(&canonical).map_err(|error| {
        vec![ConfigDiagnostic {
            code: "config.file_read".into(),
            message: error.to_string(),
            path: None,
            source: Some(source.clone()),
        }]
    })?;
    ConfigLayer::from_toml(source_kind, source.clone(), &document).map_err(|error| {
        vec![ConfigDiagnostic {
            code: "config.file_parse".into(),
            message: error.to_string(),
            path: None,
            source: Some(source),
        }]
    })
}

fn validate_profile_layout(manifest: &MincoManifest) -> Result<(), Vec<ConfigDiagnostic>> {
    let root_is_safe = !manifest.configuration.root.as_os_str().is_empty()
        && manifest
            .configuration
            .root
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    let default_is_safe = safe_filename(&manifest.configuration.default_file);
    let local_is_safe = safe_filename(&manifest.configuration.local_override);
    if root_is_safe && default_is_safe && local_is_safe {
        return Ok(());
    }
    Err(vec![ConfigDiagnostic {
        code: "config.profile_path".into(),
        message: "configuration root must be project-relative and profile files must be filenames"
            .into(),
        path: None,
        source: Some("minco.toml".into()),
    }])
}

fn safe_filename(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn layer_diagnostic(error: &minco_config::ConfigLayerError) -> Vec<ConfigDiagnostic> {
    vec![ConfigDiagnostic {
        code: "config.duplicate_field".into(),
        message: error.to_string(),
        path: None,
        source: None,
    }]
}

fn read_environment_overrides(
    prefix: &str,
) -> Result<BTreeMap<String, Value>, Vec<ConfigDiagnostic>> {
    normalize_environment_overrides(prefix, std::env::vars_os())
}

fn normalize_environment_overrides(
    prefix: &str,
    variables: impl IntoIterator<Item = (OsString, OsString)>,
) -> Result<BTreeMap<String, Value>, Vec<ConfigDiagnostic>> {
    let stem = prefix.strip_suffix("__");
    if prefix.len() < 3
        || stem.is_none_or(str::is_empty)
        || !stem.is_some_and(|value| value.as_bytes()[0].is_ascii_uppercase())
        || !prefix
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(vec![ConfigDiagnostic {
            code: "config.environment_prefix".into(),
            message: "environment_prefix must be an uppercase identifier ending in __".into(),
            path: None,
            source: Some("minco.toml".into()),
        }]);
    }
    let mut variables = variables
        .into_iter()
        .filter(|(name, _)| name.to_string_lossy().starts_with(prefix))
        .collect::<Vec<_>>();
    variables.sort_by(|left, right| left.0.cmp(&right.0));

    let mut values = BTreeMap::new();
    for (name, raw) in variables {
        let name = name.into_string().map_err(|_| {
            vec![ConfigDiagnostic {
                code: "config.environment_override_name".into(),
                message: "configuration override names must be valid UTF-8".into(),
                path: None,
                source: Some(format!("{prefix}*")),
            }]
        })?;
        let Some(suffix) = name.strip_prefix(prefix) else {
            continue;
        };
        let Some(raw) = raw.to_str() else {
            return Err(vec![ConfigDiagnostic {
                code: "config.environment_override_value".into(),
                message: "configuration override values must be valid UTF-8".into(),
                path: None,
                source: Some(name),
            }]);
        };
        let path = suffix
            .split("__")
            .map(str::to_ascii_lowercase)
            .collect::<Vec<_>>()
            .join(".");
        if path.split('.').any(str::is_empty) {
            return Err(vec![ConfigDiagnostic {
                code: "config.environment_override_name".into(),
                message: "override names require non-empty __-separated segments".into(),
                path: None,
                source: Some(name),
            }]);
        }
        if values.insert(path.clone(), parse_value(raw)).is_some() {
            return Err(vec![ConfigDiagnostic {
                code: "config.duplicate_field".into(),
                message: "environment overrides normalize to the same path".into(),
                path: Some(path),
                source: Some(name),
            }]);
        }
    }
    Ok(values)
}

fn parse_overrides(overrides: &[String]) -> Result<BTreeMap<String, Value>, Vec<ConfigDiagnostic>> {
    let mut values = BTreeMap::new();
    for assignment in overrides {
        let Some((path, raw)) = assignment.split_once('=') else {
            return Err(vec![ConfigDiagnostic {
                code: "config.cli_override".into(),
                message: "CLI overrides must use KEY=JSON-or-string".into(),
                path: None,
                source: Some("command line".into()),
            }]);
        };
        if path.is_empty() {
            return Err(vec![ConfigDiagnostic {
                code: "config.cli_override".into(),
                message: "CLI override paths must not be empty".into(),
                path: None,
                source: Some("command line".into()),
            }]);
        }
        if values.insert(path.into(), parse_value(raw)).is_some() {
            return Err(vec![ConfigDiagnostic {
                code: "config.duplicate_field".into(),
                message: "CLI override repeats a path".into(),
                path: Some(path.into()),
                source: Some("command line".into()),
            }]);
        }
    }
    Ok(values)
}

fn parse_value(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.into()))
}

fn fail(environment: String, diagnostics: Vec<ConfigDiagnostic>, as_json: bool) -> Result<()> {
    print_value(
        &ConfigFailureReport {
            schema: ConfigurationSchema::SCHEMA_VERSION,
            valid: false,
            environment,
            diagnostics,
        },
        as_json,
    )?;
    bail!("configuration validation failed")
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn valid_environment_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    value.len() <= 64
        && first.is_ascii_lowercase()
        && bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
}

fn canonical_environment_class(environment: &str) -> Option<minco_config::EnvironmentClass> {
    use minco_config::EnvironmentClass;
    match environment {
        "local" => Some(EnvironmentClass::Local),
        "test" => Some(EnvironmentClass::Test),
        "dev" | "development" => Some(EnvironmentClass::Development),
        "staging" => Some(EnvironmentClass::Staging),
        "prod" | "production" => Some(EnvironmentClass::Production),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository() -> (std::path::PathBuf, MincoManifest) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("repository root");
        let manifest = MincoManifest::load(&root).expect("repository manifest");
        (root, manifest)
    }

    #[test]
    fn enabled_schema_does_not_require_disabled_plugin_fields() {
        let (root, manifest) = repository();
        let graph = load_graph(&root, &manifest, "dev", &[]).expect("valid dev graph");
        let paths = graph
            .schema()
            .fields()
            .map(|(path, _)| path)
            .collect::<Vec<_>>();
        assert!(paths.contains(&"plugins.idempotency.claim_timeout_seconds"));
        assert!(!paths.contains(&"plugins.feedback.project_id"));
    }

    #[test]
    fn command_diff_never_serializes_secret_reference_names() {
        let (root, manifest) = repository();
        let development = load_graph(&root, &manifest, "dev", &[]).expect("valid dev graph");
        let production =
            load_graph(&root, &manifest, "production", &[]).expect("valid production graph");
        let output = serde_json::to_string(&development.diff(&production)).expect("JSON diff");
        assert!(!output.contains("MINCO_DEV_DATABASE_URL"));
        assert!(!output.contains("/minco/orders/production/database-url"));
    }

    #[test]
    fn environment_names_cannot_escape_the_profile_root() {
        let (root, manifest) = repository();
        let diagnostics =
            load_graph(&root, &manifest, "../../minco", &[]).expect_err("invalid environment");
        assert_eq!(diagnostics[0].code, "config.invalid_environment");
        assert_ne!(diagnostics[0].code, "config.file_read");
    }

    #[test]
    fn configuration_paths_cannot_escape_the_project_root() {
        let (root, mut manifest) = repository();
        manifest.configuration.root = "../../outside".into();
        let diagnostics =
            load_graph(&root, &manifest, "dev", &[]).expect_err("invalid profile root");
        assert_eq!(diagnostics[0].code, "config.profile_path");

        manifest.configuration.root = "examples/orders/config/environments".into();
        manifest.configuration.default_file = "../default.toml".into();
        let diagnostics =
            load_graph(&root, &manifest, "dev", &[]).expect_err("invalid default filename");
        assert_eq!(diagnostics[0].code, "config.profile_path");
    }

    #[test]
    fn normalized_environment_collisions_are_deterministic() {
        let diagnostics = normalize_environment_overrides(
            "MINCO_CONFIG__",
            [
                (
                    std::ffi::OsString::from("MINCO_CONFIG__application__name"),
                    std::ffi::OsString::from("second"),
                ),
                (
                    std::ffi::OsString::from("MINCO_CONFIG__APPLICATION__NAME"),
                    std::ffi::OsString::from("first"),
                ),
            ],
        )
        .unwrap_err();
        assert_eq!(diagnostics[0].code, "config.duplicate_field");
        assert_eq!(
            diagnostics[0].source.as_deref(),
            Some("MINCO_CONFIG__application__name")
        );
    }

    #[cfg(unix)]
    #[test]
    fn prefixed_non_utf8_environment_names_fail_closed() {
        use std::os::unix::ffi::OsStringExt;

        let mut name = b"MINCO_CONFIG__APPLICATION__".to_vec();
        name.push(0xff);
        let diagnostics = normalize_environment_overrides(
            "MINCO_CONFIG__",
            [(
                std::ffi::OsString::from_vec(name),
                std::ffi::OsString::from("orders"),
            )],
        )
        .unwrap_err();
        assert_eq!(diagnostics[0].code, "config.environment_override_name");
    }

    #[test]
    fn cli_overrides_reject_duplicate_paths() {
        let diagnostics = parse_overrides(&[
            "application.name=first".into(),
            "application.name=second".into(),
        ])
        .unwrap_err();
        assert_eq!(diagnostics[0].code, "config.duplicate_field");
    }
}
