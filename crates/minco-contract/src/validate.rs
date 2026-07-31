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
