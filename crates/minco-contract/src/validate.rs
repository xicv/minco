use crate::{ContractDocument, HttpMethod, OwnedOperation, ResourceAction};
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
    load_contract_source(path.display().to_string(), &source)
}

pub fn load_contract_source(
    source_name: impl Into<String>,
    source: &str,
) -> Result<ContractReport, ContractError> {
    let source_name = source_name.into();
    let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(source)?;
    let raw = serde_json::to_value(yaml)?;
    let canonical = serde_json::to_vec(&canonicalize(&raw))?;
    let sha256 = hex::encode(Sha256::digest(canonical));
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
    validate_reference_integrity(&raw, &mut findings);
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
                validate_resource_metadata(
                    ResourceValidation {
                        document: &raw,
                        metadata: operation_object.get("x-minco-resource"),
                        method,
                        path_parameters: path_object.get("parameters"),
                        operation_parameters: operation_object.get("parameters"),
                        responses: operation_object.get("responses"),
                        idempotent,
                        has_idempotency_header,
                        location: &location,
                    },
                    &mut findings,
                );
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
    validate_resource_families(&raw, &operations, &mut findings);
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
    validate_request_validation_profile(&raw, &mut findings);
    schema_names.sort();
    Ok(ContractReport {
        document: ContractDocument {
            source: source_name,
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

fn validate_resource_families(
    document: &Value,
    operations: &[OwnedOperation],
    findings: &mut Vec<ContractFinding>,
) {
    let mut seen = BTreeSet::new();
    let mut paths: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)> = BTreeMap::new();
    for operation in operations {
        let method = operation.method.as_str().to_ascii_lowercase();
        let Some(metadata) = document
            .pointer(&format!(
                "/paths/{}/{method}/x-minco-resource",
                escape_json_pointer(&operation.path)
            ))
            .and_then(Value::as_object)
        else {
            continue;
        };
        let Some(name) = metadata.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(action) = metadata
            .get("action")
            .and_then(Value::as_str)
            .and_then(ResourceAction::from_openapi)
        else {
            continue;
        };
        if !seen.insert((name.to_owned(), action)) {
            error(
                findings,
                "MINCO-CONTRACT-028",
                &format!("resource {name} declares the {action:?} action more than once"),
                &format!("paths.{}.{}", operation.path, method),
            );
        }
        let (collection, member) = paths.entry(name.to_owned()).or_default();
        if matches!(action, ResourceAction::Create | ResourceAction::List) {
            collection.insert(operation.path.clone());
        } else {
            member.insert(operation.path.clone());
        }
    }
    for (name, (collection, member)) in paths {
        let coherent = collection.len() <= 1
            && member.len() <= 1
            && match (collection.first(), member.first()) {
                (Some(collection), Some(member)) => {
                    member.strip_prefix(collection).is_some_and(|suffix| {
                        suffix.starts_with("/{")
                            && suffix.ends_with('}')
                            && !suffix[2..suffix.len() - 1].contains(['{', '}', '/'])
                    })
                }
                _ => true,
            };
        if !coherent {
            error(
                findings,
                "MINCO-CONTRACT-028",
                &format!(
                    "resource {name} actions must share one collection path and one direct member path"
                ),
                "paths",
            );
        }
    }
}

fn escape_json_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

#[derive(Debug, Clone, Copy)]
struct ResourceValidation<'a> {
    document: &'a Value,
    metadata: Option<&'a Value>,
    method: HttpMethod,
    path_parameters: Option<&'a Value>,
    operation_parameters: Option<&'a Value>,
    responses: Option<&'a Value>,
    idempotent: bool,
    has_idempotency_header: bool,
    location: &'a str,
}

fn validate_resource_metadata(
    context: ResourceValidation<'_>,
    findings: &mut Vec<ContractFinding>,
) {
    let ResourceValidation {
        document,
        metadata,
        method,
        path_parameters,
        operation_parameters,
        responses,
        idempotent,
        has_idempotency_header,
        location,
    } = context;
    let Some(metadata) = metadata else {
        return;
    };
    let valid = metadata.as_object().is_some_and(|metadata| {
        let name = metadata.get("name").and_then(Value::as_str);
        let action = metadata
            .get("action")
            .and_then(Value::as_str)
            .and_then(ResourceAction::from_openapi);
        name.is_some_and(valid_resource_name)
            && action.is_some_and(|action| resource_method_matches(action, method))
            && (matches!(action, Some(ResourceAction::List)) || metadata.len() == 2)
    });
    if !valid {
        error(
            findings,
            "MINCO-CONTRACT-022",
            "x-minco-resource must contain only a lower-kebab-case name and a create, list, read, update, or delete action matching POST, GET, GET, PATCH, or DELETE",
            location,
        );
        return;
    }
    let action = metadata
        .get("action")
        .and_then(Value::as_str)
        .and_then(ResourceAction::from_openapi)
        .expect("validated resource action");
    if action == ResourceAction::List {
        let valid_list = metadata.as_object().is_some_and(|metadata| {
            valid_list_resource_policy(metadata)
                && list_contract_realizes_policy(
                    document,
                    metadata,
                    path_parameters,
                    operation_parameters,
                    responses,
                    location,
                    findings,
                )
        });
        if !valid_list {
            error(
                findings,
                "MINCO-CONTRACT-026",
                "resource list operations require bounded page[limit]/page[after] cursor parameters, allowlisted sort/filter parameters, and a data/page response matching the declared list policy",
                location,
            );
        }
    }
    if matches!(action, ResourceAction::Update | ResourceAction::Delete)
        && !has_required_header(
            document,
            path_parameters,
            operation_parameters,
            "if-match",
            location,
            findings,
        )
    {
        error(
            findings,
            "MINCO-CONTRACT-023",
            "resource update and delete operations require an effective required If-Match header",
            location,
        );
    }
    if action == ResourceAction::Create
        && (!idempotent
            || !has_idempotency_header
            || !response_has_headers(document, responses, "201", &["location"]))
    {
        error(
            findings,
            "MINCO-CONTRACT-027",
            "resource create operations require Idempotency-Key semantics and a 201 Location header",
            location,
        );
    }
    if matches!(action, ResourceAction::Update | ResourceAction::Delete)
        && !responses
            .and_then(Value::as_object)
            .is_some_and(|responses| responses.contains_key("412") && responses.contains_key("428"))
    {
        error(
            findings,
            "MINCO-CONTRACT-024",
            "resource update and delete operations require explicit 412 and 428 Problem responses",
            location,
        );
    }
    let success_status = match action {
        ResourceAction::Create => Some("201"),
        ResourceAction::Read | ResourceAction::Update => Some("200"),
        ResourceAction::List | ResourceAction::Delete => None,
    };
    if success_status.is_some_and(|status| {
        !response_has_required_properties_and_headers(
            document,
            responses,
            status,
            &["data"],
            &["etag"],
        )
    }) || (action == ResourceAction::Create
        && responses
            .and_then(Value::as_object)
            .is_some_and(|responses| responses.contains_key("200"))
        && !response_has_required_properties_and_headers(
            document,
            responses,
            "200",
            &["data"],
            &["etag", "location"],
        ))
    {
        error(
            findings,
            "MINCO-CONTRACT-025",
            "resource create, read, and update success responses require an application/json data envelope and ETag header; declared create replays also require Location",
            location,
        );
    }
    if action == ResourceAction::Delete
        && responses
            .and_then(Value::as_object)
            .and_then(|responses| responses.get("204"))
            .and_then(|response| resolve_local_value(document, response))
            .is_none_or(|response| response.get("content").is_some())
    {
        error(
            findings,
            "MINCO-CONTRACT-025",
            "resource delete success requires a 204 response without content",
            location,
        );
    }
}

fn response_has_headers(
    document: &Value,
    responses: Option<&Value>,
    status: &str,
    required: &[&str],
) -> bool {
    let Some(response) = responses
        .and_then(Value::as_object)
        .and_then(|responses| responses.get(status))
        .and_then(|response| resolve_local_value(document, response))
    else {
        return false;
    };
    let headers = response
        .get("headers")
        .and_then(Value::as_object)
        .map(|headers| {
            headers
                .keys()
                .map(|name| name.to_ascii_lowercase())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    required.iter().all(|name| headers.contains(*name))
}

fn list_contract_realizes_policy(
    document: &Value,
    metadata: &serde_json::Map<String, Value>,
    path_parameters: Option<&Value>,
    operation_parameters: Option<&Value>,
    responses: Option<&Value>,
    location: &str,
    findings: &mut Vec<ContractFinding>,
) -> bool {
    let default_limit = metadata
        .get("defaultLimit")
        .and_then(Value::as_u64)
        .expect("validated default limit");
    let max_limit = metadata
        .get("maxLimit")
        .and_then(Value::as_u64)
        .expect("validated maximum limit");
    let filters =
        unique_field_list(metadata.get("filterFields"), true).expect("validated filter field list");
    let parameters = effective_parameters(
        document,
        path_parameters,
        operation_parameters,
        location,
        findings,
    );
    let limit_valid = parameters
        .get(&("page[limit]".to_owned(), "query".to_owned()))
        .is_some_and(|parameter| {
            parameter.get("required").and_then(Value::as_bool) != Some(true)
                && parameter.get("schema").is_some_and(|schema| {
                    schema.get("type").and_then(Value::as_str) == Some("integer")
                        && schema.get("minimum").and_then(Value::as_u64) == Some(1)
                        && schema.get("maximum").and_then(Value::as_u64) == Some(max_limit)
                        && schema.get("default").and_then(Value::as_u64) == Some(default_limit)
                })
        });
    let after_valid = parameters
        .get(&("page[after]".to_owned(), "query".to_owned()))
        .is_some_and(|parameter| {
            parameter.get("required").and_then(Value::as_bool) != Some(true)
                && parameter.get("schema").is_some_and(|schema| {
                    schema.get("type").and_then(Value::as_str) == Some("string")
                        && schema.get("minLength").and_then(Value::as_u64) == Some(1)
                        && schema.get("maxLength").and_then(Value::as_u64) == Some(512)
                })
        });
    let sort_valid = parameters
        .get(&("sort".to_owned(), "query".to_owned()))
        .is_some_and(|parameter| {
            parameter.get("required").and_then(Value::as_bool) != Some(true)
                && parameter
                    .get("schema")
                    .and_then(|schema| schema.get("type"))
                    .and_then(Value::as_str)
                    == Some("string")
        });
    let declared_filters = parameters
        .keys()
        .filter_map(|(name, location)| {
            (location == "query")
                .then(|| {
                    name.strip_prefix("filter[")
                        .and_then(|name| name.strip_suffix(']'))
                })
                .flatten()
        })
        .collect::<BTreeSet<_>>();
    let filters_valid =
        declared_filters == filters.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let Some(schema) = response_schema(document, responses, "200") else {
        return false;
    };
    let required = required_schema_properties(schema);
    let data_is_array = schema
        .pointer("/properties/data")
        .and_then(|value| resolve_local_value(document, value))
        .is_some_and(|value| {
            value.get("type").and_then(Value::as_str) == Some("array")
                && value.get("items").is_some()
        });
    let page = schema
        .pointer("/properties/page")
        .and_then(|value| resolve_local_value(document, value));
    let page_required = page.map(required_schema_properties).unwrap_or_default();
    let page_properties = page
        .and_then(|page| page.get("properties"))
        .and_then(Value::as_object);
    let page_shape_valid = page.is_some_and(|page| {
        page.get("type").and_then(Value::as_str) == Some("object")
            && page_properties
                .and_then(|properties| properties.get("hasMore"))
                .and_then(|schema| resolve_local_value(document, schema))
                .and_then(|schema| schema.get("type"))
                .and_then(Value::as_str)
                == Some("boolean")
            && page_properties
                .and_then(|properties| properties.get("nextCursor"))
                .and_then(|schema| resolve_local_value(document, schema))
                .and_then(|schema| schema.get("type"))
                .and_then(Value::as_array)
                .is_some_and(|types| {
                    types.iter().any(|value| value.as_str() == Some("string"))
                        && types.iter().any(|value| value.as_str() == Some("null"))
                })
    });
    limit_valid
        && after_valid
        && sort_valid
        && filters_valid
        && required.contains("data")
        && required.contains("page")
        && data_is_array
        && page_shape_valid
        && page_required.contains("hasMore")
        && page_required.contains("nextCursor")
}

fn response_schema<'a>(
    document: &'a Value,
    responses: Option<&'a Value>,
    status: &str,
) -> Option<&'a Value> {
    responses
        .and_then(Value::as_object)
        .and_then(|responses| responses.get(status))
        .and_then(|response| resolve_local_value(document, response))
        .and_then(|response| response.pointer("/content/application~1json/schema"))
        .and_then(|schema| resolve_local_value(document, schema))
}

fn required_schema_properties(schema: &Value) -> BTreeSet<&str> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

fn valid_list_resource_policy(metadata: &serde_json::Map<String, Value>) -> bool {
    const EXPECTED_KEYS: [&str; 8] = [
        "action",
        "cursorFields",
        "defaultLimit",
        "defaultSort",
        "filterFields",
        "maxLimit",
        "name",
        "sortFields",
    ];
    if metadata.len() != EXPECTED_KEYS.len()
        || !EXPECTED_KEYS.iter().all(|key| metadata.contains_key(*key))
    {
        return false;
    }
    let Some(default_limit) = metadata.get("defaultLimit").and_then(Value::as_u64) else {
        return false;
    };
    let Some(max_limit) = metadata.get("maxLimit").and_then(Value::as_u64) else {
        return false;
    };
    if default_limit == 0 || max_limit > 1_000 || default_limit > max_limit {
        return false;
    }
    let Some(sort_fields) = unique_field_list(metadata.get("sortFields"), false) else {
        return false;
    };
    let Some(filter_fields) = unique_field_list(metadata.get("filterFields"), true) else {
        return false;
    };
    let Some(cursor_fields) = unique_field_list(metadata.get("cursorFields"), false) else {
        return false;
    };
    let Some(default_sort) = metadata.get("defaultSort").and_then(Value::as_array) else {
        return false;
    };
    let mut default_fields = BTreeSet::new();
    let valid_default = !default_sort.is_empty()
        && default_sort.iter().all(|item| {
            let Some(item) = item.as_str() else {
                return false;
            };
            let field = item.strip_prefix('-').unwrap_or(item);
            valid_api_field(field)
                && sort_fields.contains(field)
                && default_fields.insert(field.to_owned())
        });
    valid_default
        && filter_fields.iter().all(|field| valid_api_field(field))
        && cursor_fields
            .iter()
            .all(|field| default_fields.contains(field) && sort_fields.contains(field))
}

fn unique_field_list(value: Option<&Value>, empty_allowed: bool) -> Option<BTreeSet<String>> {
    let values = value?.as_array()?;
    if !empty_allowed && values.is_empty() {
        return None;
    }
    let mut fields = BTreeSet::new();
    values
        .iter()
        .all(|value| {
            value
                .as_str()
                .is_some_and(|field| valid_api_field(field) && fields.insert(field.to_owned()))
        })
        .then_some(fields)
}

fn valid_api_field(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(first) if first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_alphanumeric())
}

fn response_has_required_properties_and_headers(
    document: &Value,
    responses: Option<&Value>,
    status: &str,
    properties: &[&str],
    headers: &[&str],
) -> bool {
    let Some(response) = responses
        .and_then(Value::as_object)
        .and_then(|responses| responses.get(status))
        .and_then(|response| resolve_local_value(document, response))
    else {
        return false;
    };
    let Some(schema) = response_schema(document, responses, status) else {
        return false;
    };
    let required = required_schema_properties(schema);
    let declared = schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| {
            properties
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let response_headers = response
        .get("headers")
        .and_then(Value::as_object)
        .map(|headers| {
            headers
                .keys()
                .map(|name| name.to_ascii_lowercase())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    schema.get("type").and_then(Value::as_str) == Some("object")
        && properties
            .iter()
            .all(|property| required.contains(property) && declared.contains(property))
        && headers
            .iter()
            .all(|header| response_headers.contains(*header))
}

fn resolve_local_value<'a>(document: &'a Value, value: &'a Value) -> Option<&'a Value> {
    let Some(reference) = value.get("$ref").and_then(Value::as_str) else {
        return Some(value);
    };
    reference
        .strip_prefix('#')
        .and_then(|pointer| document.pointer(pointer))
}

const fn resource_method_matches(action: ResourceAction, method: HttpMethod) -> bool {
    matches!(
        (action, method),
        (ResourceAction::Create, HttpMethod::Post)
            | (ResourceAction::List | ResourceAction::Read, HttpMethod::Get)
            | (ResourceAction::Update, HttpMethod::Patch)
            | (ResourceAction::Delete, HttpMethod::Delete)
    )
}

fn valid_resource_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
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
        if requirement.len() > 1 {
            error(
                findings,
                "MINCO-CONTRACT-036",
                "generated authorization does not support AND-composed security schemes",
                location,
            );
            valid = false;
        }
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

const REQUEST_VALIDATION_PROFILE: &str = "generated";
const REQUEST_SCHEMA_MAX_DEPTH: usize = 32;
const REQUEST_SCHEMA_MAX_NODES: usize = 4_096;
const REQUEST_SCHEMA_MAX_PROPERTIES: usize = 256;
const REQUEST_SCHEMA_MAX_IDENTIFIER_BYTES: usize = 128;
const REQUEST_SCHEMA_MAX_ENUM_MEMBERS: usize = 128;

fn validate_request_validation_profile(document: &Value, findings: &mut Vec<ContractFinding>) {
    let Some(profile) = document.get("x-minco-request-validation") else {
        return;
    };
    if profile.as_str() != Some(REQUEST_VALIDATION_PROFILE) {
        error(
            findings,
            "MINCO-CONTRACT-029",
            "x-minco-request-validation must be generated when present",
            "x-minco-request-validation",
        );
        return;
    }

    let mut validation = RequestSchemaValidation {
        document,
        findings,
        active_references: BTreeSet::new(),
        visited_nodes: 0,
        complexity_reported: false,
    };
    let Some(paths) = document.get("paths").and_then(Value::as_object) else {
        return;
    };
    let mut path_names = paths.keys().collect::<Vec<_>>();
    path_names.sort();
    for path_name in path_names {
        let Some(path_item) = paths[path_name].as_object() else {
            continue;
        };
        let path_location = format!("$.paths.{path_name}");
        validation.visit_parameters(
            path_item.get("parameters"),
            &format!("{path_location}.parameters"),
        );
        for method in [
            "get", "put", "post", "delete", "options", "head", "patch", "trace",
        ] {
            let Some(operation) = path_item.get(method).and_then(Value::as_object) else {
                continue;
            };
            let operation_location = format!("{path_location}.{method}");
            validation.visit_parameters(
                operation.get("parameters"),
                &format!("{operation_location}.parameters"),
            );
            if let Some(request_body) = operation.get("requestBody") {
                validation
                    .visit_request_body(request_body, &format!("{operation_location}.requestBody"));
            }
        }
    }
}

struct RequestSchemaValidation<'a, 'b> {
    document: &'a Value,
    findings: &'b mut Vec<ContractFinding>,
    active_references: BTreeSet<String>,
    visited_nodes: usize,
    complexity_reported: bool,
}

impl RequestSchemaValidation<'_, '_> {
    fn validate_json_request_root(&mut self, schema: &Value, location: &str) {
        let mut active = BTreeSet::new();
        if !self.json_request_root_is_generated(schema, 0, &mut active) {
            error(
                self.findings,
                "MINCO-CONTRACT-035",
                "generated JSON request bodies must reference a named object or string-enum schema",
                location,
            );
        }
    }

    fn json_request_root_is_generated(
        &self,
        schema: &Value,
        depth: usize,
        active: &mut BTreeSet<String>,
    ) -> bool {
        if depth > REQUEST_SCHEMA_MAX_DEPTH {
            return false;
        }
        let Some(object) = schema.as_object() else {
            return false;
        };
        let Some(reference) = object.get("$ref").and_then(Value::as_str) else {
            return false;
        };
        let Some(name) = exact_schema_component_name(reference) else {
            return false;
        };
        if object.len() != 1 || !active.insert(reference.to_owned()) {
            return false;
        }
        let pointer = format!("/components/schemas/{}", escape_json_pointer(name));
        let supported = self.document.pointer(&pointer).is_some_and(|target| {
            let Some(target_object) = target.as_object() else {
                return false;
            };
            if target_object.contains_key("$ref") {
                return self.json_request_root_is_generated(target, depth + 1, active);
            }
            target_object.get("type").and_then(Value::as_str) == Some("object")
                || (target_object.get("type").and_then(Value::as_str) == Some("string")
                    && target_object.get("enum").is_some())
        });
        active.remove(reference);
        supported
    }

    fn visit_parameters(&mut self, parameters: Option<&Value>, location: &str) {
        let Some(parameters) = parameters.and_then(Value::as_array) else {
            return;
        };
        for (index, parameter) in parameters.iter().enumerate() {
            let parameter_location = format!("{location}[{index}]");
            let Some(parameter) = self
                .resolve_component_reference(
                    parameter,
                    "#/components/parameters/",
                    &parameter_location,
                )
                .cloned()
            else {
                continue;
            };
            let Some(parameter) = parameter.as_object() else {
                self.malformed(
                    "generated request parameters must be objects",
                    &parameter_location,
                );
                continue;
            };
            let valid_name = parameter
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| !name.is_empty());
            let parameter_in = parameter.get("in").and_then(Value::as_str);
            let valid_in = matches!(parameter_in, Some("query" | "path" | "header" | "cookie"));
            if !valid_name || !valid_in {
                self.malformed(
                    "generated request parameters require a nonempty name and valid in value",
                    &parameter_location,
                );
            }
            let has_schema = parameter.contains_key("schema");
            let has_content = parameter.contains_key("content");
            if has_schema == has_content {
                self.malformed(
                    "generated request parameters require exactly one of schema or content",
                    &parameter_location,
                );
            }
            if !matches!(parameter_in, Some("path" | "query")) {
                continue;
            }
            if parameter.contains_key("content") {
                error(
                    self.findings,
                    "MINCO-CONTRACT-035",
                    "generated request parameters do not support content-based schemas",
                    &format!("{parameter_location}.content"),
                );
            }
            if let Some(schema) = parameter.get("schema") {
                self.visit_schema(schema, &format!("{parameter_location}.schema"), 0, false);
            }
        }
    }

    fn visit_request_body(&mut self, request_body: &Value, location: &str) {
        let Some(request_body) = self
            .resolve_component_reference(request_body, "#/components/requestBodies/", location)
            .cloned()
        else {
            return;
        };
        let Some(request_body) = request_body.as_object() else {
            self.malformed("generated request bodies must be objects", location);
            return;
        };
        let Some(content) = request_body.get("content").and_then(Value::as_object) else {
            self.malformed(
                "generated request bodies require an object-form content map",
                &format!("{location}.content"),
            );
            return;
        };
        let mut media_types = content.keys().collect::<Vec<_>>();
        media_types.sort();
        for media_type in media_types {
            if media_type != "application/json" && !media_type.ends_with("+json") {
                continue;
            }
            let Some(schema) = content[media_type]
                .as_object()
                .and_then(|media| media.get("schema"))
            else {
                self.malformed(
                    "generated JSON request media types require a schema",
                    &format!("{location}.content.{media_type}"),
                );
                continue;
            };
            let schema_location = format!("{location}.content.{media_type}.schema");
            self.validate_json_request_root(schema, &schema_location);
            self.visit_schema(schema, &schema_location, 0, false);
        }
    }

    fn visit_schema(
        &mut self,
        schema: &Value,
        location: &str,
        depth: usize,
        named_component_root: bool,
    ) {
        if depth > REQUEST_SCHEMA_MAX_DEPTH {
            error(
                self.findings,
                "MINCO-CONTRACT-032",
                "generated request schema reference depth exceeds the supported limit",
                location,
            );
            return;
        }
        self.visited_nodes += 1;
        if self.visited_nodes > REQUEST_SCHEMA_MAX_NODES {
            if !self.complexity_reported {
                error(
                    self.findings,
                    "MINCO-CONTRACT-033",
                    "generated request schema complexity exceeds the supported limit",
                    location,
                );
                self.complexity_reported = true;
            }
            return;
        }
        let Some(object) = schema.as_object() else {
            error(
                self.findings,
                "MINCO-CONTRACT-035",
                "generated request schemas must use object-form schemas",
                location,
            );
            return;
        };

        if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
            if object.len() != 1 {
                error(
                    self.findings,
                    "MINCO-CONTRACT-035",
                    "generated request schema $ref siblings are not supported",
                    location,
                );
            }
            self.visit_schema_reference(reference, location, depth);
            return;
        } else if object.contains_key("$ref") {
            error(
                self.findings,
                "MINCO-CONTRACT-031",
                "generated request schema $ref must be a local string reference",
                &format!("{location}.$ref"),
            );
        }

        self.validate_keywords(object, location);
        self.validate_schema_type(object.get("type"), location);
        self.validate_assertion_applicability(object, location);
        self.validate_unsigned_bounds(object, "minLength", "maxLength", location);
        self.validate_unsigned_bounds(object, "minItems", "maxItems", location);
        self.validate_unsigned_bounds(object, "minProperties", "maxProperties", location);
        self.validate_numeric_bounds(object, location);
        self.validate_scalar_values(object, location);

        if let Some(properties) = object.get("properties") {
            let Some(properties) = properties.as_object() else {
                self.malformed("properties must be an object", location);
                return;
            };
            if properties.len() > REQUEST_SCHEMA_MAX_PROPERTIES {
                error(
                    self.findings,
                    "MINCO-CONTRACT-033",
                    "generated request object property count exceeds the supported limit",
                    &format!("{location}.properties"),
                );
                return;
            }
            let mut names = properties.keys().collect::<Vec<_>>();
            names.sort();
            let required = object
                .get("required")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<BTreeSet<_>>();
            let mut generated_names = BTreeSet::new();
            for name in names {
                if name.len() > REQUEST_SCHEMA_MAX_IDENTIFIER_BYTES {
                    error(
                        self.findings,
                        "MINCO-CONTRACT-033",
                        "generated request property name exceeds the supported byte limit",
                        &format!("{location}.properties.{name}"),
                    );
                    continue;
                }
                if name.is_empty() || name == "$" || name.contains('.') {
                    error(
                        self.findings,
                        "MINCO-CONTRACT-033",
                        "generated request property names must be unambiguous dot-path segments",
                        &format!("{location}.properties.{name}"),
                    );
                }
                if !generated_names.insert(request_rust_identifier(name)) {
                    error(
                        self.findings,
                        "MINCO-CONTRACT-033",
                        "generated request property names must map to unique Rust identifiers",
                        &format!("{location}.properties.{name}"),
                    );
                }
                let property = &properties[name];
                if !required.contains(name.as_str()) && schema_is_nullable_value(property) {
                    let presence_is_observable = object.get("minProperties").is_some()
                        || object.get("maxProperties").is_some();
                    let null_is_distinct = property
                        .get("enum")
                        .and_then(Value::as_array)
                        .is_some_and(|values| !values.iter().any(Value::is_null))
                        || property.get("const").is_some_and(|value| !value.is_null());
                    if presence_is_observable || null_is_distinct {
                        error(
                            self.findings,
                            "MINCO-CONTRACT-035",
                            "generated optional nullable properties cannot be combined with presence-sensitive assertions",
                            &format!("{location}.properties.{name}"),
                        );
                    }
                }
                self.visit_schema(
                    property,
                    &format!("{location}.properties.{name}"),
                    depth + 1,
                    false,
                );
            }
        }
        self.validate_required(object, location);
        if schema_includes_object(object.get("type"))
            && object.get("additionalProperties") != Some(&Value::Bool(false))
        {
            error(
                self.findings,
                "MINCO-CONTRACT-035",
                "generated request object schemas must set additionalProperties to false",
                location,
            );
        }
        if schema_includes_object(object.get("type")) && !named_component_root {
            error(
                self.findings,
                "MINCO-CONTRACT-035",
                "generated request object schemas must use a named components.schemas reference",
                location,
            );
        }
        if let Some(items) = object.get("items") {
            self.visit_schema(items, &format!("{location}.items"), depth + 1, false);
        }
    }

    fn visit_schema_reference(&mut self, reference: &str, location: &str, depth: usize) {
        let Some(name) = exact_schema_component_name(reference) else {
            error(
                self.findings,
                "MINCO-CONTRACT-031",
                "generated request schema references must resolve under components.schemas",
                &format!("{location}.$ref"),
            );
            return;
        };
        if name.len() > REQUEST_SCHEMA_MAX_IDENTIFIER_BYTES {
            error(
                self.findings,
                "MINCO-CONTRACT-031",
                "generated request schema reference has an invalid component name",
                &format!("{location}.$ref"),
            );
            return;
        }
        if !self.active_references.insert(reference.to_owned()) {
            error(
                self.findings,
                "MINCO-CONTRACT-032",
                "recursive generated request schemas are not supported",
                &format!("$.components.schemas.{name}"),
            );
            return;
        }
        let pointer = format!("/components/schemas/{}", escape_json_pointer(name));
        if let Some(target) = self.document.pointer(&pointer) {
            self.visit_schema(
                target,
                &format!("$.components.schemas.{name}"),
                depth + 1,
                true,
            );
        } else {
            error(
                self.findings,
                "MINCO-CONTRACT-031",
                "generated request schema reference does not resolve",
                &format!("{location}.$ref"),
            );
        }
        self.active_references.remove(reference);
    }

    fn resolve_component_reference<'a>(
        &'a mut self,
        value: &'a Value,
        prefix: &str,
        location: &str,
    ) -> Option<&'a Value> {
        let Some(reference) = value.get("$ref") else {
            return Some(value);
        };
        let Some(reference) = reference.as_str() else {
            error(
                self.findings,
                "MINCO-CONTRACT-031",
                "generated request component $ref must be a string",
                &format!("{location}.$ref"),
            );
            return None;
        };
        let Some(name) = reference.strip_prefix(prefix) else {
            error(
                self.findings,
                "MINCO-CONTRACT-031",
                "generated request component references must be local and use the expected component kind",
                &format!("{location}.$ref"),
            );
            return None;
        };
        if name.is_empty() || name.contains('/') || name.contains('~') {
            error(
                self.findings,
                "MINCO-CONTRACT-031",
                "generated request component references must target one exact component entry",
                &format!("{location}.$ref"),
            );
            return None;
        }
        if value.as_object().is_some_and(|object| object.len() != 1) {
            error(
                self.findings,
                "MINCO-CONTRACT-035",
                "generated request component $ref siblings are not supported",
                location,
            );
            return None;
        }
        match self.document.pointer(&reference[1..]) {
            Some(target) if target.is_object() && target.get("$ref").is_none() => Some(target),
            Some(_) => {
                error(
                    self.findings,
                    "MINCO-CONTRACT-031",
                    "generated request component references must resolve directly to an object",
                    &format!("{location}.$ref"),
                );
                None
            }
            None => {
                error(
                    self.findings,
                    "MINCO-CONTRACT-031",
                    "generated request component reference does not resolve",
                    &format!("{location}.$ref"),
                );
                None
            }
        }
    }

    fn validate_keywords(&mut self, object: &serde_json::Map<String, Value>, location: &str) {
        const SUPPORTED: &[&str] = &[
            "$ref",
            "$comment",
            "type",
            "title",
            "description",
            "default",
            "deprecated",
            "readOnly",
            "writeOnly",
            "examples",
            "example",
            "externalDocs",
            "xml",
            "discriminator",
            "format",
            "properties",
            "required",
            "additionalProperties",
            "items",
            "minLength",
            "maxLength",
            "minItems",
            "maxItems",
            "minProperties",
            "maxProperties",
            "minimum",
            "maximum",
            "exclusiveMinimum",
            "exclusiveMaximum",
            "enum",
            "const",
            "x-minco-open-object",
        ];
        for keyword in object.keys() {
            if SUPPORTED.contains(&keyword.as_str()) || keyword.starts_with("x-") {
                continue;
            }
            error(
                self.findings,
                "MINCO-CONTRACT-030",
                &format!(
                    "request-reachable assertion {keyword} is not supported by generated validation"
                ),
                &format!("{location}.{keyword}"),
            );
        }
        if object.get("readOnly") == Some(&Value::Bool(true)) {
            error(
                self.findings,
                "MINCO-CONTRACT-035",
                "readOnly request properties are not supported by generated request DTOs",
                &format!("{location}.readOnly"),
            );
        }
    }

    fn validate_schema_type(&mut self, value: Option<&Value>, location: &str) {
        let valid_primitive =
            |value: &str| matches!(value, "boolean" | "object" | "array" | "integer" | "string");
        let valid = match value {
            Some(Value::String(value)) => valid_primitive(value),
            Some(Value::Array(values)) => {
                let strings = values.iter().filter_map(Value::as_str).collect::<Vec<_>>();
                strings.len() == values.len()
                    && strings.len() == 2
                    && strings
                        .iter()
                        .all(|value| *value == "null" || valid_primitive(value))
                    && strings.iter().filter(|value| **value == "null").count() == 1
                    && strings
                        .iter()
                        .any(|value| matches!(*value, "boolean" | "integer" | "string"))
            }
            _ => false,
        };
        if !valid {
            error(
                self.findings,
                "MINCO-CONTRACT-035",
                "generated request type must be a representable JSON type, optionally plus null",
                location,
            );
        }
    }

    fn validate_assertion_applicability(
        &mut self,
        object: &serde_json::Map<String, Value>,
        location: &str,
    ) {
        let schema_type = request_schema_type(object);
        if matches!(
            object.get("format").and_then(Value::as_str),
            Some("uuid" | "date-time")
        ) && ["minLength", "maxLength", "enum", "const"]
            .iter()
            .any(|keyword| object.contains_key(*keyword))
        {
            error(
                self.findings,
                "MINCO-CONTRACT-035",
                "generated typed string formats cannot be combined with lexical assertions",
                location,
            );
        }
        if object.contains_key("enum")
            && ["minLength", "maxLength", "const"]
                .iter()
                .any(|keyword| object.contains_key(*keyword))
        {
            error(
                self.findings,
                "MINCO-CONTRACT-035",
                "generated request enum schemas cannot combine additional value assertions",
                location,
            );
        }
        for (keywords, expected) in [
            (&["minLength", "maxLength"][..], "string"),
            (&["minItems", "maxItems", "items"][..], "array"),
            (
                &[
                    "minProperties",
                    "maxProperties",
                    "properties",
                    "required",
                    "additionalProperties",
                ][..],
                "object",
            ),
            (
                &["minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum"][..],
                "integer",
            ),
        ] {
            for keyword in keywords {
                if object.contains_key(*keyword) && schema_type != Some(expected) {
                    error(
                        self.findings,
                        "MINCO-CONTRACT-035",
                        &format!("generated request assertion {keyword} requires type {expected}"),
                        &format!("{location}.{keyword}"),
                    );
                }
            }
        }
    }

    fn validate_unsigned_bounds(
        &mut self,
        object: &serde_json::Map<String, Value>,
        minimum: &str,
        maximum: &str,
        location: &str,
    ) {
        let minimum_value = object.get(minimum).map(Value::as_u64);
        let maximum_value = object.get(maximum).map(Value::as_u64);
        let shaped = matches!(minimum_value, None | Some(Some(_)))
            && matches!(maximum_value, None | Some(Some(_)));
        let ordered = match (minimum_value.flatten(), maximum_value.flatten()) {
            (Some(minimum), Some(maximum)) => minimum <= maximum,
            _ => true,
        };
        if !shaped || !ordered {
            self.malformed(
                &format!("{minimum} and {maximum} must be ordered non-negative integers"),
                location,
            );
        }
    }

    fn validate_numeric_bounds(&mut self, object: &serde_json::Map<String, Value>, location: &str) {
        for keyword in ["minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum"] {
            if object
                .get(keyword)
                .is_some_and(|value| json_integer_i128(value).is_none())
            {
                error(
                    self.findings,
                    "MINCO-CONTRACT-035",
                    &format!("generated integer assertion {keyword} must use a whole 64-bit value"),
                    &format!("{location}.{keyword}"),
                );
            }
        }
        for (minimum, maximum) in [
            ("minimum", "maximum"),
            ("exclusiveMinimum", "exclusiveMaximum"),
        ] {
            if let (Some(minimum), Some(maximum)) = (
                object.get(minimum).and_then(json_integer_i128),
                object.get(maximum).and_then(json_integer_i128),
            ) && minimum > maximum
            {
                self.malformed(&format!("{minimum} must not exceed {maximum}"), location);
            }
        }
    }

    fn validate_scalar_values(&mut self, object: &serde_json::Map<String, Value>, location: &str) {
        let schema_type = request_schema_type(object);
        let nullable = schema_is_nullable_type(object.get("type"));
        if let Some(values) = object.get("enum") {
            let Some(values) = values.as_array() else {
                self.malformed("enum must be an array", location);
                return;
            };
            if values.is_empty()
                || values.len() > REQUEST_SCHEMA_MAX_ENUM_MEMBERS
                || !values.iter().all(is_scalar_json)
            {
                self.malformed(
                    "enum must contain between 1 and 128 unique scalar values",
                    location,
                );
            }
            let unique = values.iter().map(Value::to_string).collect::<BTreeSet<_>>();
            if unique.len() != values.len() {
                self.malformed("enum values must be unique", location);
            }
            if values
                .iter()
                .any(|value| !scalar_matches_generated_type(value, schema_type, nullable, object))
            {
                error(
                    self.findings,
                    "MINCO-CONTRACT-035",
                    "generated request enum values must match the declared representable type",
                    &format!("{location}.enum"),
                );
            }
            if schema_includes_string(object.get("type")) {
                let strings = values.iter().filter_map(Value::as_str).collect::<Vec<_>>();
                let identifiers = strings
                    .iter()
                    .map(|value| request_rust_enum_identifier(value))
                    .collect::<BTreeSet<_>>();
                if strings
                    .iter()
                    .any(|value| value.len() > REQUEST_SCHEMA_MAX_IDENTIFIER_BYTES)
                    || identifiers.len() != strings.len()
                {
                    error(
                        self.findings,
                        "MINCO-CONTRACT-033",
                        "generated request enum values must map to unique bounded Rust identifiers",
                        location,
                    );
                }
            }
        }
        if object
            .get("const")
            .is_some_and(|value| !is_scalar_json(value))
        {
            self.malformed("const must be a scalar value", location);
        } else if object.get("const").is_some_and(|value| {
            !scalar_matches_generated_type(value, schema_type, nullable, object)
        }) {
            error(
                self.findings,
                "MINCO-CONTRACT-035",
                "generated request const must match the declared representable type",
                &format!("{location}.const"),
            );
        }
    }

    fn validate_required(&mut self, object: &serde_json::Map<String, Value>, location: &str) {
        let Some(required) = object.get("required") else {
            return;
        };
        let Some(required) = required.as_array() else {
            self.malformed(
                "required must be an array of unique property names",
                location,
            );
            return;
        };
        let names = required
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        let unique = names.iter().copied().collect::<BTreeSet<_>>();
        if names.len() != required.len() || unique.len() != names.len() {
            self.malformed(
                "required must be an array of unique property names",
                location,
            );
        }
        if let Some(properties) = object.get("properties").and_then(Value::as_object)
            && names.iter().any(|name| !properties.contains_key(*name))
        {
            self.malformed("required names must exist in properties", location);
        }
    }

    fn malformed(&mut self, message: &str, location: &str) {
        error(self.findings, "MINCO-CONTRACT-034", message, location);
    }
}

