use minco_deploy_aws::{
    ChangeAction, ChangeScope, ChangeSetReview, ChangeSetStatus, ChangeSetType,
    CloudFormationChangeSet, ExecutionStatus, PromotionBoundaryError, PromotionOutcome,
    PromotionReceipt, PromotionReceiptError, PromotionReceiptInput, Replacement, ResourceChange,
    verify_promotion_boundary,
};
use minco_release::{FileDigest, ReleaseEnvironment};

fn change_set(changes: Vec<ResourceChange>) -> CloudFormationChangeSet {
    CloudFormationChangeSet {
        change_set_name: "promote-release".into(),
        change_set_id:
            "arn:aws:cloudformation:ap-southeast-2:111122223333:changeSet/promote-release/abc"
                .into(),
        stack_id: "arn:aws:cloudformation:ap-southeast-2:111122223333:stack/minco-orders/def"
            .into(),
        stack_name: "minco-orders".into(),
        change_set_type: ChangeSetType::Update,
        status: ChangeSetStatus::CreateComplete,
        execution_status: ExecutionStatus::Available,
        review: ChangeSetReview::classify(changes).expect("classify changes"),
    }
}

#[test]
fn promotion_refuses_any_non_routing_resource_change() {
    let changes = vec![
        ResourceChange::new(
            "LiveFunctionAlias",
            "AWS::Lambda::Alias",
            ChangeAction::Modify,
            Some(Replacement::Never),
        ),
        ResourceChange::new(
            "ApiFunction",
            "AWS::Lambda::Function",
            ChangeAction::Modify,
            Some(Replacement::Never),
        ),
    ];

    assert_eq!(
        verify_promotion_boundary(&change_set(changes), "minco-orders", "LiveFunctionAlias",),
        Err(PromotionBoundaryError::NonRoutingChange)
    );
}

#[test]
fn promotion_accepts_only_the_live_alias_property_update() {
    let mut live_alias = ResourceChange::new(
        "LiveFunctionAlias",
        "AWS::Lambda::Alias",
        ChangeAction::Modify,
        Some(Replacement::Never),
    );
    live_alias.scope = vec![ChangeScope::Properties];

    verify_promotion_boundary(
        &change_set(vec![live_alias]),
        "minco-orders",
        "LiveFunctionAlias",
    )
    .expect("exact live alias routing update");
}

#[test]
fn promotion_requires_provider_proof_of_a_property_scope_change() {
    let live_alias = ResourceChange::new(
        "LiveFunctionAlias",
        "AWS::Lambda::Alias",
        ChangeAction::Modify,
        Some(Replacement::Never),
    );

    assert_eq!(
        verify_promotion_boundary(
            &change_set(vec![live_alias]),
            "minco-orders",
            "LiveFunctionAlias",
        ),
        Err(PromotionBoundaryError::NonRoutingChange)
    );
}

#[test]
fn promotion_receipt_is_persisted_started_before_one_terminal_transition() {
    let hosted_verification = FileDigest {
        path: "target/minco/hosted-verification.json".into(),
        sha256: "b".repeat(64),
        bytes: 512,
    };
    let mut receipt = PromotionReceipt::start(PromotionReceiptInput {
        attempt_id: "018f1f9e-6d75-7a8b-9c0d-111122223333".into(),
        release_id: format!("minco.{}", "a".repeat(24)),
        release_digest: "a".repeat(64),
        environment: ReleaseEnvironment {
            application: "minco-orders".into(),
            environment: "staging".into(),
            region: "ap-southeast-2".into(),
        },
        deployment_receipt: FileDigest {
            path: "target/minco/deployment-receipt.json".into(),
            sha256: "c".repeat(64),
            bytes: 1_024,
        },
        hosted_verification: hosted_verification.clone(),
        operator_approval_digest: hosted_verification.sha256,
        stack_name: "minco-orders".into(),
        live_alias_logical_id: "LiveFunctionAlias".into(),
        previous_version: "41".into(),
        promoted_version: "42".into(),
        change_set: change_set(vec![{
            let mut alias = ResourceChange::new(
                "LiveFunctionAlias",
                "AWS::Lambda::Alias",
                ChangeAction::Modify,
                Some(Replacement::Never),
            );
            alias.scope = vec![ChangeScope::Properties];
            alias
        }]),
    })
    .expect("start promotion receipt");
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("promotion.json");

    receipt.write_json(&path).expect("persist started");
    assert_eq!(receipt.outcome(), PromotionOutcome::Started);
    receipt.succeed().expect("terminal success");
    receipt.write_json(&path).expect("persist success");
    assert!(matches!(
        receipt.fail("late_failure"),
        Err(PromotionReceiptError::Terminal {
            attempt_id,
            outcome: PromotionOutcome::Succeeded,
        }) if attempt_id == receipt.attempt_id
    ));
}

