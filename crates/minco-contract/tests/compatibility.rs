use minco_contract::{
    CompatibilityClassification, ContractDocument, ContractOperationChange, ContractSchemaChange,
    HttpMethod, OwnedOperation, diff_contracts, load_contract_source,
};
use serde_json::json;

fn document(sha256: &str, operations: Vec<OwnedOperation>) -> ContractDocument {
    ContractDocument {
        source: "test.yaml".into(),
        openapi_version: "3.1.0".into(),
        title: "Compatibility fixture".into(),
        version: "1.0.0".into(),
        sha256: sha256.into(),
        operations,
        schema_names: Vec::new(),
        raw: json!({}),
    }
}

fn operation(operation_id: &str, method: HttpMethod, path: &str) -> OwnedOperation {
    OwnedOperation {
        operation_id: operation_id.into(),
        method,
        path: path.into(),
        authenticated: false,
        idempotent: false,
    }
}

#[test]
fn added_and_removed_operations_have_deterministic_compatibility_evidence() {
    let baseline = document(
        "baseline",
        vec![
            operation("getWidget", HttpMethod::Get, "/widgets/{id}"),
            operation("createWidget", HttpMethod::Post, "/widgets"),
        ],
    );
    let candidate = document(
        "candidate",
        vec![
            operation("listWidgets", HttpMethod::Get, "/widgets"),
            operation("getWidget", HttpMethod::Get, "/widgets/{id}"),
        ],
    );

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(report.schema_version, 1);
    assert_eq!(report.baseline_sha256, "baseline");
    assert_eq!(report.candidate_sha256, "candidate");
    assert_eq!(report.classification, CompatibilityClassification::Breaking);
    assert_eq!(
        report.operation_changes,
        vec![
            ContractOperationChange {
                code: "operation.added".into(),
                classification: CompatibilityClassification::NonBreaking,
                operation_id: "listWidgets".into(),
                method: HttpMethod::Get,
                path: "/widgets".into(),
                evidence: "GET /widgets (listWidgets) exists only in the candidate contract".into(),
            },
            ContractOperationChange {
                code: "operation.removed".into(),
                classification: CompatibilityClassification::Breaking,
                operation_id: "createWidget".into(),
                method: HttpMethod::Post,
                path: "/widgets".into(),
                evidence: "POST /widgets (createWidget) exists only in the baseline contract"
                    .into(),
            },
        ]
    );
    assert!(report.limitations.iter().any(|limitation| {
        limitation.contains("does not prove semantic business compatibility")
    }));

    let encoded = serde_json::to_string(&report).expect("serialize compatibility report");
    assert_eq!(
        encoded,
        serde_json::to_string(&report).expect("serialize the same report again")
    );
}

#[test]
fn changing_a_stable_operation_binding_is_breaking() {
    let baseline = document(
        "baseline",
        vec![operation("getWidget", HttpMethod::Get, "/widgets/{id}")],
    );
    let candidate = document(
        "candidate",
        vec![operation(
            "getWidget",
            HttpMethod::Post,
            "/widget-queries/{id}",
        )],
    );

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(report.classification, CompatibilityClassification::Breaking);
    assert_eq!(
        report.operation_changes,
        vec![ContractOperationChange {
            code: "operation.binding_changed".into(),
            classification: CompatibilityClassification::Breaking,
            operation_id: "getWidget".into(),
            method: HttpMethod::Post,
            path: "/widget-queries/{id}".into(),
            evidence:
                "getWidget changed binding from GET /widgets/{id} to POST /widget-queries/{id}"
                    .into(),
        }]
    );
}

#[test]
fn requiring_authentication_on_an_existing_operation_is_breaking() {
    let baseline = document(
        "baseline",
        vec![operation("listWidgets", HttpMethod::Get, "/widgets")],
    );
    let mut protected = operation("listWidgets", HttpMethod::Get, "/widgets");
    protected.authenticated = true;
    let candidate = document("candidate", vec![protected]);

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(report.classification, CompatibilityClassification::Breaking);
    assert_eq!(
        report.operation_changes,
        vec![ContractOperationChange {
            code: "operation.authentication_required".into(),
            classification: CompatibilityClassification::Breaking,
            operation_id: "listWidgets".into(),
            method: HttpMethod::Get,
            path: "/widgets".into(),
            evidence: "listWidgets now requires authentication".into(),
        }]
    );
}