const fn is_scalar_json(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
    )
}

fn exact_schema_component_name(reference: &str) -> Option<&str> {
    let name = reference.strip_prefix("#/components/schemas/")?;
    (!name.is_empty() && !name.contains('/') && !name.contains('~')).then_some(name)
}

fn request_schema_type(object: &serde_json::Map<String, Value>) -> Option<&str> {
    match object.get("type") {
        Some(Value::String(value)) => Some(value),
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .find(|value| *value != "null"),
        _ => None,
    }
}

fn schema_is_nullable_type(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_array)
        .is_some_and(|types| types.iter().any(|value| value.as_str() == Some("null")))
}

fn json_integer_i128(value: &Value) -> Option<i128> {
    value
        .as_i64()
        .map(i128::from)
        .or_else(|| value.as_u64().map(i128::from))
}

fn scalar_matches_generated_type(
    value: &Value,
    schema_type: Option<&str>,
    nullable: bool,
    schema: &serde_json::Map<String, Value>,
) -> bool {
    if value.is_null() {
        return nullable;
    }
    match schema_type {
        Some("string") => value.is_string(),
        Some("boolean") => value.is_boolean(),
        Some("integer") => {
            let Some(value) = json_integer_i128(value) else {
                return false;
            };
            if schema.get("format").and_then(Value::as_str) == Some("int32") {
                (i128::from(i32::MIN)..=i128::from(i32::MAX)).contains(&value)
            } else {
                (i128::from(i64::MIN)..=i128::from(i64::MAX)).contains(&value)
            }
        }
        _ => false,
    }
}

