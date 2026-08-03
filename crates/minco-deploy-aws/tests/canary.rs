use minco_deploy_aws::{
    CanaryAlarmState, CanaryExecutionOutcome, CanaryExecutionReceipt, CanaryExecutionReceiptInput,
    CanaryObservation, CanaryOutcome, CanaryShiftInput, CanaryTargetPolicy, ChangeAction,
    ChangeScope, ChangeSetReview, ChangeSetStatus, ChangeSetType, CloudFormationChangeSet,
    ExecutionStatus, Replacement, ResourceChange, evaluate_canary_observation, plan_canary_shift,
};
use std::collections::BTreeMap;

fn policy() -> CanaryTargetPolicy {
    CanaryTargetPolicy {
        initial_traffic_percent: 10,
        monitoring_minutes: 15,
        alarm_arns: vec![
            "arn:aws:cloudwatch:ap-southeast-2:111122223333:alarm:minco-api-errors".into(),
        ],
        api_routing: "weighted_live_alias".into(),
        worker_routing: "preserve_current_event_sources".into(),
        provisioned_concurrency: false,
    }
}

fn canary_change_set() -> CloudFormationChangeSet {
    let mut alias = ResourceChange::new(
        "LiveFunctionAlias",
        "AWS::Lambda::Alias",
        ChangeAction::Modify,
        Some(Replacement::Never),
    );
    alias.scope = vec![ChangeScope::Properties];
    CloudFormationChangeSet {
        change_set_name: "canary".into(),
        stack_name: "minco-orders-production".into(),
        change_set_id: "arn:aws:cloudformation:ap-southeast-2:111122223333:changeSet/canary/id"
            .into(),
        stack_id:
            "arn:aws:cloudformation:ap-southeast-2:111122223333:stack/minco-orders-production/id"
                .into(),
        change_set_type: ChangeSetType::Update,
        status: ChangeSetStatus::CreateComplete,
        execution_status: ExecutionStatus::Available,
        review: ChangeSetReview::classify(vec![alias]).expect("classify canary alias change"),
    }
}

#[test]
fn canary_plan_is_explicit_api_only_and_cost_visible() {
    let plan = plan_canary_shift(CanaryShiftInput {
        policy: policy(),
        expected_account_id: "111122223333".into(),
        expected_region: "ap-southeast-2".into(),
        stack_name: "minco-orders-production".into(),
        function_name: "minco-orders-api".into(),
        alias_name: "live".into(),
        current_version: "12".into(),
        candidate_version: "13".into(),
        pre_traffic_verification_digest: "a".repeat(64),
    })
    .unwrap();

    assert_eq!(plan.candidate_weight_basis_points, 1_000);
    assert_eq!(plan.additional_resources, Vec::<String>::new());
    assert_eq!(plan.idle_compute_cost, "none");
    assert!(!plan.pricing_complete);
    assert_eq!(plan.worker_routing, "preserve_current_event_sources");
    assert!(
        plan.cost_notes
            .iter()
            .any(|note| note.contains("externally managed CloudWatch alarms"))
    );
}

#[test]
fn canary_plan_rejects_alarms_outside_the_bound_account_or_region() {
    let mut canary = policy();
    canary.alarm_arns =
        vec!["arn:aws:cloudwatch:us-east-1:111122223333:alarm:minco-api-errors".into()];
    assert!(
        plan_canary_shift(CanaryShiftInput {
            policy: canary,
            expected_account_id: "111122223333".into(),
            expected_region: "ap-southeast-2".into(),
            stack_name: "minco-orders-production".into(),
            function_name: "minco-orders-api".into(),
            alias_name: "live".into(),
            current_version: "12".into(),
            candidate_version: "13".into(),
            pre_traffic_verification_digest: "a".repeat(64),
        })
        .is_err()
    );
}