#[test]
fn removing_authentication_is_explicitly_uncertain() {
    let mut protected = operation("listWidgets", HttpMethod::Get, "/widgets");
    protected.authenticated = true;
    let baseline = document("baseline", vec![protected]);
    let candidate = document(
        "candidate",
        vec![operation("listWidgets", HttpMethod::Get, "/widgets")],
    );

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(
        report.classification,
        CompatibilityClassification::Uncertain
    );
    assert_eq!(
        report.operation_changes,
        vec![ContractOperationChange {
            code: "operation.authentication_removed".into(),
            classification: CompatibilityClassification::Uncertain,
            operation_id: "listWidgets".into(),
            method: HttpMethod::Get,
            path: "/widgets".into(),
            evidence:
                "listWidgets no longer requires authentication; authorization intent needs review"
                    .into(),
        }]
    );
}

#[test]
fn requiring_idempotency_on_an_existing_operation_is_breaking() {
    let baseline = document(
        "baseline",
        vec![operation("createWidget", HttpMethod::Post, "/widgets")],
    );
    let mut idempotent = operation("createWidget", HttpMethod::Post, "/widgets");
    idempotent.idempotent = true;
    let candidate = document("candidate", vec![idempotent]);

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(report.classification, CompatibilityClassification::Breaking);
    assert_eq!(
        report.operation_changes,
        vec![ContractOperationChange {
            code: "operation.idempotency_required".into(),
            classification: CompatibilityClassification::Breaking,
            operation_id: "createWidget".into(),
            method: HttpMethod::Post,
            path: "/widgets".into(),
            evidence: "createWidget now requires the idempotency contract".into(),
        }]
    );
}

#[test]
fn removing_idempotency_guarantees_is_breaking() {
    let mut idempotent = operation("createWidget", HttpMethod::Post, "/widgets");
    idempotent.idempotent = true;
    let baseline = document("baseline", vec![idempotent]);
    let candidate = document(
        "candidate",
        vec![operation("createWidget", HttpMethod::Post, "/widgets")],
    );

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(report.classification, CompatibilityClassification::Breaking);
    assert_eq!(
        report.operation_changes,
        vec![ContractOperationChange {
            code: "operation.idempotency_removed".into(),
            classification: CompatibilityClassification::Breaking,
            operation_id: "createWidget".into(),
            method: HttpMethod::Post,
            path: "/widgets".into(),
            evidence: "createWidget no longer guarantees idempotent retries".into(),
        }]
    );
}

#[test]
fn added_and_removed_component_schemas_are_classified() {
    let mut baseline = document("baseline", Vec::new());
    baseline.schema_names = vec!["LegacyWidget".into(), "Widget".into()];
    baseline.raw = json!({
        "components": {
            "schemas": {
                "LegacyWidget": {"type": "string"},
                "Widget": {"type": "object", "additionalProperties": false}
            }
        }
    });
    let mut candidate = document("candidate", Vec::new());
    candidate.schema_names = vec!["Widget".into(), "WidgetList".into()];
    candidate.raw = json!({
        "components": {
            "schemas": {
                "Widget": {"type": "object", "additionalProperties": false},
                "WidgetList": {"type": "array", "items": {"$ref": "#/components/schemas/Widget"}}
            }
        }
    });

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(report.classification, CompatibilityClassification::Breaking);
    assert_eq!(
        report.schema_changes,
        vec![
            ContractSchemaChange {
                code: "schema.added".into(),
                classification: CompatibilityClassification::NonBreaking,
                schema: "WidgetList".into(),
                location: "#/components/schemas/WidgetList".into(),
                evidence: "WidgetList exists only in the candidate contract".into(),
            },
            ContractSchemaChange {
                code: "schema.removed".into(),
                classification: CompatibilityClassification::Breaking,
                schema: "LegacyWidget".into(),
                location: "#/components/schemas/LegacyWidget".into(),
                evidence: "LegacyWidget exists only in the baseline contract".into(),
            },
        ]
    );
}

