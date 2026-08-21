use minco_contract::{ContractFinding, generate_rust, load_contract_source};
use std::fmt::Write as _;

fn contract(request_schema: &str, extra_schemas: &str, response_schema: &str) -> String {
    format!(
        r"
openapi: 3.1.1
info: {{title: Request profile, version: 1.0.0}}
x-minco-request-validation: generated
paths:
  /items:
    post:
      operationId: createItem
      security: []
      requestBody:
        required: true
        content:
          application/json:
            schema:
{request_schema}
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema:
{response_schema}
        default:
          description: problem
          content:
            application/problem+json:
              schema: {{type: string}}
components:
  schemas:
{extra_schemas}
"
    )
}

fn findings(source: &str) -> Vec<ContractFinding> {
    load_contract_source("request-profile.yaml", source)
        .expect("contract parses")
        .findings
}

#[test]
fn response_only_unsupported_assertions_do_not_block_the_request_profile() {
    let source = contract(
        "              $ref: '#/components/schemas/Empty'",
        "    Empty: {type: object, additionalProperties: false}",
        "                oneOf:\n                  - {type: string}\n                  - {type: integer}",
    );

    let findings = findings(&source);

    assert!(
        findings.is_empty(),
        "response-only schema must not be request-validated: {findings:?}"
    );
}

#[test]
fn request_reachable_unsupported_assertions_fail_with_a_stable_diagnostic() {
    let source = contract(
        "              $ref: '#/components/schemas/CreateItem'",
        "    CreateItem:\n      oneOf:\n        - {type: string}\n        - {type: integer}",
        "                type: string",
    );

    let findings = findings(&source);

    assert!(findings.iter().any(|finding| {
        finding.code == "MINCO-CONTRACT-030"
            && finding.location == "$.components.schemas.CreateItem.oneOf"
    }));
}

#[test]
fn request_reachable_reference_semantics_fail_closed() {
    for keyword in ["$id", "$schema", "$anchor"] {
        let source = contract(
            "              $ref: '#/components/schemas/CreateItem'",
            &format!(
                "    CreateItem:\n      type: string\n      {keyword}: 'https://example.invalid/schema'"
            ),
            "                type: string",
        );

        let findings = findings(&source);

        assert!(findings.iter().any(|finding| {
            finding.code == "MINCO-CONTRACT-030"
                && finding.location == format!("$.components.schemas.CreateItem.{keyword}")
        }));
    }
}

#[test]
fn invalid_profile_spelling_fails_closed() {
    let source = contract(
        "              type: string",
        "    Empty: {type: object, additionalProperties: false}",
        "                type: string",
    )
    .replace(
        "x-minco-request-validation: generated",
        "x-minco-request-validation: runtime",
    );

    let findings = findings(&source);

    assert!(findings.iter().any(|finding| {
        finding.code == "MINCO-CONTRACT-029" && finding.location == "x-minco-request-validation"
    }));
}

#[test]
fn external_and_recursive_request_references_fail_closed() {
    let external = contract(
        "              $ref: 'https://example.invalid/schema.yaml'",
        "    Empty: {type: object, additionalProperties: false}",
        "                type: string",
    );
    let recursive = contract(
        "              $ref: '#/components/schemas/Node'",
        "    Node:\n      type: object\n      additionalProperties: false\n      properties:\n        child: {$ref: '#/components/schemas/Node'}",
        "                type: string",
    );

    assert!(
        findings(&external)
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-031")
    );
    assert!(
        findings(&recursive)
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-032")
    );
}

#[test]
fn malformed_request_bounds_fail_with_a_stable_diagnostic() {
    let source = contract(
        "              $ref: '#/components/schemas/CreateItem'",
        "    CreateItem:\n      type: string\n      minLength: 4\n      maxLength: 2",
        "                type: string",
    );

    let findings = findings(&source);

    assert!(findings.iter().any(|finding| {
        finding.code == "MINCO-CONTRACT-034"
            && finding.location == "$.components.schemas.CreateItem"
    }));
}

