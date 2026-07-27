use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

/// Stable machine-readable configuration diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigDiagnostic {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl ConfigDiagnostic {
    pub(crate) fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            path: None,
            source: None,
        }
    }

    pub(crate) fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub(crate) fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}

/// One or more fail-closed configuration diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigurationError {
    diagnostics: Vec<ConfigDiagnostic>,
}

impl ConfigurationError {
    pub(crate) fn new(diagnostics: Vec<ConfigDiagnostic>) -> Self {
        debug_assert!(!diagnostics.is_empty());
        Self { diagnostics }
    }

    /// Construct an error at an integration boundary that maps another
    /// subsystem into one stable configuration diagnostic.
    pub fn from_diagnostic(diagnostic: ConfigDiagnostic) -> Self {
        Self {
            diagnostics: vec![diagnostic],
        }
    }

    /// Stable diagnostics in deterministic discovery order.
    pub fn diagnostics(&self) -> &[ConfigDiagnostic] {
        &self.diagnostics
    }
}

impl fmt::Display for ConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "configuration rejected with {} diagnostic(s)",
            self.diagnostics.len()
        )
    }
}

impl Error for ConfigurationError {}
