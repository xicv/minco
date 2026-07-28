use crate::{ContractDocument, HttpMethod, OwnedOperation};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

const LIMITATIONS: [&str; 2] = [
    "This structural report does not prove semantic business compatibility.",
    "Deployment, persisted data, migrations, and runtime behavior require separate evidence.",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityClassification {
    Breaking,
    NonBreaking,
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractOperationChange {
    pub code: String,
    pub classification: CompatibilityClassification,
    pub operation_id: String,
    pub method: HttpMethod,
    pub path: String,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractSchemaChange {
    pub code: String,
    pub classification: CompatibilityClassification,
    pub schema: String,
    pub location: String,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractCompatibilityReport {
    pub schema_version: u32,
    pub baseline_source: String,
    pub baseline_sha256: String,
    pub candidate_source: String,
    pub candidate_sha256: String,
    pub classification: CompatibilityClassification,
    pub operation_changes: Vec<ContractOperationChange>,
    pub schema_changes: Vec<ContractSchemaChange>,
    pub limitations: Vec<String>,
}

pub fn diff_contracts(
    baseline: &ContractDocument,
    candidate: &ContractDocument,
) -> ContractCompatibilityReport {
    let baseline_operations = operations_by_id(&baseline.operations);
    let candidate_operations = operations_by_id(&candidate.operations);
    let mut operation_changes = Vec::new();

    for (operation_id, candidate_operation) in &candidate_operations {
        let Some(baseline_operation) = baseline_operations.get(operation_id) else {
            continue;
        };
        if baseline_operation.method != candidate_operation.method
            || baseline_operation.path != candidate_operation.path
        {
            operation_changes.push(ContractOperationChange {
                code: "operation.binding_changed".into(),
                classification: CompatibilityClassification::Breaking,
                operation_id: (*operation_id).into(),
                method: candidate_operation.method,
                path: candidate_operation.path.clone(),
                evidence: format!(
                    "{operation_id} changed binding from {} {} to {} {}",
                    baseline_operation.method.as_str(),
                    baseline_operation.path,
                    candidate_operation.method.as_str(),
                    candidate_operation.path
                ),
            });
        }
        if !baseline_operation.authenticated && candidate_operation.authenticated {
            operation_changes.push(ContractOperationChange {
                code: "operation.authentication_required".into(),
                classification: CompatibilityClassification::Breaking,
                operation_id: (*operation_id).into(),
                method: candidate_operation.method,
                path: candidate_operation.path.clone(),
                evidence: format!("{operation_id} now requires authentication"),
            });
        } else if baseline_operation.authenticated && !candidate_operation.authenticated {
            operation_changes.push(ContractOperationChange {
                code: "operation.authentication_removed".into(),
                classification: CompatibilityClassification::Uncertain,
                operation_id: (*operation_id).into(),
                method: candidate_operation.method,
                path: candidate_operation.path.clone(),
                evidence: format!(
                    "{operation_id} no longer requires authentication; authorization intent needs review"
                ),
            });
        }
        if !baseline_operation.idempotent && candidate_operation.idempotent {
            operation_changes.push(ContractOperationChange {
                code: "operation.idempotency_required".into(),
                classification: CompatibilityClassification::Breaking,
                operation_id: (*operation_id).into(),
                method: candidate_operation.method,
                path: candidate_operation.path.clone(),
                evidence: format!("{operation_id} now requires the idempotency contract"),
            });
        } else if baseline_operation.idempotent && !candidate_operation.idempotent {
            operation_changes.push(ContractOperationChange {
                code: "operation.idempotency_removed".into(),
                classification: CompatibilityClassification::Breaking,
                operation_id: (*operation_id).into(),
                method: candidate_operation.method,
                path: candidate_operation.path.clone(),
                evidence: format!("{operation_id} no longer guarantees idempotent retries"),
            });
        }
        if operation_structure_changed(baseline, baseline_operation, candidate, candidate_operation)
        {
            operation_changes.push(ContractOperationChange {
                code: "operation.structure_changed".into(),
                classification: CompatibilityClassification::Uncertain,
                operation_id: (*operation_id).into(),
                method: candidate_operation.method,
                path: candidate_operation.path.clone(),
                evidence: format!(
                    "{operation_id} request/response/parameter structure changed outside the bounded classifier"
                ),
            });
        }
    }
    for (operation_id, operation) in &candidate_operations {
        if !baseline_operations.contains_key(operation_id) {
            operation_changes.push(operation_change(
                "operation.added",
                CompatibilityClassification::NonBreaking,
                operation,
                "candidate",
            ));
        }
    }
    for (operation_id, operation) in &baseline_operations {
        if !candidate_operations.contains_key(operation_id) {
            operation_changes.push(operation_change(
                "operation.removed",
                CompatibilityClassification::Breaking,
                operation,
                "baseline",
            ));
        }
    }
    operation_changes.sort_by(|left, right| {
        (&left.code, &left.operation_id, left.method, &left.path).cmp(&(
            &right.code,
            &right.operation_id,
            right.method,
            &right.path,
        ))
    });
    let baseline_schemas = component_schemas(baseline);
    let candidate_schemas = component_schemas(candidate);
    let mut schema_changes = Vec::new();
    for name in candidate_schemas.keys() {
        if !baseline_schemas.contains_key(name) {
            schema_changes.push(ContractSchemaChange {
                code: "schema.added".into(),
                classification: CompatibilityClassification::NonBreaking,
                schema: (*name).into(),
                location: format!("#/components/schemas/{name}"),
                evidence: format!("{name} exists only in the candidate contract"),
            });
        }
    }
    for name in baseline_schemas.keys() {
        if !candidate_schemas.contains_key(name) {
            schema_changes.push(ContractSchemaChange {
                code: "schema.removed".into(),
                classification: CompatibilityClassification::Breaking,
                schema: (*name).into(),
                location: format!("#/components/schemas/{name}"),
                evidence: format!("{name} exists only in the baseline contract"),
            });
        }
    }
    for (name, baseline_schema) in &baseline_schemas {
        let Some(candidate_schema) = candidate_schemas.get(name) else {
            continue;
        };
        let location = format!("#/components/schemas/{name}");
        match (
            resolve_local_schema(&baseline.raw, baseline_schema, &mut BTreeSet::new()),
            resolve_local_schema(&candidate.raw, candidate_schema, &mut BTreeSet::new()),
        ) {
            (Ok(baseline_schema), Ok(candidate_schema)) => diff_schema_properties(
                name,
                &location,
                &baseline_schema,
                &candidate_schema,
                &mut schema_changes,
            ),
            (baseline_result, candidate_result) => {
                let detail = baseline_result
                    .err()
                    .or_else(|| candidate_result.err())
                    .unwrap_or_else(|| "unknown reference resolution failure".into());
                schema_changes.push(ContractSchemaChange {
                    code: "schema.reference_unresolved".into(),
                    classification: CompatibilityClassification::Uncertain,
                    schema: (*name).into(),
                    location,
                    evidence: detail,
                });
            }
        }
    }
    schema_changes.sort_by(|left, right| {
        (&left.code, &left.schema, &left.location).cmp(&(
            &right.code,
            &right.schema,
            &right.location,
        ))
    });
    let classification = aggregate_classification(
        operation_changes
            .iter()
            .map(|change| change.classification)
            .chain(schema_changes.iter().map(|change| change.classification)),
    );

    ContractCompatibilityReport {
        schema_version: 1,
        baseline_source: baseline.source.clone(),
        baseline_sha256: baseline.sha256.clone(),
        candidate_source: candidate.source.clone(),
        candidate_sha256: candidate.sha256.clone(),
        classification,
        operation_changes,
        schema_changes,
        limitations: LIMITATIONS.into_iter().map(String::from).collect(),
    }
}

fn operations_by_id(operations: &[OwnedOperation]) -> BTreeMap<&str, &OwnedOperation> {
    operations
        .iter()
        .map(|operation| (operation.operation_id.as_str(), operation))
        .collect()
}

fn operation_structure_changed(
    baseline: &ContractDocument,
    baseline_operation: &OwnedOperation,
    candidate: &ContractDocument,
    candidate_operation: &OwnedOperation,
) -> bool {
    let Some(baseline_value) = operation_value(baseline, baseline_operation) else {
        return operation_value(candidate, candidate_operation).is_some();
    };
    let Some(candidate_value) = operation_value(candidate, candidate_operation) else {
        return true;
    };
    let Ok(baseline_value) =
        resolve_local_schema(&baseline.raw, baseline_value, &mut BTreeSet::new())
    else {
        return true;
    };
    let Ok(candidate_value) =
        resolve_local_schema(&candidate.raw, candidate_value, &mut BTreeSet::new())
    else {
        return true;
    };
    strip_documentation(baseline_value) != strip_documentation(candidate_value)
}

fn operation_value<'a>(
    document: &'a ContractDocument,
    operation: &OwnedOperation,
) -> Option<&'a Value> {
    document
        .raw
        .get("paths")?
        .get(&operation.path)?
        .get(operation.method.as_str().to_ascii_lowercase())
}

fn strip_documentation(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(strip_documentation).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .filter(|(key, _)| {
                    !matches!(
                        key.as_str(),
                        "description"
                            | "example"
                            | "examples"
                            | "externalDocs"
                            | "operationId"
                            | "summary"
                            | "tags"
                            | "title"
                    )
                })
                .map(|(key, value)| (key, strip_documentation(value)))
                .collect(),
        ),
        scalar => scalar,
    }
}