#[test]
fn opted_in_request_dtos_generate_direct_nested_validation_and_non_null_deserialization() {
    let source = contract(
        "              $ref: '#/components/schemas/CreateItem'",
        "    CreateItem:\n      type: object\n      additionalProperties: false\n      minProperties: 1\n      properties:\n        label: {type: string, minLength: 2, maxLength: 4}\n        lines: {type: array, minItems: 1, maxItems: 3, items: {$ref: '#/components/schemas/Line'}}\n    Line:\n      type: object\n      additionalProperties: false\n      required: [quantity]\n      properties:\n        quantity: {type: integer, minimum: 2, exclusiveMaximum: 10}",
        "                type: string",
    );
    let report = load_contract_source("generated-request.yaml", &source).expect("contract parses");
    assert!(report.is_valid(), "{:?}", report.findings);

    let generated = generate_rust(&report.document);

    assert!(generated.contains("impl ContractValidate for CreateItem"));
    assert!(generated.contains("let character_count = value.chars().count();"));
    assert!(generated.contains("character_count < 2"));
    assert!(generated.contains("value.len() > 3"));
    assert!(generated.contains("value.iter().take(3).enumerate()"));
    assert!(generated.contains("if errors.is_truncated()"));
    assert!(generated.contains("errors.at_index(index"));
    assert!(generated.contains("item.validate_contract(errors)"));
    assert!(generated.contains("i128::from(*value) < 2"));
    assert!(generated.contains("i128::from(*value) >= 10"));
    assert!(generated.contains("deserialize_with = \"deserialize_optional_non_null\""));
}

#[test]
fn contracts_without_the_profile_keep_the_existing_optional_field_behavior() {
    let source = contract(
        "              $ref: '#/components/schemas/CreateItem'",
        "    CreateItem:\n      type: object\n      additionalProperties: false\n      properties:\n        label: {type: string, minLength: 2}",
        "                type: string",
    )
    .replace("x-minco-request-validation: generated\n", "");
    let report = load_contract_source("legacy-generation.yaml", &source).expect("contract parses");
    assert!(report.is_valid(), "{:?}", report.findings);

    let generated = generate_rust(&report.document);

    assert!(!generated.contains("ContractValidate"));
    assert!(!generated.contains("deserialize_optional_non_null"));
    assert!(!generated.contains("use minco_contract::ContractAuthorizationAlternative;"));
    assert!(generated.contains(
        "#[serde(default, skip_serializing_if = \"Option::is_none\")]\n    pub label: Option<String>"
    ));
}

#[test]
fn generated_profile_does_not_change_response_only_dto_deserialization() {
    let source = contract(
        "              $ref: '#/components/schemas/CreateItem'",
        "    CreateItem:\n      type: object\n      additionalProperties: false\n      properties:\n        label: {type: string}\n    ResponseOnly:\n      type: object\n      additionalProperties: false\n      properties:\n        detail: {type: string}",
        "                $ref: '#/components/schemas/ResponseOnly'",
    );
    let report =
        load_contract_source("response-only-shape.yaml", &source).expect("contract parses");
    assert!(report.is_valid(), "{:?}", report.findings);

    let generated = generate_rust(&report.document);
    let response_only = generated
        .split("pub struct ResponseOnly")
        .nth(1)
        .expect("response-only DTO")
        .split('}')
        .next()
        .expect("response-only DTO body");

    assert!(response_only.contains("default, skip_serializing_if"));
    assert!(!response_only.contains("deserialize_optional_non_null"));
    assert!(!generated.contains("impl ContractValidate for ResponseOnly"));
}

#[test]
fn generated_authorization_is_separate_from_the_frozen_operation_shape() {
    let source = contract(
        "              $ref: '#/components/schemas/Empty'",
        "    Empty: {type: object, additionalProperties: false}",
        "                type: string",
    )
    .replace(
        "      security: []\n      requestBody:",
        "      security:\n        - oauth: [items.write, openid]\n        - administrator: [admin]\n      x-minco-auth:\n        mode: permission_scoped\n        permissions: [items.write]\n      requestBody:",
    );
    let report =
        load_contract_source("authorization-generation.yaml", &source).expect("contract parses");
    assert!(report.is_valid(), "{:?}", report.findings);

    let generated = generate_rust(&report.document);

    assert!(generated.contains("CREATE_ITEM_AUTHORIZATION"));
    assert!(generated.contains("use minco_contract::ContractAuthorizationAlternative;"));
    assert!(generated.contains("ContractAuthorizationPolicy::new("));
    assert!(
        generated.contains("ContractAuthorizationAlternative::new(&[\"items.write\", \"openid\"])")
    );
    assert!(generated.contains("&[\"items.write\"]"));
    assert!(generated.contains(
        "ContractOperation::new(\n    \"createItem\",\n    HttpMethod::Post,\n    \"/items\",\n    true,"
    ));
}