#[test]
fn property_removal_is_breaking_after_local_reference_resolution() {
    let mut baseline = document("baseline", Vec::new());
    baseline.raw = json!({
        "components": {
            "schemas": {
                "Widget": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": {"type": "string"},
                        "name": {"type": "string"}
                    },
                    "required": ["id"]
                }
            }
        }
    });
    let mut candidate = document("candidate", Vec::new());
    candidate.raw = json!({
        "components": {
            "schemas": {
                "Widget": {"$ref": "#/components/schemas/WidgetShape"},
                "WidgetShape": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "id": {"type": "string"}
                    },
                    "required": ["id"]
                }
            }
        }
    });

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(report.classification, CompatibilityClassification::Breaking);
    assert_eq!(
        report.schema_changes,
        vec![
            ContractSchemaChange {
                code: "schema.added".into(),
                classification: CompatibilityClassification::NonBreaking,
                schema: "WidgetShape".into(),
                location: "#/components/schemas/WidgetShape".into(),
                evidence: "WidgetShape exists only in the candidate contract".into(),
            },
            ContractSchemaChange {
                code: "schema.property_removed".into(),
                classification: CompatibilityClassification::Breaking,
                schema: "Widget".into(),
                location: "#/components/schemas/Widget/properties/name".into(),
                evidence: "Widget property name was removed".into(),
            },
        ]
    );
}

#[test]
fn changing_a_schema_type_is_breaking() {
    let mut baseline = document("baseline", Vec::new());
    baseline.raw = json!({
        "components": {"schemas": {"WidgetId": {"type": "string"}}}
    });
    let mut candidate = document("candidate", Vec::new());
    candidate.raw = json!({
        "components": {"schemas": {"WidgetId": {"type": "integer"}}}
    });

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(report.classification, CompatibilityClassification::Breaking);
    assert_eq!(
        report.schema_changes,
        vec![ContractSchemaChange {
            code: "schema.type_changed".into(),
            classification: CompatibilityClassification::Breaking,
            schema: "WidgetId".into(),
            location: "#/components/schemas/WidgetId/type".into(),
            evidence: "WidgetId type changed from string to integer".into(),
        }]
    );
}

#[test]
fn adding_and_removing_type_constraints_are_explicit() {
    let mut baseline = document("baseline", Vec::new());
    baseline.raw = json!({
        "components": {"schemas": {"WidgetValue": {}}}
    });
    let mut constrained = baseline.clone();
    constrained.sha256 = "constrained".into();
    constrained.raw["components"]["schemas"]["WidgetValue"]["type"] = json!("string");

    let added = diff_contracts(&baseline, &constrained);
    let removed = diff_contracts(&constrained, &baseline);

    assert_eq!(added.classification, CompatibilityClassification::Breaking);
    assert_eq!(
        added.schema_changes,
        vec![ContractSchemaChange {
            code: "schema.type_constraint_added".into(),
            classification: CompatibilityClassification::Breaking,
            schema: "WidgetValue".into(),
            location: "#/components/schemas/WidgetValue/type".into(),
            evidence: "WidgetValue added type constraint string".into(),
        }]
    );
    assert_eq!(
        removed.classification,
        CompatibilityClassification::Uncertain
    );
    assert_eq!(
        removed.schema_changes,
        vec![ContractSchemaChange {
            code: "schema.type_constraint_removed".into(),
            classification: CompatibilityClassification::Uncertain,
            schema: "WidgetValue".into(),
            location: "#/components/schemas/WidgetValue/type".into(),
            evidence:
                "WidgetValue removed type constraint string; producer/consumer direction needs review"
                    .into(),
        }]
    );
}

#[test]
fn making_an_existing_property_required_is_breaking() {
    let mut baseline = document("baseline", Vec::new());
    baseline.raw = json!({
        "components": {"schemas": {"Widget": {
            "type": "object",
            "additionalProperties": false,
            "properties": {"name": {"type": "string"}}
        }}}
    });
    let mut candidate = baseline.clone();
    candidate.sha256 = "candidate".into();
    candidate.raw["components"]["schemas"]["Widget"]["required"] = json!(["name"]);

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(report.classification, CompatibilityClassification::Breaking);
    assert_eq!(
        report.schema_changes,
        vec![ContractSchemaChange {
            code: "schema.required_property_added".into(),
            classification: CompatibilityClassification::Breaking,
            schema: "Widget".into(),
            location: "#/components/schemas/Widget/required/name".into(),
            evidence: "Widget property name became required".into(),
        }]
    );
}

