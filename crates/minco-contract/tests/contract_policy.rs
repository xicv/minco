use minco_contract::load_contract;
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
fn official_feedback_contract_obeys_the_same_policy() {
    let contract = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins/minco-plugin-feedback/openapi/feedback.openapi.yaml");
    let report = load_contract(contract).expect("Feedback contract parses");
    assert!(report.is_valid(), "{:?}", report.findings);
}
