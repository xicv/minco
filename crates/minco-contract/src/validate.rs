use crate::{ContractDocument, HttpMethod, OwnedOperation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::Path};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractFinding {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub location: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractReport {
    pub document: ContractDocument,
    pub findings: Vec<ContractFinding>,
}

impl ContractReport {
    pub fn is_valid(&self) -> bool {
        !self
            .findings
            .iter()
            .any(|finding| finding.severity == Severity::Error)
    }
}

pub fn load_contract(path: impl AsRef<Path>) -> Result<ContractReport, ContractError> {
    let path = path.as_ref();
    let source = fs::read_to_string(path)?;
    let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&source)?;
    let raw = serde_json::to_value(yaml)?;
    let canonical = serde_json::to_vec(&canonicalize(&raw))?;
    let sha256 = format!("{:x}", Sha256::digest(canonical));
    let openapi_version = string_at(&raw, &["openapi"]).unwrap_or_default();
    let title = string_at(&raw, &["info", "title"]).unwrap_or_default();
    let version = string_at(&raw, &["info", "version"]).unwrap_or_default();
    let mut findings = Vec::new();
    if !openapi_version.starts_with("3.1.") {
        error(
            &mut findings,
            "MINCO-CONTRACT-001",
            "Minco requires OpenAPI 3.1.x",
            "openapi",
        );
    }
    if title.trim().is_empty() {
        error(
            &mut findings,
            "MINCO-CONTRACT-002",
            "info.title is required",
            "info.title",
        );
    }
    let mut seen = BTreeSet::new();
    let mut operations = Vec::new();
    if let Some(paths) = raw.get("paths").and_then(Value::as_object) {
        for (path_key, path_item) in paths {
            let Some(path_object) = path_item.as_object() else {
                error(
                    &mut findings,
                    "MINCO-CONTRACT-003",
                    "path item must be an object",
                    path_key,
                );
                continue;
            };
            for (method_key, operation) in path_object {
                let Some(method) = HttpMethod::from_openapi(method_key) else {
                    continue;
                };
                let location = format!("paths.{path_key}.{method_key}");
                let Some(operation_object) = operation.as_object() else {
                    error(
                        &mut findings,
                        "MINCO-CONTRACT-004",
                        "operation must be an object",
                        &location,
                    );
                    continue;
                };
                let operation_id = operation_object
                    .get("operationId")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !valid_operation_id(operation_id) {
                    error(
                        &mut findings,
                        "MINCO-CONTRACT-005",
                        "operationId must be lowerCamelCase ASCII",
                        &location,
                    );
                    continue;
                }
                if !seen.insert(operation_id.to_owned()) {
                    error(
                        &mut findings,
                        "MINCO-CONTRACT-006",
                        "operationId must be unique",
                        &location,
                    );
                }
                validate_responses(
                    operation_id,
                    operation_object.get("responses"),
                    &location,
                    &mut findings,
                );
                let idempotent = operation_object
                    .get("x-minco-idempotent")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if idempotent
                    && !has_required_idempotency_header(operation_object.get("parameters"))
                {
                    error(
                        &mut findings,
                        "MINCO-CONTRACT-007",
                        "idempotent operation requires Idempotency-Key",
                        &location,
                    );
                }
                let public = operation_object
                    .get("security")
                    .and_then(Value::as_array)
                    .is_some_and(Vec::is_empty)
                    || operation_object.get("x-minco-auth").and_then(Value::as_str)
                        == Some("public");
                operations.push(OwnedOperation {
                    operation_id: operation_id.to_owned(),
                    method,
                    path: path_key.clone(),
                    authenticated: !public,
                    idempotent,
                });
            }
        }
    } else {
        error(
            &mut findings,
            "MINCO-CONTRACT-008",
            "paths object is required",
            "paths",
        );
    }
    operations.sort_by(|left, right| left.operation_id.cmp(&right.operation_id));
    let mut schema_names = Vec::new();
    if let Some(schemas) = raw
        .pointer("/components/schemas")
        .and_then(Value::as_object)
    {
        for (name, schema) in schemas {
            schema_names.push(name.clone());
            if !valid_schema_name(name) {
                error(
                    &mut findings,
                    "MINCO-CONTRACT-013",
                    "schema names must be PascalCase ASCII identifiers",
                    &format!("components.schemas.{name}"),
                );
            }
            if schema.get("type").and_then(Value::as_str) == Some("object")
                && schema.get("additionalProperties") != Some(&Value::Bool(false))
            {
                error(
                    &mut findings,
                    "MINCO-CONTRACT-009",
                    "top-level object schemas must set additionalProperties: false",
                    &format!("components.schemas.{name}"),
                );
            }
        }
    }
    schema_names.sort();
    Ok(ContractReport {
        document: ContractDocument {
            source: path.display().to_string(),
            openapi_version,
            title,
            version,
            sha256,
            operations,
            schema_names,
            raw,
        },
        findings,
    })
}