#[test]
fn removing_a_required_marker_is_explicitly_uncertain() {
    let mut baseline = document("baseline", Vec::new());
    baseline.raw = json!({
        "components": {"schemas": {"Widget": {
            "type": "object",
            "additionalProperties": false,
            "properties": {"name": {"type": "string"}},
            "required": ["name"]
        }}}
    });
    let mut candidate = baseline.clone();
    candidate.sha256 = "candidate".into();
    candidate.raw["components"]["schemas"]["Widget"]["required"] = json!([]);

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(
        report.classification,
        CompatibilityClassification::Uncertain
    );
    assert_eq!(
        report.schema_changes,
        vec![ContractSchemaChange {
            code: "schema.required_property_removed".into(),
            classification: CompatibilityClassification::Uncertain,
            schema: "Widget".into(),
            location: "#/components/schemas/Widget/required/name".into(),
            evidence:
                "Widget property name is no longer required; request/response usage needs review"
                    .into(),
        }]
    );
}

#[test]
fn adding_an_optional_property_is_non_breaking_and_visible() {
    let mut baseline = document("baseline", Vec::new());
    baseline.raw = json!({
        "components": {"schemas": {"Widget": {
            "type": "object",
            "additionalProperties": false,
            "properties": {"id": {"type": "string"}},
            "required": ["id"]
        }}}
    });
    let mut candidate = baseline.clone();
    candidate.sha256 = "candidate".into();
    candidate.raw["components"]["schemas"]["Widget"]["properties"]["name"] =
        json!({"type": "string"});

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(
        report.classification,
        CompatibilityClassification::NonBreaking
    );
    assert_eq!(
        report.schema_changes,
        vec![ContractSchemaChange {
            code: "schema.optional_property_added".into(),
            classification: CompatibilityClassification::NonBreaking,
            schema: "Widget".into(),
            location: "#/components/schemas/Widget/properties/name".into(),
            evidence: "Widget optional property name was added".into(),
        }]
    );
}

#[test]
fn nested_property_type_changes_are_detected() {
    let mut baseline = document("baseline", Vec::new());
    baseline.raw = json!({
        "components": {"schemas": {"Widget": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "metadata": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {"count": {"type": "integer"}}
                }
            }
        }}}
    });
    let mut candidate = baseline.clone();
    candidate.sha256 = "candidate".into();
    candidate.raw["components"]["schemas"]["Widget"]["properties"]["metadata"]["properties"]["count"]
        ["type"] = json!("string");

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(
        report.schema_changes,
        vec![ContractSchemaChange {
            code: "schema.type_changed".into(),
            classification: CompatibilityClassification::Breaking,
            schema: "Widget".into(),
            location: "#/components/schemas/Widget/properties/metadata/properties/count/type"
                .into(),
            evidence: "Widget type changed from integer to string".into(),
        }]
    );
}

#[test]
fn removing_an_enum_value_is_breaking() {
    let mut baseline = document("baseline", Vec::new());
    baseline.raw = json!({
        "components": {"schemas": {"WidgetState": {
            "type": "string",
            "enum": ["active", "draft"]
        }}}
    });
    let mut candidate = baseline.clone();
    candidate.sha256 = "candidate".into();
    candidate.raw["components"]["schemas"]["WidgetState"]["enum"] = json!(["active"]);

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(
        report.schema_changes,
        vec![ContractSchemaChange {
            code: "schema.enum_value_removed".into(),
            classification: CompatibilityClassification::Breaking,
            schema: "WidgetState".into(),
            location: "#/components/schemas/WidgetState/enum/draft".into(),
            evidence: "WidgetState enum value \"draft\" was removed".into(),
        }]
    );
}

#[test]
fn adding_an_enum_value_is_explicitly_uncertain() {
    let mut baseline = document("baseline", Vec::new());
    baseline.raw = json!({
        "components": {"schemas": {"WidgetState": {
            "type": "string",
            "enum": ["active"]
        }}}
    });
    let mut candidate = baseline.clone();
    candidate.sha256 = "candidate".into();
    candidate.raw["components"]["schemas"]["WidgetState"]["enum"] = json!(["active", "draft"]);

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(
        report.classification,
        CompatibilityClassification::Uncertain
    );
    assert_eq!(
        report.schema_changes,
        vec![ContractSchemaChange {
            code: "schema.enum_value_added".into(),
            classification: CompatibilityClassification::Uncertain,
            schema: "WidgetState".into(),
            location: "#/components/schemas/WidgetState/enum/draft".into(),
            evidence:
                "WidgetState enum value \"draft\" was added; producer/consumer direction needs review"
                    .into(),
        }]
    );
}