#[test]
fn initial_promotion_can_anchor_the_live_alias_to_a_numeric_version() {
    let hosted_verification = FileDigest {
        path: "target/minco/hosted-verification.json".into(),
        sha256: "b".repeat(64),
        bytes: 512,
    };
    let receipt = PromotionReceipt::start(PromotionReceiptInput {
        attempt_id: "018f1f9e-6d75-7a8b-9c0d-111122223333".into(),
        release_id: format!("minco.{}", "a".repeat(24)),
        release_digest: "a".repeat(64),
        environment: ReleaseEnvironment {
            application: "minco-orders".into(),
            environment: "staging".into(),
            region: "ap-southeast-2".into(),
        },
        deployment_receipt: FileDigest {
            path: "target/minco/deployment-receipt.json".into(),
            sha256: "c".repeat(64),
            bytes: 1_024,
        },
        hosted_verification: hosted_verification.clone(),
        operator_approval_digest: hosted_verification.sha256,
        stack_name: "minco-orders".into(),
        live_alias_logical_id: "LiveFunctionAlias".into(),
        previous_version: "candidate".into(),
        promoted_version: "42".into(),
        change_set: change_set(vec![{
            let mut alias = ResourceChange::new(
                "LiveFunctionAlias",
                "AWS::Lambda::Alias",
                ChangeAction::Modify,
                Some(Replacement::Never),
            );
            alias.scope = vec![ChangeScope::Properties];
            alias
        }]),
    })
    .expect("initial candidate-to-version promotion");

    assert_eq!(receipt.previous_version, "candidate");
    assert_eq!(receipt.promoted_version, "42");
}

#[test]
fn persisted_promotion_receipts_reject_unknown_nested_fields() {
    let hosted_verification = FileDigest {
        path: "target/minco/hosted-verification.json".into(),
        sha256: "b".repeat(64),
        bytes: 512,
    };
    let receipt = PromotionReceipt::start(PromotionReceiptInput {
        attempt_id: "018f1f9e-6d75-7a8b-9c0d-111122223333".into(),
        release_id: format!("minco.{}", "a".repeat(24)),
        release_digest: "a".repeat(64),
        environment: ReleaseEnvironment {
            application: "minco-orders".into(),
            environment: "staging".into(),
            region: "ap-southeast-2".into(),
        },
        deployment_receipt: FileDigest {
            path: "target/minco/deployment-receipt.json".into(),
            sha256: "c".repeat(64),
            bytes: 1_024,
        },
        hosted_verification: hosted_verification.clone(),
        operator_approval_digest: hosted_verification.sha256,
        stack_name: "minco-orders".into(),
        live_alias_logical_id: "LiveFunctionAlias".into(),
        previous_version: "41".into(),
        promoted_version: "42".into(),
        change_set: change_set(vec![{
            let mut alias = ResourceChange::new(
                "LiveFunctionAlias",
                "AWS::Lambda::Alias",
                ChangeAction::Modify,
                Some(Replacement::Never),
            );
            alias.scope = vec![ChangeScope::Properties];
            alias
        }]),
    })
    .expect("start promotion receipt");
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("promotion.json");
    let mut value = serde_json::to_value(receipt).expect("serialize receipt");
    value["deployment_receipt"]["authorization"] = serde_json::json!("secret");
    std::fs::write(
        &path,
        serde_json::to_vec(&value).expect("serialize mutation"),
    )
    .expect("write mutation");

    assert!(matches!(
        PromotionReceipt::read_json(&path),
        Err(PromotionReceiptError::Invalid(_))
    ));
}
