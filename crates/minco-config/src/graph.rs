use crate::{
    ConfigDiagnostic, ConfigLayer, ConfigSourceKind, ConfigurationError, ConfigurationSchema,
    ConfigurationValueKind, SecretReference, value_matches,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

/// Fail-closed operational environment class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentClass {
    Local,
    Test,
    Development,
    Staging,
    Production,
}

/// Named environment and its operational class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    pub name: String,
    pub class: EnvironmentClass,
}

impl Environment {
    pub fn new(name: impl Into<String>, class: EnvironmentClass) -> Self {
        Self {
            name: name.into(),
            class,
        }
    }
}

/// Public provenance for one effective field. It contains no field value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigProvenance {
    pub source_kind: ConfigSourceKind,
    pub source: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrode: Vec<ConfigSource>,
}

/// One earlier source in an override chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigSource {
    pub source_kind: ConfigSourceKind,
    pub source: String,
}

/// Secret-safe explanation of one effective field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigExplanation {
    pub path: String,
    pub kind: ConfigurationValueKind,
    pub required: bool,
    pub secret: bool,
    pub redacted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    pub provenance: ConfigProvenance,
}

/// Secret-safe deterministic difference between two effective graphs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigDiff {
    pub from: Environment,
    pub to: Environment,
    pub from_digest: String,
    pub to_digest: String,
    pub changes: Vec<ConfigDiffEntry>,
}

/// One changed effective field. Secret values and reference names are omitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigDiffEntry {
    pub path: String,
    pub secret: bool,
    pub redacted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<Value>,
}

#[derive(Clone, PartialEq, Eq)]
struct EffectiveEntry {
    value: Value,
    provenance: ConfigProvenance,
}

/// Validated, typed and deterministically digested effective configuration.
#[derive(Clone, PartialEq, Eq)]
pub struct ConfigurationGraph {
    environment: Environment,
    schema: ConfigurationSchema,
    entries: BTreeMap<String, EffectiveEntry>,
    digest: String,
}

impl ConfigurationGraph {
    pub fn compile(
        schema: &ConfigurationSchema,
        environment: Environment,
        layers: impl IntoIterator<Item = ConfigLayer>,
    ) -> Result<Self, ConfigurationError> {
        let mut diagnostics = Vec::new();
        if !valid_environment_name(&environment.name) {
            diagnostics.push(ConfigDiagnostic::new(
                "config.invalid_environment",
                "environment name must be a bounded lowercase identifier",
            ));
        }

        let mut layers: Vec<_> = layers.into_iter().collect();
        layers.sort_by_key(|layer| layer.source_kind.precedence());
        validate_layer_set(&environment, &layers, &mut diagnostics);

        let mut entries = BTreeMap::new();
        for (path, field) in schema.fields() {
            if let Some(default) = &field.default {
                entries.insert(
                    path.to_owned(),
                    EffectiveEntry {
                        value: default.clone(),
                        provenance: ConfigProvenance {
                            source_kind: ConfigSourceKind::CompiledDefault,
                            source: "compiled default".into(),
                            overrode: Vec::new(),
                        },
                    },
                );
            }
        }

        for layer in &layers {
            let flattened = flatten_layer(schema, layer, &mut diagnostics);
            for (path, raw_value) in flattened {
                let Some(field) = schema.field(&path) else {
                    diagnostics.push(
                        ConfigDiagnostic::new(
                            "config.unknown_field",
                            "field is not declared by the application or an enabled plugin",
                        )
                        .with_path(path)
                        .with_source(&layer.source),
                    );
                    continue;
                };
                let value = if field.secret {
                    let Some(reference) = raw_value.as_str() else {
                        diagnostics.push(
                            ConfigDiagnostic::new(
                                "config.secret_reference_required",
                                "secret fields accept only env:NAME or ssm:/absolute/name references",
                            )
                            .with_path(path)
                            .with_source(&layer.source),
                        );
                        continue;
                    };
                    match SecretReference::parse(reference).and_then(|reference| {
                        serde_json::to_value(reference)
                            .map_err(|_| crate::SecretReferenceError::UnsupportedSyntax)
                    }) {
                        Ok(value) => value,
                        Err(error) => {
                            diagnostics.push(
                                ConfigDiagnostic::new(
                                    "config.invalid_secret_reference",
                                    error.to_string(),
                                )
                                .with_path(path)
                                .with_source(&layer.source),
                            );
                            continue;
                        }
                    }
                } else if value_matches(field.kind, &raw_value) {
                    raw_value
                } else {
                    diagnostics.push(
                        ConfigDiagnostic::new(
                            "config.type_mismatch",
                            format!("value does not match {:?}", field.kind),
                        )
                        .with_path(path)
                        .with_source(&layer.source),
                    );
                    continue;
                };

                let mut overrode = Vec::new();
                if let Some(previous) = entries.get(&path) {
                    overrode.extend(previous.provenance.overrode.clone());
                    overrode.push(ConfigSource {
                        source_kind: previous.provenance.source_kind,
                        source: previous.provenance.source.clone(),
                    });
                }
                entries.insert(
                    path,
                    EffectiveEntry {
                        value,
                        provenance: ConfigProvenance {
                            source_kind: layer.source_kind,
                            source: layer.source.clone(),
                            overrode,
                        },
                    },
                );
            }
        }

        for (path, field) in schema.fields() {
            if field.required && !entries.contains_key(path) {
                diagnostics.push(
                    ConfigDiagnostic::new(
                        "config.required_field_missing",
                        "required field has no effective value",
                    )
                    .with_path(path),
                );
            }
        }
        if !diagnostics.is_empty() {
            return Err(ConfigurationError::new(diagnostics));
        }

        let digest = effective_digest(&environment, &entries).map_err(|_| {
            ConfigurationError::new(vec![ConfigDiagnostic::new(
                "config.digest_encoding",
                "effective configuration could not be encoded deterministically",
            )])
        })?;
        Ok(Self {
            environment,
            schema: schema.clone(),
            entries,
            digest,
        })
    }