fn schema_is_nullable_value(schema: &Value) -> bool {
    schema
        .get("type")
        .and_then(Value::as_array)
        .is_some_and(|types| types.iter().any(|value| value.as_str() == Some("null")))
}

fn request_rust_identifier(value: &str) -> String {
    let mut output = String::new();
    let mut previous_was_separator = false;
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 && !previous_was_separator {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
            previous_was_separator = false;
        } else if character.is_ascii_alphanumeric() || character == '_' {
            output.push(character.to_ascii_lowercase());
            previous_was_separator = false;
        } else if !output.is_empty() && !previous_was_separator {
            output.push('_');
            previous_was_separator = true;
        }
    }
    while output.ends_with('_') {
        output.pop();
    }
    if output.is_empty() {
        output.push_str("field");
    }
    if output.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        output.insert(0, '_');
    }
    output
}

fn request_rust_enum_identifier(value: &str) -> String {
    let mut output = value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            match characters.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect::<String>();
    if output.is_empty() {
        output.push_str("Value");
    } else if output.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        output.insert_str(0, "Value");
    }
    if is_rust_reserved_identifier(&output) {
        output.insert_str(0, "Value");
    }
    output
}

fn is_rust_reserved_identifier(value: &str) -> bool {
    matches!(
        value,
        "Self"
            | "as"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
            | "try"
    )
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

fn schema_includes_string(schema_type: Option<&Value>) -> bool {
    match schema_type {
        Some(Value::String(value)) => value == "string",
        Some(Value::Array(values)) => values.iter().any(|value| value.as_str() == Some("string")),
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
    has_required_header(
        document,
        path_parameters,
        operation_parameters,
        "idempotency-key",
        location,
        findings,
    )
}

fn has_required_header(
    document: &Value,
    path_parameters: Option<&Value>,
    operation_parameters: Option<&Value>,
    header_name: &str,
    location: &str,
    findings: &mut Vec<ContractFinding>,
) -> bool {
    effective_parameters(
        document,
        path_parameters,
        operation_parameters,
        location,
        findings,
    )
    .get(&(header_name.to_owned(), "header".to_owned()))
    .is_some_and(|parameter| parameter.get("required").and_then(Value::as_bool) == Some(true))
}

fn effective_parameters<'a>(
    document: &'a Value,
    path_parameters: Option<&'a Value>,
    operation_parameters: Option<&'a Value>,
    location: &str,
    findings: &mut Vec<ContractFinding>,
) -> BTreeMap<(String, String), &'a serde_json::Map<String, Value>> {
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

// A contract document is a standalone artifact: every local `#/...`
// reference must resolve inside the same document. External references are
// out of scope for local validation.
fn validate_reference_integrity(document: &Value, findings: &mut Vec<ContractFinding>) {
    let mut references = Vec::new();
    collect_local_refs(document, "", &mut references);
    for (location, pointer) in references {
        if resolve_json_pointer(document, &pointer).is_none() {
            error(
                findings,
                "MINCO-CONTRACT-037",
                "unresolved local $ref target",
                &format!("{location}: #/{pointer}"),
            );
        }
    }
}

fn collect_local_refs(value: &Value, location: &str, references: &mut Vec<(String, String)>) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let child_location = if location.is_empty() {
                    key.clone()
                } else {
                    format!("{location}.{key}")
                };
                if key == "$ref"
                    && let Value::String(target) = child
                    && let Some(pointer) = target.strip_prefix("#/")
                {
                    references.push((child_location, pointer.to_owned()));
                } else {
                    collect_local_refs(child, &child_location, references);
                }
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_local_refs(item, &format!("{location}[{index}]"), references);
            }
        }
        _ => {}
    }
}

