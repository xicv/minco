use minco_deploy_aws::{
    ChangeSetReceipt, ChangeSetReceiptInput, CloudFormationChangeSet, StackDrift,
};
use minco_release::{FileDigest, ReleaseEnvironment};
use tempfile::tempdir;

fn file(path: &str, digest_byte: char) -> FileDigest {
    FileDigest {
        path: path.into(),
        sha256: digest_byte.to_string().repeat(64),
        bytes: 123,
    }
}

fn provider_change_set() -> CloudFormationChangeSet {
    CloudFormationChangeSet::from_aws_json(
        br#"
        {
          "ChangeSetName": "minco-orders-reviewed",
          "ChangeSetId": "arn:aws:cloudformation:ap-southeast-2:111122223333:changeSet/minco-orders-reviewed/abc",
          "StackId": "arn:aws:cloudformation:ap-southeast-2:111122223333:stack/minco-orders/def",
          "StackName": "minco-orders",
          "ChangeSetType": "CREATE",
          "Status": "CREATE_COMPLETE",
          "ExecutionStatus": "AVAILABLE",
          "Changes": [{
            "Type": "Resource",
            "ResourceChange": {
              "Action": "Add",
              "LogicalResourceId": "OrdersApi",
              "ResourceType": "AWS::ApiGatewayV2::Api"
            }
          }]
        }
        "#,
        minco_deploy_aws::ChangeSetType::Create,
    )
    .expect("provider change set")
}

#[test]
fn exact_change_set_review_receipt_is_digest_sealed_and_immutable() {
    let release_digest = "b".repeat(64);
    let input = ChangeSetReceiptInput {
        source_change: "d".repeat(64),
        release_manifest: file("target/minco/release.json", 'a'),
        release_id: format!("minco.{}", &release_digest[..24]),
        release_digest: release_digest.clone(),
        release_approval_digest: release_digest,
        configuration_digest: "f".repeat(64),
        environment: ReleaseEnvironment {
            application: "minco-orders".into(),
            environment: "dev".into(),
            region: "ap-southeast-2".into(),
        },
        expected_account_id: "111122223333".into(),
        expected_role_arn: "arn:aws:iam::111122223333:role/minco-dev".into(),
        target_config: file("infra/aws/deployment-targets.toml", 'c'),
        packaged_template: file("target/minco/packaged-template.yaml", 'e'),
        drift: StackDrift::NotApplicableNewStack,
        change_set: provider_change_set(),
    };
    let receipt = ChangeSetReceipt::seal(input.clone()).expect("seal receipt");
    receipt.verify_structure().expect("verify receipt");

    let mut invalid_arn = input;
    invalid_arn.change_set.change_set_id = invalid_arn.change_set.stack_id.clone();
    assert!(ChangeSetReceipt::seal(invalid_arn).is_err());

    let mut tampered = serde_json::to_value(&receipt).expect("receipt JSON");
    tampered["change_set"]["stack_name"] = serde_json::json!("other-stack");
    assert!(
        ChangeSetReceipt::from_json(&serde_json::to_vec(&tampered).expect("tampered receipt JSON"))
            .is_err()
    );

    let mut extended = serde_json::to_value(&receipt).expect("receipt JSON");
    extended["unreviewed_metadata"] = serde_json::json!({"approved": true});
    assert!(
        ChangeSetReceipt::from_json(&serde_json::to_vec(&extended).expect("extended receipt JSON"))
            .is_err()
    );
    let mut nested_extension = serde_json::to_value(&receipt).expect("receipt JSON");
    nested_extension["change_set"]["review"]["additions"][0]["unreviewed_metadata"] =
        serde_json::json!("not digest bound");
    assert!(
        ChangeSetReceipt::from_json(
            &serde_json::to_vec(&nested_extension).expect("nested extended receipt JSON")
        )
        .is_err()
    );

    let directory = tempdir().expect("temporary receipt directory");
    let path = directory.path().join("change-set.json");
    receipt.write_json(&path).expect("write receipt");
    receipt.write_json(&path).expect("idempotent receipt write");

    let mut other = receipt;
    other.change_set.change_set_name = "different-review".into();
    assert!(other.write_json(&path).is_err());
}