fn validate_responses(
    operation_id: &str,
    responses: Option<&Value>,
    location: &str,
    findings: &mut Vec<ContractFinding>,
) {
    let Some(responses) = responses.and_then(Value::as_object) else {
        error(
            findings,
            "MINCO-CONTRACT-010",
            &format!("{operation_id} requires responses"),
            location,
        );
        return;
    };
    if !responses.keys().any(|status| status.starts_with('2')) {
        error(
            findings,
            "MINCO-CONTRACT-011",
            &format!("{operation_id} requires a 2xx response"),
            location,
        );
    }
    if !responses
        .keys()
        .any(|status| status == "default" || status.starts_with('4') || status.starts_with('5'))
    {
        error(
            findings,
            "MINCO-CONTRACT-012",
            &format!("{operation_id} requires an error response"),
            location,
        );
    }
}

fn has_required_idempotency_header(parameters: Option<&Value>) -> bool {
    parameters
        .and_then(Value::as_array)
        .is_some_and(|parameters| {
            parameters.iter().any(|parameter| {
                parameter.get("in").and_then(Value::as_str) == Some("header")
                    && parameter
                        .get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|name| name.eq_ignore_ascii_case("Idempotency-Key"))
                    && parameter.get("required").and_then(Value::as_bool) == Some(true)
            })
        })
}

fn valid_schema_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(first) if first.is_ascii_uppercase())
        && bytes.all(|byte| byte.is_ascii_alphanumeric())
}

fn valid_operation_id(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(first) if first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_alphanumeric())
}

fn string_at(value: &Value, path: &[&str]) -> Option<String> {
    path.iter()
        .try_fold(value, |current, segment| current.get(*segment))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn canonicalize(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut ordered = serde_json::Map::new();
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort();
            for key in keys {
                ordered.insert(key.clone(), canonicalize(&object[key]));
            }
            Value::Object(ordered)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize).collect()),
        other => other.clone(),
    }
}

fn error(findings: &mut Vec<ContractFinding>, code: &str, message: &str, location: &str) {
    findings.push(ContractFinding {
        code: code.to_owned(),
        severity: Severity::Error,
        message: message.to_owned(),
        location: location.to_owned(),
    });
}

#[derive(Debug, Error)]
pub enum ContractError {
    #[error("failed to read contract: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse YAML: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),
    #[error("failed to transform contract: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn validates_a_minimal_contract() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, "openapi: 3.1.0\ninfo: {{title: Test, version: 1.0.0}}\npaths:\n  /health:\n    get:\n      operationId: getHealth\n      security: []\n      responses:\n        '200': {{description: ok}}\n        default: {{description: problem}}\ncomponents:\n  schemas: {{}}\n").unwrap();
        let report = load_contract(file.path()).unwrap();
        assert!(report.is_valid(), "{:?}", report.findings);
        assert_eq!(report.document.operations.len(), 1);
    }
}