#[test]
fn scalar_enum_and_const_generation_handles_nullable_null_without_accepting_non_null_values() {
    let source = contract(
        "              $ref: '#/components/schemas/CreateItem'",
        "    CreateItem:\n      type: object\n      additionalProperties: false\n      required: [nullableRequired]\n      properties:\n        nullableRequired: {type: [string, 'null']}\n        nullableConst: {type: [string, 'null'], const: null}\n        nullableEnum: {type: [string, 'null'], enum: [null]}\n        exact: {type: string, const: fixed}\n        choice: {type: integer, enum: [1, 2]}",
        "                type: string",
    );
    let report = load_contract_source("scalar-values.yaml", &source).expect("contract parses");
    assert!(report.is_valid(), "{:?}", report.findings);

    let first = generate_rust(&report.document);
    let second = generate_rust(&report.document);

    assert_eq!(first, second, "generation must be deterministic");
    assert!(first.contains("deserialize_with = \"minco_contract::deserialize_required_nullable\""));
    assert!(first.contains("value.as_str() == \"fixed\""));
    assert!(first.contains("*value == 1 || *value == 2"));
    assert!(first.contains(
        "errors.at_field(\"nullableConst\", |errors| {\n                errors.add(\"must equal the required value\");"
    ));
    assert!(first.contains(
        "errors.at_field(\"nullableEnum\", |errors| {\n                errors.add(\"must be an allowed value\");"
    ));
}