    pub const fn environment(&self) -> &Environment {
        &self.environment
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }

    pub fn explain(&self, path: &str) -> Option<ConfigExplanation> {
        let field = self.schema.field(path)?;
        let entry = self.entries.get(path)?;
        Some(ConfigExplanation {
            path: path.into(),
            kind: field.kind,
            required: field.required,
            secret: field.secret,
            redacted: field.secret,
            value: (!field.secret).then(|| entry.value.clone()),
            provenance: entry.provenance.clone(),
        })
    }

    pub fn explanations(&self) -> Vec<ConfigExplanation> {
        self.schema
            .fields()
            .filter_map(|(path, _)| self.explain(path))
            .collect()
    }

    pub fn diff(&self, other: &Self) -> ConfigDiff {
        let paths = self
            .entries
            .keys()
            .chain(other.entries.keys())
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let changes = paths
            .into_iter()
            .filter_map(|path| {
                let before = self.entries.get(path);
                let after = other.entries.get(path);
                if before.map(|entry| &entry.value) == after.map(|entry| &entry.value) {
                    return None;
                }
                let secret = self.schema.field(path).is_some_and(|field| field.secret)
                    || other.schema.field(path).is_some_and(|field| field.secret);
                Some(ConfigDiffEntry {
                    path: path.into(),
                    secret,
                    redacted: secret,
                    before: (!secret)
                        .then(|| before.map(|entry| entry.value.clone()))
                        .flatten(),
                    after: (!secret)
                        .then(|| after.map(|entry| entry.value.clone()))
                        .flatten(),
                })
            })
            .collect();
        ConfigDiff {
            from: self.environment.clone(),
            to: other.environment.clone(),
            from_digest: self.digest.clone(),
            to_digest: other.digest.clone(),
            changes,
        }
    }

    /// Deserialize one namespace at the application composition boundary.
    pub fn deserialize_namespace<T: DeserializeOwned>(
        &self,
        namespace: &str,
    ) -> Result<T, ConfigurationError> {
        let prefix = format!("{namespace}.");
        let known = self.schema.field(namespace).is_some()
            || self
                .schema
                .fields()
                .any(|(path, _)| path.starts_with(&prefix));
        let secret_bearing = self
            .schema
            .field(namespace)
            .is_some_and(|field| field.secret)
            || self
                .schema
                .fields()
                .any(|(path, field)| path.starts_with(&prefix) && field.secret);
        if !known {
            return Err(ConfigurationError::new(vec![
                ConfigDiagnostic::new(
                    "config.unknown_namespace",
                    "typed constructor requested an undeclared configuration namespace",
                )
                .with_path(namespace),
            ]));
        }
        if let Some(entry) = self.entries.get(namespace) {
            return deserialize_typed(namespace, entry.value.clone(), secret_bearing);
        }
        let mut root = Map::new();
        for (path, entry) in &self.entries {
            if let Some(relative) = path.strip_prefix(&prefix) {
                insert_nested(&mut root, relative, entry.value.clone());
            }
        }
        deserialize_typed(namespace, Value::Object(root), secret_bearing)
    }