fn component_schemas(document: &ContractDocument) -> BTreeMap<&str, &Value> {
    document
        .raw
        .pointer("/components/schemas")
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flat_map(|schemas| schemas.iter())
        .map(|(name, schema)| (name.as_str(), schema))
        .collect()
}

fn resolve_local_schema(
    document: &Value,
    schema: &Value,
    active_references: &mut BTreeSet<String>,
) -> Result<Value, String> {
    if let Some(object) = schema.as_object()
        && let Some(reference) = object.get("$ref").and_then(Value::as_str)
    {
        if !reference.starts_with("#/") {
            return Err(format!(
                "external schema reference {reference} cannot be compared"
            ));
        }
        let target = document
            .pointer(&reference[1..])
            .ok_or_else(|| format!("local schema reference {reference} does not resolve"))?;
        if !active_references.insert(reference.into()) {
            return Ok(serde_json::json!({"$recursiveRef": reference}));
        }
        let resolved = resolve_local_schema(document, target, active_references)?;
        active_references.remove(reference);
        let siblings = object
            .iter()
            .filter(|(key, _)| key.as_str() != "$ref")
            .map(|(key, value)| {
                Ok((
                    key.clone(),
                    resolve_local_schema(document, value, active_references)?,
                ))
            })
            .collect::<Result<Map<String, Value>, String>>()?;
        if siblings.is_empty() {
            return Ok(resolved);
        }
        return Ok(serde_json::json!({"allOf": [resolved, Value::Object(siblings)]}));
    }
    match schema {
        Value::Array(values) => values
            .iter()
            .map(|value| resolve_local_schema(document, value, active_references))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(values) => values
            .iter()
            .map(|(key, value)| {
                Ok((
                    key.clone(),
                    resolve_local_schema(document, value, active_references)?,
                ))
            })
            .collect::<Result<Map<String, Value>, String>>()
            .map(Value::Object),
        scalar => Ok(scalar.clone()),
    }
}

