use minco_contract::{ResourceAction, load_contract, load_contract_source};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn codes(name: &str) -> Vec<String> {
    load_contract(fixture(name))
        .expect("fixture parses")
        .findings
        .into_iter()
        .map(|finding| finding.code)
        .collect()
}

#[test]
fn explicit_closed_and_rationalized_open_objects_are_valid() {
    let report = load_contract(fixture("valid-policy.yaml")).expect("fixture parses");
    assert!(report.is_valid(), "{:?}", report.findings);
}

#[test]
fn open_objects_require_an_explicit_rationale() {
    assert!(codes("invalid-open-object.yaml").contains(&"MINCO-CONTRACT-009".to_owned()));
}

#[test]
fn idempotency_metadata_is_bidirectionally_consistent() {
    assert!(codes("invalid-idempotency.yaml").contains(&"MINCO-CONTRACT-015".to_owned()));
}

#[test]
fn authentication_metadata_cannot_contradict_openapi_security() {
    assert!(codes("invalid-auth.yaml").contains(&"MINCO-CONTRACT-016".to_owned()));
}

#[test]
fn permission_metadata_requires_a_nonempty_validated_scope_set() {
    assert!(codes("invalid-permission.yaml").contains(&"MINCO-CONTRACT-016".to_owned()));
}

#[test]
fn policy_relevant_parameter_references_must_resolve_locally() {
    assert!(codes("invalid-parameter-ref.yaml").contains(&"MINCO-CONTRACT-021".to_owned()));
}

#[test]
fn malformed_effective_security_has_a_stable_diagnostic() {
    assert!(codes("invalid-security-shape.yaml").contains(&"MINCO-CONTRACT-020".to_owned()));
}

#[test]
fn malformed_security_requirement_entries_have_stable_diagnostics() {
    assert_eq!(
        codes("invalid-security-requirements.yaml")
            .iter()
            .filter(|code| code.as_str() == "MINCO-CONTRACT-020")
            .count(),
        4
    );
}

#[test]
fn absent_empty_and_mixed_anonymous_security_are_public() {
    let report = load_contract(fixture("security-variants.yaml")).expect("fixture parses");
    assert!(report.is_valid(), "{:?}", report.findings);
    assert!(
        report
            .document
            .operations
            .iter()
            .all(|operation| !operation.authenticated)
    );
}

#[test]
fn path_level_referenced_idempotency_parameters_are_effective() {
    let report = load_contract(fixture("valid-policy.yaml")).expect("fixture parses");
    assert!(report.is_valid(), "{:?}", report.findings);
    assert!(
        report
            .document
            .operations
            .iter()
            .find(|operation| operation.operation_id == "createWidget")
            .is_some_and(|operation| operation.idempotent)
    );
}

#[test]
fn error_responses_use_problem_details_media_type() {
    assert!(codes("invalid-problem-media.yaml").contains(&"MINCO-CONTRACT-017".to_owned()));
}

#[test]
fn resource_metadata_is_exposed_to_contract_consumers() {
    let report = load_contract_source(
        "resource.yaml",
        r"
openapi: 3.1.0
info: { title: Resource API, version: 1.0.0 }
paths:
  /orders:
    post:
      operationId: placeOrder
      x-minco-resource:
        name: order
        action: create
      x-minco-idempotent: true
      parameters:
        - in: header
          name: Idempotency-Key
          required: true
          schema: { type: string }
      responses:
        '201':
          description: Created
          headers:
            ETag:
              schema: { type: string }
            Location:
              schema: { type: string }
          content:
            application/json:
              schema:
                type: object
                additionalProperties: false
                required: [data]
                properties:
                  data:
                    type: object
                    additionalProperties: false
        default:
          description: Problem
          content:
            application/problem+json:
              schema:
                type: object
                additionalProperties: false
",
    )
    .expect("contract parses");

    assert!(report.is_valid(), "{:?}", report.findings);
    let resources = report.document.resource_operations();
    let resource = resources.first().expect("resource metadata");
    assert_eq!(resource.operation_id, "placeOrder");
    assert_eq!(resource.name, "order");
    assert_eq!(resource.action, ResourceAction::Create);
}

#[test]
fn malformed_resource_metadata_has_a_stable_diagnostic() {
    let report = load_contract_source(
        "invalid-resource.yaml",
        r"
openapi: 3.1.0
info: { title: Resource API, version: 1.0.0 }
paths:
  /orders:
    post:
      operationId: placeOrder
      x-minco-resource:
        name: Order
        action: upsert
      responses:
        '201': { description: Created }
        default:
          description: Problem
          content:
            application/problem+json:
              schema:
                type: object
                additionalProperties: false
",
    )
    .expect("contract parses");

    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-022")
    );
}