    pub const fn schema(&self) -> &ConfigurationSchema {
        &self.schema
    }
}

impl fmt::Debug for ConfigurationGraph {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfigurationGraph")
            .field("environment", &self.environment)
            .field("digest", &self.digest)
            .field("field_count", &self.entries.len())
            .finish_non_exhaustive()
    }
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

fn validate_layer_set(
    environment: &Environment,
    layers: &[ConfigLayer],
    diagnostics: &mut Vec<ConfigDiagnostic>,
) {
    let mut kinds = BTreeSet::new();
    for layer in layers {
        if !kinds.insert(layer.source_kind) {
            diagnostics.push(
                ConfigDiagnostic::new(
                    "config.duplicate_layer",
                    "each precedence class may be supplied at most once",
                )
                .with_source(&layer.source),
            );
        }
        match layer.source_kind {
            ConfigSourceKind::CompiledDefault => diagnostics.push(
                ConfigDiagnostic::new(
                    "config.compiled_layer_forbidden",
                    "compiled defaults are declared by the schema, not supplied as a layer",
                )
                .with_source(&layer.source),
            ),
            ConfigSourceKind::EnvironmentFile => match layer.environment_class {
                None => diagnostics.push(
                    ConfigDiagnostic::new(
                        "config.environment_class_missing",
                        "environment files must declare environment_class",
                    )
                    .with_source(&layer.source),
                ),
                Some(class) if class != environment.class => diagnostics.push(
                    ConfigDiagnostic::new(
                        "config.environment_class_mismatch",
                        format!(
                            "file declares {class:?} but selected environment is {:?}",
                            environment.class
                        ),
                    )
                    .with_source(&layer.source),
                ),
                Some(_) => {}
            },
            ConfigSourceKind::LocalOverride
                if !matches!(
                    environment.class,
                    EnvironmentClass::Local | EnvironmentClass::Development
                ) =>
            {
                diagnostics.push(
                    ConfigDiagnostic::new(
                        "config.local_override_forbidden",
                        "local overrides are permitted only for local and development classes",
                    )
                    .with_source(&layer.source),
                );
            }
            _ if layer.environment_class.is_some() => diagnostics.push(
                ConfigDiagnostic::new(
                    "config.environment_class_unexpected",
                    "only an environment file may declare environment_class",
                )
                .with_source(&layer.source),
            ),
            _ => {}
        }
    }
}

fn flatten_layer(
    schema: &ConfigurationSchema,
    layer: &ConfigLayer,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) -> BTreeMap<String, Value> {
    let mut result = BTreeMap::new();
    for (key, value) in &layer.values {
        flatten_value(schema, key, value, &mut result, diagnostics, &layer.source);
    }
    result
}

fn flatten_value(
    schema: &ConfigurationSchema,
    path: &str,
    value: &Value,
    result: &mut BTreeMap<String, Value>,
    diagnostics: &mut Vec<ConfigDiagnostic>,
    source: &str,
) {
    if schema
        .field(path)
        .is_some_and(|field| field.kind == ConfigurationValueKind::Object && !field.secret)
    {
        insert_flattened(result, path, value, diagnostics, source);
        return;
    }
    if let Some(object) = value.as_object() {
        if object.is_empty() {
            diagnostics.push(
                ConfigDiagnostic::new(
                    "config.empty_table",
                    "empty configuration tables cannot select a typed field",
                )
                .with_path(path)
                .with_source(source),
            );
            return;
        }
        for (key, nested) in object {
            flatten_value(
                schema,
                &format!("{path}.{key}"),
                nested,
                result,
                diagnostics,
                source,
            );
        }
    } else {
        insert_flattened(result, path, value, diagnostics, source);
    }
}

fn insert_flattened(
    result: &mut BTreeMap<String, Value>,
    path: &str,
    value: &Value,
    diagnostics: &mut Vec<ConfigDiagnostic>,
    source: &str,
) {
    if result.insert(path.into(), value.clone()).is_some() {
        diagnostics.push(
            ConfigDiagnostic::new(
                "config.duplicate_field",
                "one configuration layer resolves multiple values to the same path",
            )
            .with_path(path)
            .with_source(source),
        );
    }
}

