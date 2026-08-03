use minco_deploy_aws::{
    RollbackAssessmentInput, RollbackClassification, RollbackCompatibility,
    assess_rollback_compatibility,
};
use std::collections::BTreeMap;

fn input() -> RollbackAssessmentInput {
    RollbackAssessmentInput {
        current_release_id: "minco.current".into(),
        target_release_id: "minco.previous".into(),
        current_environment: "production".into(),
        target_environment: "production".into(),
        contract: RollbackCompatibility::Compatible,
        current_configuration_digest: "a".repeat(64),
        target_configuration_digest: "a".repeat(64),
        current_deployment_plan_digest: "b".repeat(64),
        target_deployment_plan_digest: "b".repeat(64),
        current_migration_catalog_digest: "c".repeat(64),
        target_migration_catalog_digest: "c".repeat(64),
        current_migration_plan_bindings_digest: "1".repeat(64),
        target_migration_plan_bindings_digest: "1".repeat(64),
        current_seed_catalog_digest: "d".repeat(64),
        target_seed_catalog_digest: "d".repeat(64),
        current_seed_plan_bindings_digest: "2".repeat(64),
        target_seed_plan_bindings_digest: "2".repeat(64),
        data_compatibility: RollbackCompatibility::Compatible,
        data_compatibility_evidence_digest: Some("e".repeat(64)),
        current_api_version: "12".into(),
        target_api_version: "9".into(),
        current_worker_artifacts: BTreeMap::from([("emails".into(), "f".repeat(64))]),
        target_worker_artifacts: BTreeMap::from([("emails".into(), "f".repeat(64))]),
    }
}

#[test]
fn rollback_is_compatible_only_when_every_boundary_is_proved() {
    let report = assess_rollback_compatibility(input()).unwrap();

    assert_eq!(report.classification, RollbackClassification::Compatible);
    assert_eq!(
        report.api_routing,
        "redeploy_target_artifact_as_candidate_then_route_new_verified_version"
    );
    assert_eq!(
        report.worker_routing,
        "preserve_current_worker_event_sources"
    );
    assert!(report.checks.iter().all(|check| {
        check.classification == RollbackClassification::Compatible
            && !check.code.is_empty()
            && !check.reason.is_empty()
    }));
    assert!(report.limitations.iter().any(|limitation| limitation
        == "Minco never invents reverse SQL or automatically repairs persisted data."));
}

#[test]
fn missing_data_proof_and_changed_workers_require_an_operator_decision() {
    let mut evidence = input();
    evidence.data_compatibility = RollbackCompatibility::OperatorDecisionRequired;
    evidence.data_compatibility_evidence_digest = None;
    evidence
        .target_worker_artifacts
        .insert("emails".into(), "0".repeat(64));

    let report = assess_rollback_compatibility(evidence).unwrap();

    assert_eq!(
        report.classification,
        RollbackClassification::OperatorDecisionRequired
    );
    assert!(report.checks.iter().any(|check| {
        check.code == "data.compatibility_unproved"
            && check.classification == RollbackClassification::OperatorDecisionRequired
    }));
    assert!(report.checks.iter().any(|check| {
        check.code == "workers.artifacts_changed"
            && check.reason.contains("remain on the current version")
    }));
}

#[test]
fn incompatible_data_evidence_blocks_rollback() {
    let mut evidence = input();
    evidence.data_compatibility = RollbackCompatibility::Incompatible;

    let report = assess_rollback_compatibility(evidence).unwrap();

    assert_eq!(report.classification, RollbackClassification::Incompatible);
    assert!(report.checks.iter().any(|check| {
        check.code == "data.incompatible"
            && check.classification == RollbackClassification::Incompatible
    }));
}

#[test]
fn incompatible_contract_or_environment_blocks_rollback() {
    let mut evidence = input();
    evidence.contract = RollbackCompatibility::Incompatible;
    evidence.target_environment = "staging".into();

    let report = assess_rollback_compatibility(evidence).unwrap();

    assert_eq!(report.classification, RollbackClassification::Incompatible);
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.code == "contract.breaking")
    );
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.code == "environment.mismatch")
    );
}

#[test]
fn rollback_target_must_be_an_older_published_version() {
    let mut evidence = input();
    evidence.target_api_version = "13".into();

    assert!(assess_rollback_compatibility(evidence).is_err());
}