fn resolve_json_pointer<'a>(document: &'a Value, pointer: &str) -> Option<&'a Value> {
    let mut current = document;
    if pointer.is_empty() {
        return Some(current);
    }
    for raw_token in pointer.split('/') {
        let token = raw_token.replace("~1", "/").replace("~0", "~");
        current = match current {
            Value::Object(object) => object.get(&token)?,
            Value::Array(items) => items.get(token.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
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

    const REF_CONTRACT_SOURCE: &str = r"openapi: 3.1.0
info: {title: Test, version: 1.0.0}
paths:
  /health:
    get:
      operationId: getHealth
      security: []
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema: {$ref: '#/components/schemas/Health'}
        default:
          description: problem
          content:
            application/problem+json:
              schema: {type: string}
components:
  schemas: {}
";

    #[test]
    fn flags_dangling_local_refs_as_errors() {
        let report = load_contract_source("ref-contract.yaml", REF_CONTRACT_SOURCE).unwrap();
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.code == "MINCO-CONTRACT-037")
            .expect("a dangling local $ref must produce an error finding");
        assert_eq!(finding.severity, Severity::Error);
        assert!(
            finding
                .location
                .ends_with("$ref: #/components/schemas/Health"),
            "finding location should point at the reference: {}",
            finding.location
        );
        assert!(!report.is_valid());
    }

    #[test]
    fn accepts_contracts_whose_local_refs_resolve() {
        let source = REF_CONTRACT_SOURCE.replace(
            "components:\n  schemas: {}\n",
            "components:\n  schemas:\n    Health: {type: object, additionalProperties: false}\n",
        );
        let report = load_contract_source("ref-contract.yaml", &source).unwrap();
        assert!(
            !report
                .findings
                .iter()
                .any(|finding| finding.code == "MINCO-CONTRACT-037"),
            "{:?}",
            report.findings
        );
        assert!(report.is_valid(), "{:?}", report.findings);
    }

    #[test]
    fn json_pointer_resolution_handles_escapes_and_arrays() {
        let document = serde_json::json!({"components": {"schemas": {"a~b": [1, {"x": 2}]}}});
        assert_eq!(
            resolve_json_pointer(&document, "components/schemas/a~0b/1/x"),
            Some(&serde_json::json!(2))
        );
        assert_eq!(
            resolve_json_pointer(&document, "components/schemas/missing"),
            None
        );
    }
}