#[test]
fn conditional_resource_writes_require_if_match() {
    let report = load_contract_source(
        "missing-if-match.yaml",
        r"
openapi: 3.1.0
info: { title: Resource API, version: 1.0.0 }
paths:
  /orders/{orderId}:
    patch:
      operationId: updateOrder
      x-minco-resource:
        name: order
        action: update
      responses:
        '200': { description: Updated }
        default:
          description: Problem
          content:
            application/problem+json:
              schema:
                type: object
                additionalProperties: false
",
    )
    .expect("contract parses");

    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-023")
    );
}

#[test]
fn conditional_resource_writes_declare_precondition_problems() {
    let report = load_contract_source(
        "missing-precondition-responses.yaml",
        r"
openapi: 3.1.0
info: { title: Resource API, version: 1.0.0 }
paths:
  /orders/{orderId}:
    delete:
      operationId: deleteOrder
      x-minco-resource:
        name: order
        action: delete
      parameters:
        - in: header
          name: If-Match
          required: true
          schema: { type: string }
      responses:
        '204': { description: Deleted }
        default:
          description: Problem
          content:
            application/problem+json:
              schema:
                type: object
                additionalProperties: false
",
    )
    .expect("contract parses");

    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-024")
    );
}

#[test]
fn resource_reads_require_a_data_envelope_and_etag() {
    let report = load_contract_source(
        "invalid-read-success.yaml",
        r"
openapi: 3.1.0
info: { title: Resource API, version: 1.0.0 }
paths:
  /orders/{orderId}:
    get:
      operationId: getOrder
      x-minco-resource:
        name: order
        action: read
      responses:
        '200':
          description: Found
          headers:
            ETag: { schema: { type: string } }
          content:
            application/json:
              schema:
                type: object
                additionalProperties: false
                required: [data]
                properties: {}
        default:
          description: Problem
          content:
            application/problem+json:
              schema:
                type: object
                additionalProperties: false
",
    )
    .expect("contract parses");

    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-025")
    );
}

#[test]
fn resource_lists_require_bounded_cursor_and_allowlisted_queries() {
    let report = load_contract_source(
        "invalid-list-policy.yaml",
        r"
openapi: 3.1.0
info: { title: Resource API, version: 1.0.0 }
paths:
  /orders:
    get:
      operationId: listOrders
      x-minco-resource:
        name: order
        action: list
      responses:
        '200':
          description: Orders
          content:
            application/json:
              schema:
                type: object
                additionalProperties: false
                required: [data]
                properties:
                  data:
                    type: array
                    items: { type: string }
        default:
          description: Problem
          content:
            application/problem+json:
              schema:
                type: object
                additionalProperties: false
",
    )
    .expect("contract parses");

    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-026")
    );
}

#[test]
fn resource_list_policy_must_be_realized_by_parameters_and_page_shape() {
    let report = load_contract_source(
        "unrealized-list-policy.yaml",
        r"
openapi: 3.1.0
info: { title: Resource API, version: 1.0.0 }
paths:
  /orders:
    get:
      operationId: listOrders
      x-minco-resource:
        name: order
        action: list
        defaultLimit: 20
        maxLimit: 100
        defaultSort: [-createdAt, -id]
        sortFields: [createdAt, id]
        filterFields: [status]
        cursorFields: [createdAt, id]
      responses:
        '200':
          description: Orders
          content:
            application/json:
              schema:
                type: object
                additionalProperties: false
                required: [data]
                properties:
                  data:
                    type: array
                    items: { type: string }
        default:
          description: Problem
          content:
            application/problem+json:
              schema:
                type: object
                additionalProperties: false
",
    )
    .expect("contract parses");

    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-026")
    );
}

#[test]
fn bounded_cursor_resource_list_contract_is_valid() {
    let report = load_contract_source(
        "valid-list-policy.yaml",
        r"
openapi: 3.1.0
info: { title: Resource API, version: 1.0.0 }
paths:
  /orders:
    get:
      operationId: listOrders
      x-minco-resource:
        name: order
        action: list
        defaultLimit: 20
        maxLimit: 100
        defaultSort: [-createdAt, -id]
        sortFields: [createdAt, id]
        filterFields: [status]
        cursorFields: [createdAt, id]
      parameters:
        - name: page[limit]
          in: query
          schema:
            type: integer
            minimum: 1
            maximum: 100
            default: 20
        - name: page[after]
          in: query
          schema:
            type: string
            minLength: 1
            maxLength: 512
        - name: sort
          in: query
          schema: { type: string }
        - name: filter[status]
          in: query
          schema: { type: string }
      responses:
        '200':
          description: Orders
          content:
            application/json:
              schema:
                type: object
                additionalProperties: false
                required: [data, page]
                properties:
                  data:
                    type: array
                    items: { type: string }
                  page:
                    type: object
                    additionalProperties: false
                    required: [hasMore, nextCursor]
                    properties:
                      hasMore: { type: boolean }
                      nextCursor: { type: [string, 'null'] }
        default:
          description: Problem
          content:
            application/problem+json:
              schema:
                type: object
                additionalProperties: false
",
    )
    .expect("contract parses");

    assert!(report.is_valid(), "{:?}", report.findings);
}