#[test]
fn request_schema_reference_depth_property_identifier_and_enum_limits_fail_closed() {
    let mut chain = String::new();
    for index in 0..34 {
        if index == 33 {
            writeln!(chain, "    S{index}: {{type: string}}").expect("write to String");
        } else {
            writeln!(
                chain,
                "    S{index}: {{$ref: '#/components/schemas/S{}'}}",
                index + 1
            )
            .expect("write to String");
        }
    }
    let depth = contract(
        "              $ref: '#/components/schemas/S0'",
        &chain,
        "                type: string",
    );

    let properties = (0..257)
        .map(|index| format!("        p{index:03}: {{type: string}}"))
        .collect::<Vec<_>>()
        .join("\n");
    let property_count = contract(
        "              $ref: '#/components/schemas/CreateItem'",
        &format!(
            "    CreateItem:\n      type: object\n      additionalProperties: false\n      properties:\n{properties}"
        ),
        "                type: string",
    );

    let long_name = "x".repeat(129);
    let identifier = contract(
        "              $ref: '#/components/schemas/CreateItem'",
        &format!(
            "    CreateItem:\n      type: object\n      additionalProperties: false\n      properties:\n        {long_name}: {{type: string}}"
        ),
        "                type: string",
    );
    let enum_values = (0..129)
        .map(|index| format!("v{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let enum_limit = contract(
        "              $ref: '#/components/schemas/CreateItem'",
        &format!("    CreateItem: {{type: string, enum: [{enum_values}]}}"),
        "                type: string",
    );
    let child_properties = (0..256)
        .map(|index| format!("        p{index:03}: {{type: string}}"))
        .collect::<Vec<_>>()
        .join("\n");
    let root_properties = (0..17)
        .map(|index| format!("        child{index}: {{$ref: '#/components/schemas/C{index}'}}"))
        .collect::<Vec<_>>()
        .join("\n");
    let children = (0..17)
        .map(|index| {
            format!(
                "    C{index}:\n      type: object\n      additionalProperties: false\n      properties:\n{child_properties}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let complexity = contract(
        "              $ref: '#/components/schemas/Root'",
        &format!(
            "    Root:\n      type: object\n      additionalProperties: false\n      properties:\n{root_properties}\n{children}"
        ),
        "                type: string",
    );

    assert!(
        findings(&depth)
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-032")
    );
    for source in [&property_count, &identifier] {
        assert!(
            findings(source)
                .iter()
                .any(|finding| finding.code == "MINCO-CONTRACT-033")
        );
    }
    assert!(
        findings(&enum_limit)
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-034")
    );
    assert!(findings(&complexity).iter().any(|finding| {
        finding.code == "MINCO-CONTRACT-033" && finding.message.contains("complexity")
    }));
}

#[test]
fn unresolved_local_refs_and_schema_diagnostics_are_deterministic() {
    let unresolved = contract(
        "              $ref: '#/components/schemas/Missing'",
        "    Empty: {type: object, additionalProperties: false}",
        "                type: string",
    );
    assert!(findings(&unresolved).iter().any(|finding| {
        finding.code == "MINCO-CONTRACT-031" && finding.location.ends_with(".$ref")
    }));

    let unsupported = contract(
        "              $ref: '#/components/schemas/CreateItem'",
        "    CreateItem:\n      type: object\n      additionalProperties: false\n      properties:\n        z: {type: string, pattern: z-secret-input}\n        a: {type: string, pattern: a-secret-input}",
        "                type: string",
    );
    let findings = findings(&unsupported);
    let locations = findings
        .iter()
        .filter(|finding| finding.code == "MINCO-CONTRACT-030")
        .map(|finding| finding.location.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        locations,
        [
            "$.components.schemas.CreateItem.properties.a.pattern",
            "$.components.schemas.CreateItem.properties.z.pattern",
        ]
    );
    assert!(findings.iter().all(|finding| {
        !finding.message.contains("a-secret-input") && !finding.message.contains("z-secret-input")
    }));
}

#[test]
fn unrepresentable_presence_and_generated_identifier_combinations_fail_closed() {
    for schemas in [
        "    CreateItem:\n      type: object\n      additionalProperties: false\n      minProperties: 1\n      properties:\n        value: {type: [string, 'null']}",
        "    CreateItem:\n      type: object\n      additionalProperties: false\n      properties:\n        value: {type: [string, 'null'], enum: [present]}",
        "    CreateItem:\n      type: object\n      additionalProperties: false\n      required: [missing]\n      properties:\n        value: {type: string}",
        "    CreateItem:\n      type: object\n      additionalProperties: false\n      properties:\n        a-b: {type: string}\n        a_b: {type: string}",
        "    CreateItem: {type: string, enum: [a-b, a_b]}",
    ] {
        let source = contract(
            "              $ref: '#/components/schemas/CreateItem'",
            schemas,
            "                type: string",
        );
        let findings = findings(&source);
        assert!(
            findings.iter().any(|finding| {
                matches!(
                    finding.code.as_str(),
                    "MINCO-CONTRACT-033" | "MINCO-CONTRACT-034" | "MINCO-CONTRACT-035"
                )
            }),
            "schema must fail closed: {schemas}\n{findings:?}"
        );
    }
}

#[test]
fn request_shapes_without_a_lossless_generated_representation_fail_closed() {
    for schemas in [
        "    CreateItem: {type: string, minLength: 2}",
        "    CreateItem: {type: array, maxItems: 2, items: {type: string}}",
        "    CreateItem: {type: 'null'}",
        "    CreateItem:\n      type: object\n      additionalProperties: false\n      properties:\n        nested: {type: object, additionalProperties: false}",
        "    CreateItem:\n      type: object\n      additionalProperties: false\n      properties:\n        amount: {type: number, minimum: 0}",
        "    CreateItem:\n      type: object\n      additionalProperties: false\n      required: [serverValue]\n      properties:\n        serverValue: {type: string, readOnly: true}",
        "    CreateItem:\n      type: object\n      additionalProperties: false\n      properties:\n        dotted.path: {type: string}",
        "    CreateItem:\n      type: object\n      additionalProperties: false\n      properties:\n        value: {type: string, enum: [valid, 1]}",
        "    CreateItem:\n      type: object\n      additionalProperties: false\n      properties:\n        value: {type: integer, minimum: 1.5}",
        "    CreateItem:\n      type: object\n      additionalProperties: false\n      properties:\n        value: {type: string, format: uuid, minLength: 36}",
        "    CreateItem: {type: string, enum: [a, admin], minLength: 2}",
        "    CreateItem:\n      type: object\n      additionalProperties: false\n      properties:\n        choice: {$ref: '#/components/schemas/Choice'}\n    Choice: {type: string, enum: [a, admin], const: admin}",
        "    CreateItem:\n      type: object\n      additionalProperties: false\n      properties:\n        value: {$ref: '#/components/schemas/Label', minLength: 2}\n    Label: {type: string, enum: [valid]}",
    ] {
        let source = contract(
            "              $ref: '#/components/schemas/CreateItem'",
            schemas,
            "                type: string",
        );
        let findings = findings(&source);
        assert!(
            findings.iter().any(|finding| {
                matches!(
                    finding.code.as_str(),
                    "MINCO-CONTRACT-033" | "MINCO-CONTRACT-035"
                )
            }),
            "schema must fail before generation: {schemas}\n{findings:?}"
        );
    }
}

#[test]
fn parameter_content_nested_component_pointers_and_chained_refs_fail_closed() {
    let base = contract(
        "              $ref: '#/components/schemas/CreateItem'",
        "    CreateItem: {type: object, additionalProperties: false}",
        "                type: string",
    );
    let content = base.replace(
        "      security: []\n      requestBody:",
        "      security: []\n      parameters:\n        - name: filter\n          in: query\n          content:\n            application/json:\n              schema: {type: string, minLength: 2}\n      requestBody:",
    );
    assert!(findings(&content).iter().any(|finding| {
        finding.code == "MINCO-CONTRACT-035" && finding.location.ends_with(".content")
    }));

    for (reference, parameters) in [
        (
            "#/components/parameters/P/schema",
            "    P: {name: filter, in: query, schema: {type: string}}",
        ),
        (
            "#/components/parameters/P",
            "    P: {$ref: '#/components/parameters/Q'}\n    Q: {name: filter, in: query, schema: {type: string}}",
        ),
        (
            "#/components/parameters/P",
            "    P: {$ref: '#/components/parameters/P'}",
        ),
    ] {
        let source = base
            .replace(
                "      security: []\n      requestBody:",
                &format!(
                    "      security: []\n      parameters:\n        - {{$ref: '{reference}'}}\n      requestBody:"
                ),
            )
            .replace(
                "components:\n  schemas:",
                &format!("components:\n  parameters:\n{parameters}\n  schemas:"),
            );
        assert!(
            findings(&source)
                .iter()
                .any(|finding| finding.code == "MINCO-CONTRACT-031"),
            "component reference must fail closed: {reference}"
        );
    }
}

#[test]
fn malformed_parameters_and_request_bodies_fail_closed() {
    let base = contract(
        "              $ref: '#/components/schemas/CreateItem'",
        "    CreateItem: {type: object, additionalProperties: false}",
        "                type: string",
    );

    let inline_parameter = base.replace(
        "      security: []\n      requestBody:",
        "      security: []\n      parameters:\n        - name: filter\n          schema: {type: string}\n      requestBody:",
    );
    assert!(
        findings(&inline_parameter)
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-034")
    );

    let referenced_parameter = base
        .replace(
            "      security: []\n      requestBody:",
            "      security: []\n      parameters:\n        - {$ref: '#/components/parameters/P'}\n      requestBody:",
        )
        .replace(
            "components:\n  schemas:",
            "components:\n  parameters:\n    P: {name: filter, schema: {type: string}}\n  schemas:",
        );
    assert!(
        findings(&referenced_parameter)
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-034")
    );

    let missing_content = base.replace(
        "      requestBody:\n        required: true\n        content:\n          application/json:\n            schema:\n              $ref: '#/components/schemas/CreateItem'",
        "      requestBody:\n        required: true",
    );
    assert!(
        findings(&missing_content)
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-034")
    );

    let missing_json_schema = base.replace(
        "        content:\n          application/json:\n            schema:\n              $ref: '#/components/schemas/CreateItem'",
        "        content:\n          application/json: {}",
    );
    assert!(
        findings(&missing_json_schema)
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-034")
    );

    let referenced_request_body = base
        .replace(
            "      requestBody:\n        required: true\n        content:\n          application/json:\n            schema:\n              $ref: '#/components/schemas/CreateItem'",
            "      requestBody:\n        $ref: '#/components/requestBodies/B'",
        )
        .replace(
            "components:\n  schemas:",
            "components:\n  requestBodies:\n    B: {required: true}\n  schemas:",
        );
    assert!(
        findings(&referenced_request_body)
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-034")
    );
}

#[test]
fn generated_string_enum_variants_escape_reserved_rust_identifiers() {
    let source = contract(
        "              $ref: '#/components/schemas/Choice'",
        "    Choice: {type: string, enum: [self, admin]}",
        "                type: string",
    );
    let report = load_contract_source("reserved-enum.yaml", &source).expect("contract parses");
    assert!(report.is_valid(), "{:?}", report.findings);

    let generated = generate_rust(&report.document);
    assert!(generated.contains("ValueSelf"));
    assert!(!generated.contains("    Self,"));
}
