use crate::{ContractDocument, HttpMethod, OwnedOperation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
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
                    &raw,
                    operation_id,
                    operation_object.get("responses"),
                    &location,
                    &mut findings,
                );
                let idempotency_flag = operation_object.get("x-minco-idempotent");
                if idempotency_flag.is_some_and(|value| !value.is_boolean()) {
                    error(
                        &mut findings,
                        "MINCO-CONTRACT-014",
                        "x-minco-idempotent must be a boolean",
                        &location,
                    );
                }
                let idempotent = idempotency_flag.and_then(Value::as_bool).unwrap_or(false);
                let has_idempotency_header = has_required_idempotency_header(
                    &raw,
                    path_object.get("parameters"),
                    operation_object.get("parameters"),
                    &location,
                    &mut findings,
                );
                if idempotent && !has_idempotency_header {
                    error(
                        &mut findings,
                        "MINCO-CONTRACT-007",
                        "idempotent operation requires Idempotency-Key",
                        &location,
                    );
                }
                if is_mutating(method) && has_idempotency_header && !idempotent {
                    error(
                        &mut findings,
                        "MINCO-CONTRACT-015",
                        "required Idempotency-Key requires x-minco-idempotent: true",
                        &location,
                    );
                }
                let security = operation_object
                    .get("security")
                    .or_else(|| raw.get("security"));
                let public = security_allows_anonymous(security, &location, &mut findings);
                validate_auth_policy(operation_object, public, &location, &mut findings);
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
        for name in schemas.keys() {
            schema_names.push(name.clone());
            if !valid_schema_name(name) {
                error(
                    &mut findings,
                    "MINCO-CONTRACT-013",
                    "schema names must be PascalCase ASCII identifiers",
                    &format!("components.schemas.{name}"),
                );
            }
        }
    }
    validate_schema_positions(&raw, &mut findings);
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
    document: &Value,
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
    for (status, response) in responses {
        if (status == "default" || status.starts_with('4') || status.starts_with('5'))
            && !response_has_problem_details(document, response, findings, location)
        {
            error(
                findings,
                "MINCO-CONTRACT-017",
                &format!(
                    "{operation_id} error response {status} must use application/problem+json"
                ),
                &format!("{location}.responses.{status}"),
            );
        }
    }
}

fn response_has_problem_details(
    document: &Value,
    response: &Value,
    findings: &mut Vec<ContractFinding>,
    location: &str,
) -> bool {
    let Some(response) = resolve_local_reference(document, response, findings, location) else {
        return false;
    };
    response
        .get("content")
        .and_then(Value::as_object)
        .is_some_and(|content| content.contains_key("application/problem+json"))
}

fn resolve_local_reference<'a>(
    document: &'a Value,
    value: &'a Value,
    findings: &mut Vec<ContractFinding>,
    location: &str,
) -> Option<&'a Value> {
    let Some(reference) = value.get("$ref").and_then(Value::as_str) else {
        return Some(value);
    };
    if !reference.starts_with("#/") {
        error(
            findings,
            "MINCO-CONTRACT-018",
            "error response references must be local so policy can be validated",
            location,
        );
        return None;
    }
    document.pointer(&reference[1..]).or_else(|| {
        error(
            findings,
            "MINCO-CONTRACT-018",
            "error response reference does not resolve",
            location,
        );
        None
    })
}

fn validate_auth_policy(
    operation: &serde_json::Map<String, Value>,
    public: bool,
    location: &str,
    findings: &mut Vec<ContractFinding>,
) {
    let Some(auth) = operation.get("x-minco-auth") else {
        return;
    };
    match auth.as_str() {
        Some("public") if !public => error(
            findings,
            "MINCO-CONTRACT-016",
            "x-minco-auth public requires effective OpenAPI security to allow anonymous access",
            location,
        ),
        Some("authenticated") if public => error(
            findings,
            "MINCO-CONTRACT-016",
            "x-minco-auth authenticated contradicts effective anonymous OpenAPI security",
            location,
        ),
        Some("public" | "authenticated") => {}
        None if valid_permission_policy(auth, public) => {}
        _ => error(
            findings,
            "MINCO-CONTRACT-016",
            "x-minco-auth must be public, authenticated, or a non-empty permission_scoped policy consistent with OpenAPI security",
            location,
        ),
    }
}