#[test]
fn adding_and_removing_enum_constraints_are_explicit() {
    let mut baseline = document("baseline", Vec::new());
    baseline.raw = json!({
        "components": {"schemas": {"WidgetState": {"type": "string"}}}
    });
    let mut constrained = baseline.clone();
    constrained.sha256 = "constrained".into();
    constrained.raw["components"]["schemas"]["WidgetState"]["enum"] = json!(["active", "draft"]);

    let added = diff_contracts(&baseline, &constrained);
    let removed = diff_contracts(&constrained, &baseline);

    assert_eq!(added.classification, CompatibilityClassification::Breaking);
    assert_eq!(
        added.schema_changes,
        vec![ContractSchemaChange {
            code: "schema.enum_constraint_added".into(),
            classification: CompatibilityClassification::Breaking,
            schema: "WidgetState".into(),
            location: "#/components/schemas/WidgetState/enum".into(),
            evidence: "WidgetState added an enum constraint".into(),
        }]
    );
    assert_eq!(
        removed.classification,
        CompatibilityClassification::Uncertain
    );
    assert_eq!(
        removed.schema_changes,
        vec![ContractSchemaChange {
            code: "schema.enum_constraint_removed".into(),
            classification: CompatibilityClassification::Uncertain,
            schema: "WidgetState".into(),
            location: "#/components/schemas/WidgetState/enum".into(),
            evidence:
                "WidgetState removed an enum constraint; producer/consumer direction needs review"
                    .into(),
        }]
    );
}

#[test]
fn unsupported_constraint_changes_never_claim_non_breaking() {
    let mut baseline = document("baseline", Vec::new());
    baseline.raw = json!({
        "components": {"schemas": {"WidgetCode": {
            "type": "string",
            "pattern": "^[a-z]+$"
        }}}
    });
    let mut candidate = baseline.clone();
    candidate.sha256 = "candidate".into();
    candidate.raw["components"]["schemas"]["WidgetCode"]["pattern"] = json!("^[A-Z]+$");

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(
        report.classification,
        CompatibilityClassification::Uncertain
    );
    assert_eq!(
        report.schema_changes,
        vec![ContractSchemaChange {
            code: "schema.constraint_changed".into(),
            classification: CompatibilityClassification::Uncertain,
            schema: "WidgetCode".into(),
            location: "#/components/schemas/WidgetCode/pattern".into(),
            evidence: "WidgetCode pattern changed; request/response compatibility needs review"
                .into(),
        }]
    );
}

#[test]
fn unclassified_schema_keywords_never_claim_non_breaking() {
    let mut baseline = document("baseline", Vec::new());
    baseline.raw = json!({
        "components": {"schemas": {"Widget": {
            "type": "object",
            "additionalProperties": false
        }}}
    });
    let mut candidate = baseline.clone();
    candidate.sha256 = "candidate".into();
    candidate.raw["components"]["schemas"]["Widget"]["readOnly"] = json!(true);

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(
        report.classification,
        CompatibilityClassification::Uncertain
    );
    assert_eq!(
        report.schema_changes,
        vec![ContractSchemaChange {
            code: "schema.structure_changed".into(),
            classification: CompatibilityClassification::Uncertain,
            schema: "Widget".into(),
            location: "#/components/schemas/Widget".into(),
            evidence:
                "Widget schema structure changed outside the bounded classifier at #/components/schemas/Widget"
                    .into(),
        }]
    );
}

