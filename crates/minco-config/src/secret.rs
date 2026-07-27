use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use std::fmt;
use thiserror::Error;

/// Provider named by an opaque secret reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretProvider {
    EnvironmentVariable,
    SystemsManagerParameter,
}

/// A provider-neutral pointer to a secret value.
///
/// This type contains a reference name, never a resolved secret value. Its
/// `Debug` implementation intentionally omits the name so ordinary diagnostic
/// logging cannot reveal secret inventory.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum SecretReference {
    EnvironmentVariable { name: String },
    SystemsManagerParameter { name: String },
}

#[derive(Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
enum SecretReferenceRepresentation {
    EnvironmentVariable { name: String },
    SystemsManagerParameter { name: String },
}

impl<'de> Deserialize<'de> for SecretReference {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match SecretReferenceRepresentation::deserialize(deserializer)? {
            SecretReferenceRepresentation::EnvironmentVariable { name } => {
                Self::environment_variable(name).map_err(D::Error::custom)
            }
            SecretReferenceRepresentation::SystemsManagerParameter { name } => {
                Self::systems_manager_parameter(name).map_err(D::Error::custom)
            }
        }
    }
}

impl SecretReference {
    /// Construct a reference to an explicitly named environment variable.
    pub fn environment_variable(name: impl Into<String>) -> Result<Self, SecretReferenceError> {
        let name = name.into();
        validate_environment_name(&name)?;
        Ok(Self::EnvironmentVariable { name })
    }

    /// Construct a reference to an explicitly named SSM parameter.
    pub fn systems_manager_parameter(
        name: impl Into<String>,
    ) -> Result<Self, SecretReferenceError> {
        let name = name.into();
        validate_parameter_name(&name)?;
        Ok(Self::SystemsManagerParameter { name })
    }

    /// Parse the only accepted configuration syntaxes: `env:NAME` and
    /// `ssm:/absolute/parameter/name`.
    pub fn parse(value: &str) -> Result<Self, SecretReferenceError> {
        if let Some(name) = value.strip_prefix("env:") {
            return Self::environment_variable(name);
        }
        if let Some(name) = value.strip_prefix("ssm:") {
            return Self::systems_manager_parameter(name);
        }
        Err(SecretReferenceError::UnsupportedSyntax)
    }

    pub const fn provider(&self) -> SecretProvider {
        match self {
            Self::EnvironmentVariable { .. } => SecretProvider::EnvironmentVariable,
            Self::SystemsManagerParameter { .. } => SecretProvider::SystemsManagerParameter,
        }
    }
}

impl fmt::Debug for SecretReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretReference")
            .field("provider", &self.provider())
            .field("name", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SecretReferenceError {
    #[error("secret references must use env:NAME or ssm:/absolute/name")]
    UnsupportedSyntax,
    #[error("environment secret reference names must match [A-Z_][A-Z0-9_]*")]
    InvalidEnvironmentName,
    #[error("SSM secret reference names must satisfy bounded Parameter Store path syntax")]
    InvalidParameterName,
}

fn validate_environment_name(name: &str) -> Result<(), SecretReferenceError> {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return Err(SecretReferenceError::InvalidEnvironmentName);
    };
    if name.len() > 256
        || !(first.is_ascii_uppercase() || first == b'_')
        || bytes.any(|byte| !(byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'))
    {
        return Err(SecretReferenceError::InvalidEnvironmentName);
    }
    Ok(())
}

fn validate_parameter_name(name: &str) -> Result<(), SecretReferenceError> {
    let Some(relative) = name.strip_prefix('/') else {
        return Err(SecretReferenceError::InvalidParameterName);
    };
    if name.len() > 1_011
        || relative.is_empty()
        || !relative
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-' | b'/'))
        || relative.split('/').any(str::is_empty)
        || relative.split('/').count() > 15
        || relative.get(..3).is_some_and(|prefix| {
            prefix.eq_ignore_ascii_case("aws") || prefix.eq_ignore_ascii_case("ssm")
        })
    {
        return Err(SecretReferenceError::InvalidParameterName);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_reference_syntax_is_bounded() {
        assert!(SecretReference::parse("env:DATABASE_URL").is_ok());
        assert!(SecretReference::parse("ssm:/orders/production/database-url").is_ok());
        assert!(SecretReference::parse("ssm:/Orders_v2/Production/database.url").is_ok());
        for invalid in [
            "secret",
            "env:lowercase",
            "env:",
            "ssm:relative",
            "ssm:/bad//name",
            "ssm:/bad name",
            "ssm:/bad:name",
            "ssm:/aws/managed",
            "ssm:/SSM-secret",
            "ssm:/1/2/3/4/5/6/7/8/9/10/11/12/13/14/15/16",
        ] {
            assert!(SecretReference::parse(invalid).is_err(), "{invalid}");
        }
        let oversized = format!("ssm:/{}", "a".repeat(1_011));
        assert!(SecretReference::parse(&oversized).is_err());
    }

    #[test]
    fn debug_redacts_reference_names() {
        let reference = SecretReference::environment_variable("DATABASE_URL").unwrap();
        let rendered = format!("{reference:?}");
        assert!(rendered.contains("[REDACTED]"));
        assert!(!rendered.contains("DATABASE_URL"));
    }

    #[test]
    fn deserialization_cannot_bypass_reference_validation() {
        let invalid = r#"{"provider":"environment_variable","name":"secret-value"}"#;
        assert!(serde_json::from_str::<SecretReference>(invalid).is_err());
    }
}
