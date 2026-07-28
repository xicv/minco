use minco_deploy_aws::{
    ChangeAction, ChangeSetReview, DriftState, EnvironmentExpectation, EnvironmentObservation,
    GuardFailureCode, MigrationState, Replacement, ResourceChange, SourceState, caller_role_arn,
    verify_guards,
};

#[test]
fn change_set_actions_are_classified_deterministically() {
    let review = ChangeSetReview::classify([
        ResourceChange::new(
            "OrdersFunction",
            "AWS::Lambda::Function",
            ChangeAction::Modify,
            Some(Replacement::Conditional),
        ),
        ResourceChange::new(
            "OrdersApi",
            "AWS::ApiGatewayV2::Api",
            ChangeAction::Add,
            None,
        ),
        ResourceChange::new(
            "LegacyLogGroup",
            "AWS::Logs::LogGroup",
            ChangeAction::Remove,
            None,
        ),
        ResourceChange::new(
            "ExecutionRole",
            "AWS::IAM::Role",
            ChangeAction::Modify,
            Some(Replacement::Never),
        ),
    ])
    .expect("valid change-set review");

    assert_eq!(review.additions[0].logical_id, "OrdersApi");
    assert_eq!(review.modifications[0].logical_id, "ExecutionRole");
    assert_eq!(review.replacements[0].logical_id, "OrdersFunction");
    assert_eq!(review.deletions[0].logical_id, "LegacyLogGroup");
}

#[test]
fn every_unproved_environment_guard_fails_closed_without_sensitive_values() {
    let expected_release_digest = "a".repeat(64);
    let observed_release_digest = "d".repeat(64);
    let expected = EnvironmentExpectation {
        account_id: "111122223333".into(),
        region: "ap-southeast-2".into(),
        environment: "production".into(),
        role_arn: "arn:aws:iam::111122223333:role/minco-production".into(),
        release_id: format!("minco.{}", &expected_release_digest[..24]),
        release_digest: expected_release_digest,
        configuration_digest: "b".repeat(64),
        migration_plan_digest: Some("c".repeat(64)),
    };
    let observed = EnvironmentObservation {
        account_id: "999900001111".into(),
        region: "us-east-1".into(),
        environment: "staging".into(),
        role_arn: "arn:aws:iam::999900001111:role/unreviewed".into(),
        release_id: format!("minco.{}", &observed_release_digest[..24]),
        release_digest: observed_release_digest,
        release_verified: false,
        configuration_digest: "e".repeat(64),
        drift: DriftState::Unknown,
        migration: MigrationState::Missing,
        source: SourceState::Dirty,
        operator_approval_digest: None,
    };

    let failure = verify_guards(&expected, &observed).expect_err("guards must fail closed");
    assert_eq!(
        failure.codes(),
        vec![
            GuardFailureCode::AccountMismatch,
            GuardFailureCode::RegionMismatch,
            GuardFailureCode::EnvironmentMismatch,
            GuardFailureCode::RoleMismatch,
            GuardFailureCode::ReleaseMismatch,
            GuardFailureCode::ReleaseUnverified,
            GuardFailureCode::ConfigurationMismatch,
            GuardFailureCode::DriftUnproved,
            GuardFailureCode::MigrationUnproved,
            GuardFailureCode::SourceDirty,
            GuardFailureCode::OperatorApprovalMissing,
        ]
    );

    let serialized = format!("{failure:?}");
    assert!(!serialized.contains("password"));
    assert!(!serialized.contains("token"));
}

#[test]
fn matching_but_malformed_environment_identity_never_passes_exact_guards() {
    let release_digest = "a".repeat(64);
    let expected = EnvironmentExpectation {
        account_id: "not-an-account".into(),
        region: "not a region".into(),
        environment: "../production".into(),
        role_arn: "arn:aws:iam::not-an-account:user/not-a-role".into(),
        release_id: format!("minco.{}", &release_digest[..24]),
        release_digest: release_digest.clone(),
        configuration_digest: "b".repeat(64),
        migration_plan_digest: Some("c".repeat(64)),
    };
    let observed = EnvironmentObservation {
        account_id: expected.account_id.clone(),
        region: expected.region.clone(),
        environment: expected.environment.clone(),
        role_arn: expected.role_arn.clone(),
        release_id: expected.release_id.clone(),
        release_digest: release_digest.clone(),
        release_verified: true,
        configuration_digest: expected.configuration_digest.clone(),
        drift: DriftState::Clean,
        migration: MigrationState::Verified {
            plan_digest: expected
                .migration_plan_digest
                .clone()
                .expect("migration digest"),
        },
        source: SourceState::Clean,
        operator_approval_digest: Some(release_digest),
    };

    let failure = verify_guards(&expected, &observed).expect_err("invalid identity must fail");
    assert_eq!(
        failure.codes(),
        vec![
            GuardFailureCode::AccountInvalid,
            GuardFailureCode::RegionInvalid,
            GuardFailureCode::EnvironmentInvalid,
            GuardFailureCode::RoleInvalid,
        ]
    );
}

#[test]
fn assumed_role_session_is_normalized_to_the_exact_reviewed_iam_role() {
    assert_eq!(
        caller_role_arn("arn:aws:sts::111122223333:assumed-role/minco-production/reviewed-session")
            .expect("assumed role"),
        "arn:aws:iam::111122223333:role/minco-production"
    );
    assert_eq!(
        caller_role_arn("arn:aws:iam::111122223333:role/minco-production").expect("direct role"),
        "arn:aws:iam::111122223333:role/minco-production"
    );
    for unapproved in [
        "arn:aws:iam::111122223333:user/operator",
        "arn:aws:iam::111122223333:root",
        "arn:aws:sts::111122223333:federated-user/operator",
    ] {
        assert!(caller_role_arn(unapproved).is_err(), "{unapproved}");
    }
}
