use crate::EnvironmentClass;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fmt};
use thiserror::Error;

/// Fixed precedence classes. The compiler orders layers by this enum rather
/// than trusting call-site order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSourceKind {
    CompiledDefault,
    DefaultFile,
    EnvironmentFile,
    LocalOverride,
    EnvironmentVariables,
    CliOverride,
}

impl ConfigSourceKind {
    pub(crate) const fn precedence(self) -> u8 {
        match self {
            Self::CompiledDefault => 0,
            Self::DefaultFile => 1,
            Self::EnvironmentFile => 2,
            Self::LocalOverride => 3,
            Self::EnvironmentVariables => 4,
            Self::CliOverride => 5,
        }
    }
}

/// One explicit configuration layer.
#[derive(Clone, PartialEq, Eq)]
pub struct ConfigLayer {
    pub(crate) source_kind: ConfigSourceKind,
    pub(crate) source: String,
    pub(crate) environment_class: Option<EnvironmentClass>,
    pub(crate) values: BTreeMap<String, Value>,
}

impl fmt::Debug for ConfigLayer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfigLayer")
            .field("source_kind", &self.source_kind)
            .field("source", &self.source)
            .field("environment_class", &self.environment_class)
            .field("field_count", &self.values.len())
            .finish()
    }
}

impl ConfigLayer {
    pub fn from_toml(
        source_kind: ConfigSourceKind,
        source: impl Into<String>,
        document: &str,
    ) -> Result<Self, ConfigLayerError> {
        let document: ConfigDocument =
            toml::from_str(document).map_err(|_| ConfigLayerError::InvalidToml)?;
        if document.schema != 1 {
            return Err(ConfigLayerError::UnsupportedSchema(document.schema));
        }
        if document.values.values().any(contains_datetime) {
            return Err(ConfigLayerError::UnsupportedDatetime);
        }
        let value =
            serde_json::to_value(document.values).map_err(|_| ConfigLayerError::InvalidValue)?;
        let values = value
            .as_object()
            .expect("TOML table serializes to a JSON object")
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        Ok(Self {
            source_kind,
            source: source.into(),
            environment_class: document.environment_class,
            values,
        })
    }

    pub fn from_pairs<K>(
        source_kind: ConfigSourceKind,
        source: impl Into<String>,
        pairs: impl IntoIterator<Item = (K, Value)>,
    ) -> Result<Self, ConfigLayerError>
    where
        K: Into<String>,
    {
        let mut values = BTreeMap::new();
        for (key, value) in pairs {
            let key = key.into();
            if values.insert(key.clone(), value).is_some() {
                return Err(ConfigLayerError::DuplicatePath(key));
            }
        }
        Ok(Self {
            source_kind,
            source: source.into(),
            environment_class: None,
            values,
        })
    }

    pub const fn environment_class(&self) -> Option<EnvironmentClass> {
        self.environment_class
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigDocument {
    schema: u32,
    #[serde(default)]
    environment_class: Option<EnvironmentClass>,
    #[serde(default)]
    values: toml::Table,
}

#[derive(Error)]
pub enum ConfigLayerError {
    #[error("invalid configuration TOML")]
    InvalidToml,
    #[error("unsupported configuration schema {0}; expected 1")]
    UnsupportedSchema(u32),
    #[error("TOML datetime values are unsupported; use an explicitly typed string")]
    UnsupportedDatetime,
    #[error("configuration layer repeats path {0}")]
    DuplicatePath(String),
    #[error("configuration value cannot be represented")]
    InvalidValue,
}

impl fmt::Debug for ConfigLayerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

fn contains_datetime(value: &toml::Value) -> bool {
    match value {
        toml::Value::Datetime(_) => true,
        toml::Value::Array(values) => values.iter().any(contains_datetime),
        toml::Value::Table(values) => values.values().any(contains_datetime),
        toml::Value::String(_)
        | toml::Value::Integer(_)
        | toml::Value::Float(_)
        | toml::Value::Boolean(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_header_rejects_unknown_fields() {
        let error = ConfigLayer::from_toml(
            ConfigSourceKind::DefaultFile,
            "default",
            "schema = 1\nunexpected = true",
        )
        .unwrap_err();
        assert!(matches!(error, ConfigLayerError::InvalidToml));
    }

    #[test]
    fn toml_datetimes_are_not_coerced_into_strings() {
        let error = ConfigLayer::from_toml(
            ConfigSourceKind::DefaultFile,
            "default",
            "schema = 1\n[values.application]\nstarted_at = 1979-05-27T07:32:00Z",
        )
        .unwrap_err();
        assert!(matches!(error, ConfigLayerError::UnsupportedDatetime));
    }

    #[test]
    fn parser_errors_and_debug_output_do_not_echo_values() {
        let malformed = "schema = 1\n[values.database]\nurl = \"super-secret\n";
        let error = ConfigLayer::from_toml(ConfigSourceKind::DefaultFile, "default", malformed)
            .unwrap_err();
        assert!(!error.to_string().contains("super-secret"));

        let layer = ConfigLayer::from_pairs(
            ConfigSourceKind::DefaultFile,
            "default",
            [("database.url", Value::String("super-secret".into()))],
        )
        .unwrap();
        assert!(!format!("{layer:?}").contains("super-secret"));
    }

    #[test]
    fn programmatic_layers_reject_duplicate_paths() {
        let error = ConfigLayer::from_pairs(
            ConfigSourceKind::CliOverride,
            "cli",
            [
                ("application.name", Value::String("first".into())),
                ("application.name", Value::String("second".into())),
            ],
        )
        .unwrap_err();
        assert!(matches!(error, ConfigLayerError::DuplicatePath(_)));
    }
}
