use crate::{ConfigDiagnostic, ConfigurationError, value_matches};
pub use minco_core::{ConfigurationField, ConfigurationValueKind};
use serde::Serialize;
use std::collections::{BTreeMap, btree_map::Entry};

/// Strict, deterministic application and plugin configuration schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigurationSchema {
    schema: u32,
    fields: BTreeMap<String, ConfigurationField>,
}

impl ConfigurationSchema {
    pub const SCHEMA_VERSION: u32 = 1;

    pub fn try_from_fields(
        fields: impl IntoIterator<Item = ConfigurationField>,
    ) -> Result<Self, ConfigurationError> {
        let mut schema = Self {
            schema: Self::SCHEMA_VERSION,
            fields: BTreeMap::new(),
        };
        let mut diagnostics = Vec::new();
        for field in fields {
            schema.insert_field("", field, &mut diagnostics);
        }
        if diagnostics.is_empty() {
            Ok(schema)
        } else {
            Err(ConfigurationError::new(diagnostics))
        }
    }

    /// Add descriptor fields below a stable namespace such as
    /// `plugins.idempotency`.
    pub fn with_namespace(
        mut self,
        namespace: &str,
        fields: impl IntoIterator<Item = ConfigurationField>,
    ) -> Result<Self, ConfigurationError> {
        let mut diagnostics = Vec::new();
        for field in fields {
            self.insert_field(namespace, field, &mut diagnostics);
        }
        if diagnostics.is_empty() {
            Ok(self)
        } else {
            Err(ConfigurationError::new(diagnostics))
        }
    }

    /// Integrate statically linked plugin descriptors below their stable
    /// configuration namespaces.
    pub fn with_plugin_descriptors(
        mut self,
        descriptors: impl IntoIterator<Item = minco_core::PluginDescriptor>,
    ) -> Result<Self, ConfigurationError> {
        for descriptor in descriptors {
            self = self.with_namespace(
                &descriptor.configuration_namespace,
                descriptor.configuration,
            )?;
        }
        Ok(self)
    }

    pub fn fields(&self) -> impl Iterator<Item = (&str, &ConfigurationField)> {
        self.fields
            .iter()
            .map(|(path, field)| (path.as_str(), field))
    }

    pub(crate) fn field(&self, path: &str) -> Option<&ConfigurationField> {
        self.fields.get(path)
    }

    fn insert_field(
        &mut self,
        namespace: &str,
        mut field: ConfigurationField,
        diagnostics: &mut Vec<ConfigDiagnostic>,
    ) {
        let path = if namespace.is_empty() {
            field.key.clone()
        } else {
            format!("{namespace}.{}", field.key)
        };
        if !valid_path(&path) {
            diagnostics.push(
                ConfigDiagnostic::new(
                    "config.schema.invalid_path",
                    "configuration paths must contain non-empty lowercase identifier segments",
                )
                .with_path(path),
            );
            return;
        }
        if field.description.trim().is_empty() {
            diagnostics.push(
                ConfigDiagnostic::new(
                    "config.schema.missing_description",
                    "configuration fields require a description",
                )
                .with_path(path),
            );
            return;
        }
        if field.secret && field.kind != ConfigurationValueKind::String {
            diagnostics.push(
                ConfigDiagnostic::new(
                    "config.schema.secret_reference_kind",
                    "secret fields must use the string kind for opaque references",
                )
                .with_path(path),
            );
            return;
        }
        if field.secret && field.default.is_some() {
            diagnostics.push(
                ConfigDiagnostic::new(
                    "config.schema.secret_default",
                    "secret fields cannot carry compiled secret values or references as defaults",
                )
                .with_path(path),
            );
            return;
        }
        if let Some(default) = &field.default
            && !value_matches(field.kind, default)
        {
            diagnostics.push(
                ConfigDiagnostic::new(
                    "config.schema.default_type",
                    format!("default does not match {:?}", field.kind),
                )
                .with_path(path),
            );
            return;
        }
        if self.fields.keys().any(|existing| {
            path.strip_prefix(existing)
                .is_some_and(|suffix| suffix.starts_with('.'))
                || existing
                    .strip_prefix(&path)
                    .is_some_and(|suffix| suffix.starts_with('.'))
        }) {
            diagnostics.push(
                ConfigDiagnostic::new(
                    "config.schema.overlapping_field",
                    "scalar/object configuration paths must not overlap",
                )
                .with_path(path),
            );
            return;
        }
        field.key.clone_from(&path);
        match self.fields.entry(path.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(field);
            }
            Entry::Occupied(_) => diagnostics.push(
                ConfigDiagnostic::new(
                    "config.schema.duplicate_field",
                    "configuration field is declared more than once",
                )
                .with_path(path),
            ),
        }
    }
}

fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.split('.').all(|segment| {
            let mut bytes = segment.bytes();
            let Some(first) = bytes.next() else {
                return false;
            };
            (first.is_ascii_lowercase() || first == b'_')
                && bytes.all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'_'
                        || byte == b'-'
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_core::{PluginDescriptor, PluginId};
    use semver::Version;
    use serde_json::json;

    #[test]
    fn schema_rejects_secret_defaults_and_duplicate_paths() {
        let field = ConfigurationField {
            key: "database.url".into(),
            kind: ConfigurationValueKind::String,
            required: true,
            secret: true,
            description: "Database secret reference".into(),
            default: Some(json!("a-value")),
        };
        let error = ConfigurationSchema::try_from_fields([field]).unwrap_err();
        assert_eq!(error.diagnostics()[0].code, "config.schema.secret_default");
    }

    #[test]
    fn secret_fields_require_the_reference_string_kind() {
        let error = ConfigurationSchema::try_from_fields([ConfigurationField {
            key: "database.port".into(),
            kind: ConfigurationValueKind::Integer,
            required: true,
            secret: true,
            description: "Invalid secret field kind".into(),
            default: None,
        }])
        .unwrap_err();
        assert_eq!(
            error.diagnostics()[0].code,
            "config.schema.secret_reference_kind"
        );
    }

    #[test]
    fn schema_rejects_scalar_and_nested_path_overlap() {
        let fields = [
            ConfigurationField {
                key: "application".into(),
                kind: ConfigurationValueKind::String,
                required: false,
                secret: false,
                description: "Scalar application value".into(),
                default: None,
            },
            ConfigurationField {
                key: "application.name".into(),
                kind: ConfigurationValueKind::String,
                required: false,
                secret: false,
                description: "Nested application name".into(),
                default: None,
            },
        ];
        let error = ConfigurationSchema::try_from_fields(fields).unwrap_err();
        assert_eq!(
            error.diagnostics()[0].code,
            "config.schema.overlapping_field"
        );
    }

    #[test]
    fn plugin_descriptors_extend_the_application_schema() {
        let application = ConfigurationSchema::try_from_fields([ConfigurationField {
            key: "application.name".into(),
            kind: ConfigurationValueKind::String,
            required: true,
            secret: false,
            description: "Application name".into(),
            default: Some(json!("orders")),
        }])
        .unwrap();
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("example").unwrap(),
            Version::new(1, 0, 0),
            "Example",
        );
        descriptor.configuration.push(ConfigurationField {
            key: "timeout_seconds".into(),
            kind: ConfigurationValueKind::Integer,
            required: false,
            secret: false,
            description: "Timeout".into(),
            default: Some(json!(30)),
        });
        let schema = application
            .with_plugin_descriptors([descriptor])
            .expect("plugin schema");
        let paths = schema.fields().map(|(path, _)| path).collect::<Vec<_>>();
        assert_eq!(
            paths,
            ["application.name", "plugins.example.timeout_seconds"]
        );
    }
}