#[test]
fn unclassified_operation_structure_changes_are_uncertain() {
    let mut baseline = document(
        "baseline",
        vec![operation("getWidget", HttpMethod::Get, "/widgets/{id}")],
    );
    baseline.raw = json!({
        "paths": {
            "/widgets/{id}": {
                "get": {
                    "operationId": "getWidget",
                    "responses": {
                        "200": {
                            "description": "ok",
                            "content": {
                                "application/json": {"schema": {"type": "string"}}
                            }
                        }
                    }
                }
            }
        }
    });
    let mut candidate = baseline.clone();
    candidate.sha256 = "candidate".into();
    candidate.raw["paths"]["/widgets/{id}"]["get"]["responses"]["200"]["content"]["application/json"]
        ["schema"]["type"] = json!("integer");

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(
        report.classification,
        CompatibilityClassification::Uncertain
    );
    assert_eq!(
        report.operation_changes,
        vec![ContractOperationChange {
            code: "operation.structure_changed".into(),
            classification: CompatibilityClassification::Uncertain,
            operation_id: "getWidget".into(),
            method: HttpMethod::Get,
            path: "/widgets/{id}".into(),
            evidence:
                "getWidget request/response/parameter structure changed outside the bounded classifier"
                    .into(),
        }]
    );
}

#[test]
fn resource_convention_changes_are_explicitly_classified() {
    let operation = operation("listWidgets", HttpMethod::Get, "/widgets");
    let mut baseline = document("baseline", vec![operation.clone()]);
    baseline.raw = json!({
        "paths": {
            "/widgets": {
                "get": {
                    "x-minco-resource": {
                        "name": "widget",
                        "action": "list",
                        "defaultLimit": 20,
                        "maxLimit": 100,
                        "defaultSort": ["-id"],
                        "sortFields": ["id"],
                        "filterFields": [],
                        "cursorFields": ["id"]
                    }
                }
            }
        }
    });
    let mut candidate = document("candidate", vec![operation]);
    candidate.raw = json!({
        "paths": {
            "/widgets": {
                "get": {
                    "x-minco-resource": {
                        "name": "widget",
                        "action": "list",
                        "defaultLimit": 20,
                        "maxLimit": 50,
                        "defaultSort": ["-id"],
                        "sortFields": ["id"],
                        "filterFields": [],
                        "cursorFields": ["id"]
                    }
                }
            }
        }
    });

    let report = diff_contracts(&baseline, &candidate);

    assert!(report.operation_changes.iter().any(|change| {
        change.code == "operation.resource_convention_changed"
            && change.classification == CompatibilityClassification::Breaking
    }));
}

#[test]
fn unresolved_operation_references_are_never_silently_compatible() {
    let mut baseline = document(
        "baseline",
        vec![operation("getWidget", HttpMethod::Get, "/widgets/{id}")],
    );
    baseline.raw = json!({
        "paths": {
            "/widgets/{id}": {
                "get": {
                    "operationId": "getWidget",
                    "responses": {
                        "200": {
                            "description": "ok",
                            "content": {
                                "application/json": {
                                    "schema": {"$ref": "baseline.yaml#/Widget"}
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    let mut candidate = baseline.clone();
    candidate.sha256 = "candidate".into();
    candidate.raw["paths"]["/widgets/{id}"]["get"]["responses"]["200"]["content"]["application/json"]
        ["schema"]["$ref"] = json!("candidate.yaml#/Widget");

    let report = diff_contracts(&baseline, &candidate);

    assert_eq!(
        report.classification,
        CompatibilityClassification::Uncertain
    );
    assert_eq!(
        report.operation_changes,
        vec![ContractOperationChange {
            code: "operation.structure_changed".into(),
            classification: CompatibilityClassification::Uncertain,
            operation_id: "getWidget".into(),
            method: HttpMethod::Get,
            path: "/widgets/{id}".into(),
            evidence:
                "getWidget request/response/parameter structure changed outside the bounded classifier"
                    .into(),
        }]
    );
}

#[test]
fn revision_contract_source_uses_the_same_validated_loader() {
    let source = r"
openapi: 3.1.0
info: {title: Revision contract, version: 1.0.0}
paths:
  /health:
    get:
      operationId: getHealth
      security: []
      responses:
        '200': {description: ok}
        default:
          description: problem
          content:
            application/problem+json:
              schema: {type: string}
components:
  schemas: {}
";

    let report =
        load_contract_source("main:openapi/openapi.yaml", source).expect("parse revision contract");

    assert!(report.is_valid());
    assert_eq!(
        report.document.source,
        "main:openapi/openapi.yaml".to_owned()
    );
    assert_eq!(report.document.operations[0].operation_id, "getHealth");
}