fn diff_schema_properties(
    schema_name: &str,
    location: &str,
    baseline: &Value,
    candidate: &Value,
    changes: &mut Vec<ContractSchemaChange>,
) {
    if unclassified_schema_keywords(baseline) != unclassified_schema_keywords(candidate) {
        changes.push(ContractSchemaChange {
            code: "schema.structure_changed".into(),
            classification: CompatibilityClassification::Uncertain,
            schema: schema_name.into(),
            location: location.into(),
            evidence: format!(
                "{schema_name} schema structure changed outside the bounded classifier at {location}"
            ),
        });
    }
    for keyword in [
        "additionalProperties",
        "allOf",
        "anyOf",
        "const",
        "dependentRequired",
        "exclusiveMaximum",
        "exclusiveMinimum",
        "format",
        "maxItems",
        "maxLength",
        "maxProperties",
        "maximum",
        "minItems",
        "minLength",
        "minProperties",
        "minimum",
        "not",
        "oneOf",
        "pattern",
        "uniqueItems",
    ] {
        if baseline.get(keyword) != candidate.get(keyword) {
            changes.push(ContractSchemaChange {
                code: "schema.constraint_changed".into(),
                classification: CompatibilityClassification::Uncertain,
                schema: schema_name.into(),
                location: format!("{location}/{keyword}"),
                evidence: format!(
                    "{schema_name} {keyword} changed; request/response compatibility needs review"
                ),
            });
        }
    }
    let baseline_type = baseline.get("type");
    let candidate_type = candidate.get("type");
    match (baseline_type, candidate_type) {
        (Some(baseline_type), Some(candidate_type)) if baseline_type != candidate_type => {
            changes.push(ContractSchemaChange {
                code: "schema.type_changed".into(),
                classification: CompatibilityClassification::Breaking,
                schema: schema_name.into(),
                location: format!("{location}/type"),
                evidence: format!(
                    "{schema_name} type changed from {} to {}",
                    display_schema_value(baseline_type),
                    display_schema_value(candidate_type)
                ),
            });
        }
        (None, Some(candidate_type)) => {
            changes.push(ContractSchemaChange {
                code: "schema.type_constraint_added".into(),
                classification: CompatibilityClassification::Breaking,
                schema: schema_name.into(),
                location: format!("{location}/type"),
                evidence: format!(
                    "{schema_name} added type constraint {}",
                    display_schema_value(candidate_type)
                ),
            });
        }
        (Some(baseline_type), None) => {
            changes.push(ContractSchemaChange {
                code: "schema.type_constraint_removed".into(),
                classification: CompatibilityClassification::Uncertain,
                schema: schema_name.into(),
                location: format!("{location}/type"),
                evidence: format!(
                    "{schema_name} removed type constraint {}; producer/consumer direction needs review",
                    display_schema_value(baseline_type)
                ),
            });
        }
        _ => {}
    }
    match (baseline.get("enum"), candidate.get("enum")) {
        (Some(baseline_enum), Some(candidate_enum)) => {
            let baseline_enum = schema_value_map(Some(baseline_enum));
            let candidate_enum = schema_value_map(Some(candidate_enum));
            for (encoded, value) in baseline_enum
                .iter()
                .filter(|(encoded, _)| !candidate_enum.contains_key(*encoded))
            {
                changes.push(ContractSchemaChange {
                    code: "schema.enum_value_removed".into(),
                    classification: CompatibilityClassification::Breaking,
                    schema: schema_name.into(),
                    location: format!("{location}/enum/{}", schema_location_segment(value)),
                    evidence: format!("{schema_name} enum value {encoded} was removed"),
                });
            }
            for (encoded, value) in candidate_enum
                .iter()
                .filter(|(encoded, _)| !baseline_enum.contains_key(*encoded))
            {
                changes.push(ContractSchemaChange {
                    code: "schema.enum_value_added".into(),
                    classification: CompatibilityClassification::Uncertain,
                    schema: schema_name.into(),
                    location: format!("{location}/enum/{}", schema_location_segment(value)),
                    evidence: format!(
                        "{schema_name} enum value {encoded} was added; producer/consumer direction needs review"
                    ),
                });
            }
        }
        (None, Some(_)) => changes.push(ContractSchemaChange {
            code: "schema.enum_constraint_added".into(),
            classification: CompatibilityClassification::Breaking,
            schema: schema_name.into(),
            location: format!("{location}/enum"),
            evidence: format!("{schema_name} added an enum constraint"),
        }),
        (Some(_), None) => changes.push(ContractSchemaChange {
            code: "schema.enum_constraint_removed".into(),
            classification: CompatibilityClassification::Uncertain,
            schema: schema_name.into(),
            location: format!("{location}/enum"),
            evidence: format!(
                "{schema_name} removed an enum constraint; producer/consumer direction needs review"
            ),
        }),
        (None, None) => {}
    }
    let baseline_property_values = baseline.get("properties").and_then(Value::as_object);
    let candidate_property_values = candidate.get("properties").and_then(Value::as_object);
    let baseline_properties = baseline_property_values
        .into_iter()
        .flat_map(|properties| properties.keys())
        .collect::<BTreeSet<_>>();
    let candidate_properties = candidate_property_values
        .into_iter()
        .flat_map(|properties| properties.keys())
        .collect::<BTreeSet<_>>();
    let baseline_required = schema_string_set(baseline.get("required"));
    let candidate_required = schema_string_set(candidate.get("required"));
    for property in candidate_required.difference(&baseline_required) {
        changes.push(ContractSchemaChange {
            code: "schema.required_property_added".into(),
            classification: CompatibilityClassification::Breaking,
            schema: schema_name.into(),
            location: format!("{location}/required/{property}"),
            evidence: format!("{schema_name} property {property} became required"),
        });
    }
    for property in baseline_required.difference(&candidate_required) {
        changes.push(ContractSchemaChange {
            code: "schema.required_property_removed".into(),
            classification: CompatibilityClassification::Uncertain,
            schema: schema_name.into(),
            location: format!("{location}/required/{property}"),
            evidence: format!(
                "{schema_name} property {property} is no longer required; request/response usage needs review"
            ),
        });
    }
    for property in candidate_properties.difference(&baseline_properties) {
        if candidate_required.contains(property.as_str()) {
            continue;
        }
        changes.push(ContractSchemaChange {
            code: "schema.optional_property_added".into(),
            classification: CompatibilityClassification::NonBreaking,
            schema: schema_name.into(),
            location: format!("{location}/properties/{property}"),
            evidence: format!("{schema_name} optional property {property} was added"),
        });
    }
    for property in baseline_properties.difference(&candidate_properties) {
        changes.push(ContractSchemaChange {
            code: "schema.property_removed".into(),
            classification: CompatibilityClassification::Breaking,
            schema: schema_name.into(),
            location: format!("{location}/properties/{property}"),
            evidence: format!("{schema_name} property {property} was removed"),
        });
    }
    if let (Some(baseline_values), Some(candidate_values)) =
        (baseline_property_values, candidate_property_values)
    {
        for property in baseline_properties.intersection(&candidate_properties) {
            if let (Some(baseline_property), Some(candidate_property)) = (
                baseline_values.get(property.as_str()),
                candidate_values.get(property.as_str()),
            ) {
                diff_schema_properties(
                    schema_name,
                    &format!("{location}/properties/{property}"),
                    baseline_property,
                    candidate_property,
                    changes,
                );
            }
        }
    }
    if let (Some(baseline_items), Some(candidate_items)) =
        (baseline.get("items"), candidate.get("items"))
    {
        diff_schema_properties(
            schema_name,
            &format!("{location}/items"),
            baseline_items,
            candidate_items,
            changes,
        );
    }
}