#[test]
fn alarms_or_missing_observations_reverse_the_shift() {
    let plan = plan_canary_shift(CanaryShiftInput {
        policy: policy(),
        expected_account_id: "111122223333".into(),
        expected_region: "ap-southeast-2".into(),
        stack_name: "minco-orders-production".into(),
        function_name: "minco-orders-api".into(),
        alias_name: "live".into(),
        current_version: "12".into(),
        candidate_version: "13".into(),
        pre_traffic_verification_digest: "a".repeat(64),
    })
    .unwrap();
    let alarm = plan.alarm_arns[0].clone();

    let alarmed = evaluate_canary_observation(
        &plan,
        CanaryObservation {
            elapsed_minutes: 3,
            alarm_states: BTreeMap::from([(alarm, CanaryAlarmState::Alarm)]),
            post_traffic_verification_digest: None,
        },
    );
    assert_eq!(
        alarmed,
        CanaryOutcome::Reverse {
            code: "alarm_entered_alarm_state".into()
        }
    );

    let missing = evaluate_canary_observation(
        &plan,
        CanaryObservation {
            elapsed_minutes: 3,
            alarm_states: BTreeMap::new(),
            post_traffic_verification_digest: None,
        },
    );
    assert_eq!(
        missing,
        CanaryOutcome::Reverse {
            code: "alarm_observation_missing".into()
        }
    );
}

#[test]
fn post_traffic_proof_is_required_after_the_monitoring_window() {
    let plan = plan_canary_shift(CanaryShiftInput {
        policy: policy(),
        expected_account_id: "111122223333".into(),
        expected_region: "ap-southeast-2".into(),
        stack_name: "minco-orders-production".into(),
        function_name: "minco-orders-api".into(),
        alias_name: "live".into(),
        current_version: "12".into(),
        candidate_version: "13".into(),
        pre_traffic_verification_digest: "a".repeat(64),
    })
    .unwrap();
    let ok = BTreeMap::from([(plan.alarm_arns[0].clone(), CanaryAlarmState::Ok)]);

    assert_eq!(
        evaluate_canary_observation(
            &plan,
            CanaryObservation {
                elapsed_minutes: 15,
                alarm_states: ok.clone(),
                post_traffic_verification_digest: None,
            },
        ),
        CanaryOutcome::Reverse {
            code: "post_traffic_verification_missing".into()
        }
    );
    assert_eq!(
        evaluate_canary_observation(
            &plan,
            CanaryObservation {
                elapsed_minutes: 15,
                alarm_states: ok,
                post_traffic_verification_digest: Some("b".repeat(64)),
            },
        ),
        CanaryOutcome::Complete
    );
}

#[test]
fn canary_receipt_is_written_started_before_one_terminal_transition() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("canary.json");
    let plan = plan_canary_shift(CanaryShiftInput {
        policy: policy(),
        expected_account_id: "111122223333".into(),
        expected_region: "ap-southeast-2".into(),
        stack_name: "minco-orders-production".into(),
        function_name: "minco-orders-api".into(),
        alias_name: "live".into(),
        current_version: "12".into(),
        candidate_version: "13".into(),
        pre_traffic_verification_digest: "a".repeat(64),
    })
    .unwrap();
    let mut receipt = CanaryExecutionReceipt::start(CanaryExecutionReceiptInput {
        attempt_id: "018f5f64-2712-7a65-9f4b-87c8f4af49a2".into(),
        plan,
        change_set: canary_change_set(),
    })
    .unwrap();

    receipt.write_json(&path).unwrap();
    assert_eq!(
        CanaryExecutionReceipt::read_json(&path).unwrap().outcome(),
        CanaryExecutionOutcome::Started
    );
    receipt.succeed(canary_change_set()).unwrap();
    receipt.write_json(&path).unwrap();
    assert_eq!(
        CanaryExecutionReceipt::read_json(&path).unwrap().outcome(),
        CanaryExecutionOutcome::Succeeded
    );
    assert!(receipt.reverse("too_late").is_err());
}