#[test]
fn resource_creates_require_idempotency_and_location() {
    let report = load_contract_source(
        "invalid-create-policy.yaml",
        r"
openapi: 3.1.0
info: { title: Resource API, version: 1.0.0 }
paths:
  /orders:
    post:
      operationId: placeOrder
      x-minco-resource:
        name: order
        action: create
      responses:
        '201':
          description: Created
          headers:
            ETag:
              schema: { type: string }
          content:
            application/json:
              schema:
                type: object
                additionalProperties: false
                required: [data]
                properties:
                  data:
                    type: object
                    additionalProperties: false
        default:
          description: Problem
          content:
            application/problem+json:
              schema:
                type: object
                additionalProperties: false
",
    )
    .expect("contract parses");

    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-027")
    );
}

#[test]
fn resource_create_replays_keep_the_same_document_headers() {
    let report = load_contract_source(
        "invalid-create-replay.yaml",
        r"
openapi: 3.1.0
info: { title: Resource API, version: 1.0.0 }
paths:
  /orders:
    post:
      operationId: placeOrder
      x-minco-idempotent: true
      x-minco-resource: { name: order, action: create }
      parameters:
        - in: header
          name: Idempotency-Key
          required: true
          schema: { type: string }
      responses:
        '201':
          description: Created
          headers:
            ETag: { schema: { type: string } }
            Location: { schema: { type: string } }
          content:
            application/json:
              schema:
                type: object
                additionalProperties: false
                required: [data]
                properties:
                  data: { type: string }
        '200':
          description: Replayed without the standard document or headers
        default:
          description: Problem
          content:
            application/problem+json:
              schema: { type: object, additionalProperties: false }
",
    )
    .expect("contract parses");

    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-025")
    );
}

#[test]
fn resource_families_reject_duplicate_actions() {
    let report = load_contract_source(
        "duplicate-resource-actions.yaml",
        r"
openapi: 3.1.0
info: { title: Resource API, version: 1.0.0 }
paths:
  /orders/{orderId}:
    get:
      operationId: getOrder
      x-minco-resource: { name: order, action: read }
      responses:
        '200':
          description: Found
          headers:
            ETag: { schema: { type: string } }
          content:
            application/json:
              schema:
                type: object
                additionalProperties: false
                required: [data]
                properties:
                  data:
                    type: object
                    additionalProperties: false
        default:
          description: Problem
          content:
            application/problem+json:
              schema: { type: object, additionalProperties: false }
  /archived-orders/{orderId}:
    get:
      operationId: getArchivedOrder
      x-minco-resource: { name: order, action: read }
      responses:
        '200':
          description: Found
          headers:
            ETag: { schema: { type: string } }
          content:
            application/json:
              schema:
                type: object
                additionalProperties: false
                required: [data]
                properties:
                  data:
                    type: object
                    additionalProperties: false
        default:
          description: Problem
          content:
            application/problem+json:
              schema: { type: object, additionalProperties: false }
",
    )
    .expect("contract parses");

    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-028")
    );
}

#[test]
fn resource_delete_success_is_empty_204() {
    let report = load_contract_source(
        "invalid-delete-success.yaml",
        r"
openapi: 3.1.0
info: { title: Resource API, version: 1.0.0 }
paths:
  /orders/{orderId}:
    delete:
      operationId: deleteOrder
      x-minco-resource: { name: order, action: delete }
      parameters:
        - in: header
          name: If-Match
          required: true
          schema: { type: string }
      responses:
        '204':
          description: Deleted
          content:
            application/json:
              schema: { type: object, additionalProperties: false }
        '412':
          description: Stale
          content:
            application/problem+json:
              schema: { type: object, additionalProperties: false }
        '428':
          description: Required
          content:
            application/problem+json:
              schema: { type: object, additionalProperties: false }
",
    )
    .expect("contract parses");

    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "MINCO-CONTRACT-025")
    );
}