fn unclassified_schema_keywords(schema: &Value) -> Map<String, Value> {
    schema
        .as_object()
        .into_iter()
        .flat_map(|object| object.iter())
        .filter(|(key, _)| {
            !matches!(
                key.as_str(),
                "additionalProperties"
                    | "allOf"
                    | "anyOf"
                    | "const"
                    | "dependentRequired"
                    | "description"
                    | "enum"
                    | "example"
                    | "examples"
                    | "exclusiveMaximum"
                    | "exclusiveMinimum"
                    | "externalDocs"
                    | "format"
                    | "items"
                    | "maxItems"
                    | "maxLength"
                    | "maxProperties"
                    | "maximum"
                    | "minItems"
                    | "minLength"
                    | "minProperties"
                    | "minimum"
                    | "not"
                    | "oneOf"
                    | "pattern"
                    | "properties"
                    | "required"
                    | "title"
                    | "type"
                    | "uniqueItems"
            )
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn schema_string_set(value: Option<&Value>) -> BTreeSet<&str> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

fn schema_value_map(value: Option<&Value>) -> BTreeMap<String, &Value> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|value| (value.to_string(), value))
        .collect()
}

fn schema_location_segment(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), escape_json_pointer_segment)
}

fn escape_json_pointer_segment(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn display_schema_value(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

fn operation_change(
    code: &str,
    classification: CompatibilityClassification,
    operation: &OwnedOperation,
    only_in: &str,
) -> ContractOperationChange {
    ContractOperationChange {
        code: code.into(),
        classification,
        operation_id: operation.operation_id.clone(),
        method: operation.method,
        path: operation.path.clone(),
        evidence: format!(
            "{} {} ({}) exists only in the {only_in} contract",
            operation.method.as_str(),
            operation.path,
            operation.operation_id
        ),
    }
}

fn aggregate_classification(
    classifications: impl IntoIterator<Item = CompatibilityClassification>,
) -> CompatibilityClassification {
    let classifications = classifications.into_iter().collect::<Vec<_>>();
    if classifications.contains(&CompatibilityClassification::Breaking) {
        CompatibilityClassification::Breaking
    } else if classifications.contains(&CompatibilityClassification::Uncertain) {
        CompatibilityClassification::Uncertain
    } else {
        CompatibilityClassification::NonBreaking
    }
}