fn valid_permission_policy(auth: &Value, public: bool) -> bool {
    let Some(policy) = auth.as_object() else {
        return false;
    };
    if public || policy.get("mode").and_then(Value::as_str) != Some("permission_scoped") {
        return false;
    }
    let Some(permissions) = policy.get("permissions").and_then(Value::as_array) else {
        return false;
    };
    let mut unique = BTreeSet::new();
    !permissions.is_empty()
        && permissions.iter().all(|permission| {
            permission
                .as_str()
                .is_some_and(|permission| valid_permission(permission) && unique.insert(permission))
        })
}

fn valid_permission(permission: &str) -> bool {
    !permission.is_empty()
        && permission.len() <= 128
        && permission.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b':' | b'_' | b'-')
        })
}

fn security_allows_anonymous(
    security: Option<&Value>,
    location: &str,
    findings: &mut Vec<ContractFinding>,
) -> bool {
    let Some(security) = security else {
        return true;
    };
    let Some(requirements) = security.as_array() else {
        error(
            findings,
            "MINCO-CONTRACT-020",
            "effective OpenAPI security must be an array",
            location,
        );
        return false;
    };
    let mut valid = true;
    let mut allows_anonymous = requirements.is_empty();
    for requirement in requirements {
        let Some(requirement) = requirement.as_object() else {
            valid = false;
            continue;
        };
        allows_anonymous |= requirement.is_empty();
        for scopes in requirement.values() {
            let Some(scopes) = scopes.as_array() else {
                valid = false;
                continue;
            };
            valid &= scopes.iter().all(Value::is_string);
        }
    }
    if !valid {
        error(
            findings,
            "MINCO-CONTRACT-020",
            "each OpenAPI Security Requirement must be an object whose scheme values are arrays of strings",
            location,
        );
    }
    valid && allows_anonymous
}

fn validate_schema_positions(document: &Value, findings: &mut Vec<ContractFinding>) {
    let components = document.get("components").and_then(Value::as_object);
    if let Some(schemas) = components
        .and_then(|components| components.get("schemas"))
        .and_then(Value::as_object)
    {
        for (name, schema) in schemas {
            validate_schema(schema, &format!("$.components.schemas.{name}"), findings);
        }
    }
    if let Some(components) = components {
        visit_component_entries(
            components.get("parameters"),
            "$.components.parameters",
            validate_parameter_schema,
            findings,
        );
        visit_component_entries(
            components.get("headers"),
            "$.components.headers",
            validate_header_schema,
            findings,
        );
        visit_component_entries(
            components.get("requestBodies"),
            "$.components.requestBodies",
            validate_request_body_schemas,
            findings,
        );
        visit_component_entries(
            components.get("responses"),
            "$.components.responses",
            validate_response_schemas,
            findings,
        );
        if let Some(callbacks) = components.get("callbacks").and_then(Value::as_object) {
            for (name, callback) in callbacks {
                visit_callback(
                    callback,
                    &format!("$.components.callbacks.{name}"),
                    findings,
                );
            }
        }
        visit_component_entries(
            components.get("pathItems"),
            "$.components.pathItems",
            visit_path_item,
            findings,
        );
    }
    for root in ["paths", "webhooks"] {
        if let Some(items) = document.get(root).and_then(Value::as_object) {
            for (name, path_item) in items {
                visit_path_item(path_item, &format!("$.{root}.{name}"), findings);
            }
        }
    }
}

fn visit_component_entries(
    entries: Option<&Value>,
    location: &str,
    visitor: fn(&Value, &str, &mut Vec<ContractFinding>),
    findings: &mut Vec<ContractFinding>,
) {
    if let Some(entries) = entries.and_then(Value::as_object) {
        for (name, value) in entries {
            visitor(value, &format!("{location}.{name}"), findings);
        }
    }
}