fn effective_digest(
    environment: &Environment,
    entries: &BTreeMap<String, EffectiveEntry>,
) -> Result<String, serde_json::Error> {
    #[derive(Serialize)]
    struct DigestMaterial<'a> {
        schema: u32,
        environment: &'a Environment,
        values: BTreeMap<&'a str, &'a Value>,
    }

    let values = entries
        .iter()
        .map(|(path, entry)| (path.as_str(), &entry.value))
        .collect();
    let bytes = serde_json::to_vec(&DigestMaterial {
        schema: ConfigurationSchema::SCHEMA_VERSION,
        environment,
        values,
    })?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn deserialize_typed<T: DeserializeOwned>(
    namespace: &str,
    value: Value,
    secret_bearing: bool,
) -> Result<T, ConfigurationError> {
    serde_json::from_value(value).map_err(|error| {
        let message = if secret_bearing {
            "typed constructor rejected a secret-bearing namespace".into()
        } else {
            format!("typed constructor rejected namespace: {error}")
        };
        ConfigurationError::new(vec![
            ConfigDiagnostic::new("config.typed_deserialization", message).with_path(namespace),
        ])
    })
}

fn insert_nested(root: &mut Map<String, Value>, path: &str, value: Value) {
    let mut segments = path.split('.').peekable();
    let mut current = root;
    while let Some(segment) = segments.next() {
        if segments.peek().is_none() {
            current.insert(segment.into(), value);
            return;
        }
        let entry = current
            .entry(segment)
            .or_insert_with(|| Value::Object(Map::new()));
        current = entry
            .as_object_mut()
            .expect("schema paths cannot overlap scalar and object fields");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConfigurationField, ConfigurationValueKind};
    use serde_json::json;

    #[derive(Debug)]
    struct EchoesInputOnFailure;

    impl<'de> Deserialize<'de> for EchoesInputOnFailure {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            let value = Value::deserialize(deserializer)?;
            Err(serde::de::Error::custom(format!(
                "typed constructor rejected {value}"
            )))
        }
    }

    #[test]
    fn caller_layer_order_does_not_change_precedence() {
        let schema = ConfigurationSchema::try_from_fields([ConfigurationField {
            key: "application.name".into(),
            kind: ConfigurationValueKind::String,
            required: true,
            secret: false,
            description: "Name".into(),
            default: Some(json!("compiled")),
        }])
        .unwrap();
        let environment = Environment::new("dev", EnvironmentClass::Development);
        let cli = ConfigLayer::from_pairs(
            ConfigSourceKind::CliOverride,
            "cli",
            [("application.name", json!("cli"))],
        )
        .unwrap();
        let file = ConfigLayer::from_pairs(
            ConfigSourceKind::DefaultFile,
            "default",
            [("application.name", json!("file"))],
        )
        .unwrap();
        let graph =
            ConfigurationGraph::compile(&schema, environment, [cli, file]).expect("valid graph");
        assert_eq!(
            graph.explain("application.name").unwrap().value,
            Some(json!("cli"))
        );
    }

    #[test]
    fn one_layer_cannot_repeat_a_path_with_nested_and_dotted_keys() {
        let schema = ConfigurationSchema::try_from_fields([ConfigurationField {
            key: "application.name".into(),
            kind: ConfigurationValueKind::String,
            required: true,
            secret: false,
            description: "Name".into(),
            default: None,
        }])
        .unwrap();
        let layer = ConfigLayer::from_pairs(
            ConfigSourceKind::DefaultFile,
            "default",
            [
                ("application", json!({ "name": "nested" })),
                ("application.name", json!("dotted")),
            ],
        )
        .unwrap();

        let error = ConfigurationGraph::compile(
            &schema,
            Environment::new("dev", EnvironmentClass::Development),
            [layer],
        )
        .unwrap_err();
        assert_eq!(error.diagnostics()[0].code, "config.duplicate_field");
    }

    #[test]
    fn local_override_policy_is_environment_class_specific() {
        let schema = ConfigurationSchema::try_from_fields([ConfigurationField {
            key: "application.name".into(),
            kind: ConfigurationValueKind::String,
            required: true,
            secret: false,
            description: "Name".into(),
            default: Some(json!("compiled")),
        }])
        .unwrap();
        for class in [
            EnvironmentClass::Test,
            EnvironmentClass::Staging,
            EnvironmentClass::Production,
        ] {
            let layer = ConfigLayer::from_pairs(
                ConfigSourceKind::LocalOverride,
                ".local.toml",
                [("application.name", json!("local"))],
            )
            .unwrap();
            let error =
                ConfigurationGraph::compile(&schema, Environment::new("protected", class), [layer])
                    .unwrap_err();
            assert_eq!(
                error.diagnostics()[0].code,
                "config.local_override_forbidden"
            );
        }
        for class in [EnvironmentClass::Local, EnvironmentClass::Development] {
            let layer = ConfigLayer::from_pairs(
                ConfigSourceKind::LocalOverride,
                ".local.toml",
                [("application.name", json!("local"))],
            )
            .unwrap();
            assert!(
                ConfigurationGraph::compile(&schema, Environment::new("mutable", class), [layer])
                    .is_ok()
            );
        }
    }

    #[test]
    fn diff_redacts_when_either_schema_marks_a_field_secret() {
        let public_schema = ConfigurationSchema::try_from_fields([ConfigurationField {
            key: "integration.token".into(),
            kind: ConfigurationValueKind::String,
            required: true,
            secret: false,
            description: "Legacy public field".into(),
            default: Some(json!("PUBLIC_TOKEN_SHOULD_NOT_LEAK")),
        }])
        .unwrap();
        let secret_schema = ConfigurationSchema::try_from_fields([ConfigurationField {
            key: "integration.token".into(),
            kind: ConfigurationValueKind::String,
            required: true,
            secret: true,
            description: "Secret reference".into(),
            default: None,
        }])
        .unwrap();
        let before = ConfigurationGraph::compile(
            &public_schema,
            Environment::new("development", EnvironmentClass::Development),
            [],
        )
        .unwrap();
        let after = ConfigurationGraph::compile(
            &secret_schema,
            Environment::new("after", EnvironmentClass::Production),
            [ConfigLayer::from_pairs(
                ConfigSourceKind::EnvironmentVariables,
                "environment",
                [("integration.token", json!("env:INTEGRATION_TOKEN"))],
            )
            .unwrap()],
        )
        .unwrap();

        let rendered = serde_json::to_string(&before.diff(&after)).unwrap();
        assert!(!rendered.contains("PUBLIC_TOKEN_SHOULD_NOT_LEAK"));
        assert!(!rendered.contains("INTEGRATION_TOKEN"));
    }

    #[test]
    fn typed_constructor_rejects_unknown_namespaces() {
        let schema = ConfigurationSchema::try_from_fields([ConfigurationField {
            key: "application.name".into(),
            kind: ConfigurationValueKind::String,
            required: true,
            secret: false,
            description: "Name".into(),
            default: Some(json!("orders")),
        }])
        .unwrap();
        let graph = ConfigurationGraph::compile(
            &schema,
            Environment::new("test", EnvironmentClass::Test),
            [],
        )
        .unwrap();
        let error = graph
            .deserialize_namespace::<serde_json::Value>("applicaiton")
            .unwrap_err();
        assert_eq!(error.diagnostics()[0].code, "config.unknown_namespace");
    }

    #[test]
    fn typed_constructor_errors_redact_secret_bearing_namespaces() {
        let schema = ConfigurationSchema::try_from_fields([ConfigurationField {
            key: "database.url".into(),
            kind: ConfigurationValueKind::String,
            required: true,
            secret: true,
            description: "Database secret reference".into(),
            default: None,
        }])
        .unwrap();
        let graph = ConfigurationGraph::compile(
            &schema,
            Environment::new("test", EnvironmentClass::Test),
            [ConfigLayer::from_pairs(
                ConfigSourceKind::EnvironmentVariables,
                "environment",
                [("database.url", json!("env:DO_NOT_LEAK"))],
            )
            .unwrap()],
        )
        .unwrap();

        let error = graph
            .deserialize_namespace::<EchoesInputOnFailure>("database")
            .unwrap_err();
        let diagnostic = &error.diagnostics()[0];
        assert_eq!(diagnostic.code, "config.typed_deserialization");
        assert!(!diagnostic.message.contains("DO_NOT_LEAK"));
        assert!(!format!("{diagnostic:?}").contains("DO_NOT_LEAK"));
    }
}
