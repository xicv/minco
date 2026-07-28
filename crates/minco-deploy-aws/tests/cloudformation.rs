use minco_deploy_aws::{ChangeAction, CloudFormationChangeSet, Replacement};

#[test]
fn provider_change_set_is_fully_classified_without_parameter_or_property_values() {
    let provider_json = r#"
    {
      "ChangeSetName": "minco-orders-reviewed",
      "ChangeSetId": "arn:aws:cloudformation:ap-southeast-2:111122223333:changeSet/minco-orders-reviewed/abc",
      "StackId": "arn:aws:cloudformation:ap-southeast-2:111122223333:stack/minco-orders/def",
      "StackName": "minco-orders",
      "ChangeSetType": "UPDATE",
      "Status": "CREATE_COMPLETE",
      "ExecutionStatus": "AVAILABLE",
      "Parameters": [
        {"ParameterKey": "Password", "ParameterValue": "must-never-escape"}
      ],
      "Changes": [
        {
          "Type": "Resource",
          "ResourceChange": {
            "Action": "Modify",
            "LogicalResourceId": "OrdersFunction",
            "PhysicalResourceId": "orders-api",
            "ResourceType": "AWS::Lambda::Function",
            "Replacement": "True",
            "Scope": ["Properties"],
            "Details": [{
              "Target": {
                "Attribute": "Properties",
                "Name": "Environment",
                "RequiresRecreation": "Never",
                "BeforeValue": "old-secret",
                "AfterValue": "new-secret"
              }
            }]
          }
        },
        {
          "Type": "Resource",
          "ResourceChange": {
            "Action": "Add",
            "LogicalResourceId": "OrdersApi",
            "ResourceType": "AWS::ApiGatewayV2::Api"
          }
        },
        {
          "Type": "Resource",
          "ResourceChange": {
            "Action": "Modify",
            "LogicalResourceId": "ExecutionRole",
            "ResourceType": "AWS::IAM::Role",
            "Replacement": "False"
          }
        },
        {
          "Type": "Resource",
          "ResourceChange": {
            "Action": "Remove",
            "LogicalResourceId": "LegacyLogGroup",
            "ResourceType": "AWS::Logs::LogGroup",
            "PolicyAction": "Delete"
          }
        },
        {
          "Type": "Resource",
          "ResourceChange": {
            "Action": "Import",
            "LogicalResourceId": "ImportedBucket",
            "ResourceType": "AWS::S3::Bucket"
          }
        },
        {
          "Type": "Resource",
          "ResourceChange": {
            "Action": "Dynamic",
            "LogicalResourceId": "IndeterminateResource",
            "ResourceType": "AWS::CloudFormation::CustomResource"
          }
        },
        {
          "Type": "Resource",
          "ResourceChange": {
            "Action": "SyncWithActual",
            "LogicalResourceId": "MetadataOnly",
            "ResourceType": "AWS::SSM::Parameter"
          }
        }
      ]
    }
    "#;

    let change_set = CloudFormationChangeSet::from_aws_json(provider_json.as_bytes())
        .expect("provider response");

    assert_eq!(change_set.review.additions[0].logical_id, "OrdersApi");
    assert_eq!(
        change_set.review.modifications[0].logical_id,
        "ExecutionRole"
    );
    assert_eq!(
        change_set.review.replacements[0].logical_id,
        "OrdersFunction"
    );
    assert_eq!(
        change_set.review.replacements[0].replacement,
        Some(Replacement::Always)
    );
    assert_eq!(change_set.review.deletions[0].logical_id, "LegacyLogGroup");
    assert_eq!(change_set.review.imports[0].action, ChangeAction::Import);
    assert_eq!(
        change_set.review.indeterminate[0].action,
        ChangeAction::Dynamic
    );
    assert_eq!(
        change_set.review.metadata_syncs[0].action,
        ChangeAction::SyncWithActual
    );

    let output = serde_json::to_string(&change_set).expect("serialize redacted review");
    for sensitive in [
        "must-never-escape",
        "old-secret",
        "new-secret",
        "ParameterValue",
        "BeforeValue",
        "AfterValue",
    ] {
        assert!(!output.contains(sensitive), "{sensitive} escaped");
    }
}