fn visit_path_item(value: &Value, location: &str, findings: &mut Vec<ContractFinding>) {
    let Some(path_item) = value.as_object() else {
        return;
    };
    visit_parameters(
        path_item.get("parameters"),
        &format!("{location}.parameters"),
        findings,
    );
    for method in [
        "get", "put", "post", "delete", "options", "head", "patch", "trace",
    ] {
        let Some(operation) = path_item.get(method).and_then(Value::as_object) else {
            continue;
        };
        let operation_location = format!("{location}.{method}");
        visit_parameters(
            operation.get("parameters"),
            &format!("{operation_location}.parameters"),
            findings,
        );
        if let Some(request_body) = operation.get("requestBody") {
            validate_request_body_schemas(
                request_body,
                &format!("{operation_location}.requestBody"),
                findings,
            );
        }
        if let Some(responses) = operation.get("responses").and_then(Value::as_object) {
            for (status, response) in responses {
                validate_response_schemas(
                    response,
                    &format!("{operation_location}.responses.{status}"),
                    findings,
                );
            }
        }
        if let Some(callbacks) = operation.get("callbacks").and_then(Value::as_object) {
            for (name, callback) in callbacks {
                visit_callback(
                    callback,
                    &format!("{operation_location}.callbacks.{name}"),
                    findings,
                );
            }
        }
    }
}

fn visit_callback(value: &Value, location: &str, findings: &mut Vec<ContractFinding>) {
    let Some(callback) = value.as_object() else {
        return;
    };
    if callback.contains_key("$ref") {
        return;
    }
    for (expression, path_item) in callback {
        visit_path_item(path_item, &format!("{location}.{expression}"), findings);
    }
}

fn visit_parameters(value: Option<&Value>, location: &str, findings: &mut Vec<ContractFinding>) {
    if let Some(parameters) = value.and_then(Value::as_array) {
        for (index, parameter) in parameters.iter().enumerate() {
            validate_parameter_schema(parameter, &format!("{location}[{index}]"), findings);
        }
    }
}

fn validate_parameter_schema(value: &Value, location: &str, findings: &mut Vec<ContractFinding>) {
    if let Some(schema) = value.get("schema") {
        validate_schema(schema, &format!("{location}.schema"), findings);
    }
    visit_content_schemas(
        value.get("content"),
        &format!("{location}.content"),
        findings,
    );
}

fn validate_header_schema(value: &Value, location: &str, findings: &mut Vec<ContractFinding>) {
    validate_parameter_schema(value, location, findings);
}

fn validate_request_body_schemas(
    value: &Value,
    location: &str,
    findings: &mut Vec<ContractFinding>,
) {
    visit_content_schemas(
        value.get("content"),
        &format!("{location}.content"),
        findings,
    );
}

fn validate_response_schemas(value: &Value, location: &str, findings: &mut Vec<ContractFinding>) {
    visit_content_schemas(
        value.get("content"),
        &format!("{location}.content"),
        findings,
    );
    if let Some(headers) = value.get("headers").and_then(Value::as_object) {
        for (name, header) in headers {
            validate_header_schema(header, &format!("{location}.headers.{name}"), findings);
        }
    }
}

fn visit_content_schemas(
    value: Option<&Value>,
    location: &str,
    findings: &mut Vec<ContractFinding>,
) {
    if let Some(content) = value.and_then(Value::as_object) {
        for (media_type, media) in content {
            if let Some(schema) = media.get("schema") {
                validate_schema(schema, &format!("{location}.{media_type}.schema"), findings);
            }
            if let Some(encodings) = media.get("encoding").and_then(Value::as_object) {
                for (property, encoding) in encodings {
                    if let Some(headers) = encoding.get("headers").and_then(Value::as_object) {
                        for (name, header) in headers {
                            validate_header_schema(
                                header,
                                &format!(
                                    "{location}.{media_type}.encoding.{property}.headers.{name}"
                                ),
                                findings,
                            );
                        }
                    }
                }
            }
        }
    }
}

fn validate_schema(value: &Value, location: &str, findings: &mut Vec<ContractFinding>) {
    let Some(object) = value.as_object() else {
        return;
    };
    if schema_includes_object(object.get("type")) {
        validate_object_policy(object, location, findings);
    }
    for keyword in [
        "properties",
        "patternProperties",
        "dependentSchemas",
        "$defs",
    ] {
        if let Some(children) = object.get(keyword).and_then(Value::as_object) {
            for (name, child) in children {
                validate_schema(child, &format!("{location}.{keyword}.{name}"), findings);
            }
        }
    }
    for keyword in ["allOf", "anyOf", "oneOf", "prefixItems"] {
        if let Some(children) = object.get(keyword).and_then(Value::as_array) {
            for (index, child) in children.iter().enumerate() {
                validate_schema(child, &format!("{location}.{keyword}[{index}]"), findings);
            }
        }
    }
    for keyword in [
        "items",
        "contains",
        "not",
        "if",
        "then",
        "else",
        "propertyNames",
        "additionalProperties",
        "unevaluatedProperties",
    ] {
        if let Some(child) = object.get(keyword).filter(|child| child.is_object()) {
            validate_schema(child, &format!("{location}.{keyword}"), findings);
        }
    }
}

fn schema_includes_object(schema_type: Option<&Value>) -> bool {
    match schema_type {
        Some(Value::String(value)) => value == "object",
        Some(Value::Array(values)) => values.iter().any(|value| value.as_str() == Some("object")),
        _ => false,
    }
}

fn validate_object_policy(
    object: &serde_json::Map<String, Value>,
    location: &str,
    findings: &mut Vec<ContractFinding>,
) {
    let additional = object.get("additionalProperties");
    if additional == Some(&Value::Bool(false)) {
        if object.contains_key("x-minco-open-object") {
            error(
                findings,
                "MINCO-CONTRACT-019",
                "closed objects must not declare x-minco-open-object",
                location,
            );
        }
        return;
    }

    let explicit_open_policy = matches!(additional, Some(Value::Bool(true) | Value::Object(_)));
    let rationale = object
        .get("x-minco-open-object")
        .and_then(Value::as_object)
        .and_then(|policy| policy.get("rationale"))
        .and_then(Value::as_str)
        .is_some_and(|rationale| !rationale.trim().is_empty());
    if !explicit_open_policy || !rationale {
        error(
            findings,
            "MINCO-CONTRACT-009",
            "object schemas must be closed or declare explicit additionalProperties and x-minco-open-object.rationale",
            location,
        );
    }
}

fn has_required_idempotency_header(
    document: &Value,
    path_parameters: Option<&Value>,
    operation_parameters: Option<&Value>,
    location: &str,
    findings: &mut Vec<ContractFinding>,
) -> bool {
    let mut effective = BTreeMap::new();
    for parameters in [path_parameters, operation_parameters] {
        let Some(parameters) = parameters.and_then(Value::as_array) else {
            continue;
        };
        for parameter in parameters {
            let Some(parameter) =
                resolve_parameter_reference(document, parameter, location, findings)
            else {
                continue;
            };
            let Some(parameter) = parameter.as_object() else {
                continue;
            };
            let Some(name) = parameter.get("name").and_then(Value::as_str) else {
                continue;
            };
            let Some(parameter_in) = parameter.get("in").and_then(Value::as_str) else {
                continue;
            };
            effective.insert(
                (name.to_ascii_lowercase(), parameter_in.to_ascii_lowercase()),
                parameter,
            );
        }
    }
    effective
        .get(&("idempotency-key".to_owned(), "header".to_owned()))
        .is_some_and(|parameter| parameter.get("required").and_then(Value::as_bool) == Some(true))
}

fn resolve_parameter_reference<'a>(
    document: &'a Value,
    parameter: &'a Value,
    location: &str,
    findings: &mut Vec<ContractFinding>,
) -> Option<&'a Value> {
    let Some(reference) = parameter.get("$ref").and_then(Value::as_str) else {
        return Some(parameter);
    };
    if !reference.starts_with("#/") {
        error(
            findings,
            "MINCO-CONTRACT-021",
            "parameter references must be local so policy can be validated",
            location,
        );
        return None;
    }
    document.pointer(&reference[1..]).or_else(|| {
        error(
            findings,
            "MINCO-CONTRACT-021",
            "parameter reference does not resolve",
            location,
        );
        None
    })
}

fn valid_schema_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(first) if first.is_ascii_uppercase())
        && bytes.all(|byte| byte.is_ascii_alphanumeric())
}

const fn is_mutating(method: HttpMethod) -> bool {
    matches!(
        method,
        HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch | HttpMethod::Delete
    )
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
        writeln!(file, "openapi: 3.1.0\ninfo: {{title: Test, version: 1.0.0}}\npaths:\n  /health:\n    get:\n      operationId: getHealth\n      security: []\n      responses:\n        '200': {{description: ok}}\n        default:\n          description: problem\n          content:\n            application/problem+json:\n              schema: {{type: string}}\ncomponents:\n  schemas: {{}}\n").unwrap();
        let report = load_contract(file.path()).unwrap();
        assert!(report.is_valid(), "{:?}", report.findings);
        assert_eq!(report.document.operations.len(), 1);
    }
}
