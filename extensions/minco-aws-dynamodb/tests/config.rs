use minco_aws_dynamodb::{DynamoDbConfig, DynamoDbProviderPlugin};
use minco_core::{IdleCostClass, Plugin, ResourceKind};

#[test]
fn configuration_accepts_only_bounded_names_regions_and_secure_endpoints() {
    assert!(
        DynamoDbConfig::new("orders-table", "ap-southeast-2", None)
            .validate()
            .is_ok()
    );
    assert!(
        DynamoDbConfig::new(
            "orders-table",
            "ap-southeast-2",
            Some("http://127.0.0.1:4566".into()),
        )
        .validate()
        .is_ok()
    );
    for endpoint in [
        "http://dynamodb.example.com",
        "https://user@example.com",
        "https://example.com/path?token=secret",
    ] {
        let error = DynamoDbConfig::new("orders-table", "ap-southeast-2", Some(endpoint.into()))
            .validate()
            .expect_err("unsafe endpoint must fail closed");
        assert!(!error.to_string().contains(endpoint));
        assert!(!error.to_string().contains("secret"));
    }
}

#[test]
fn configuration_debug_output_redacts_provider_identifiers() {
    let config = DynamoDbConfig::new(
        "private-orders-table",
        "ap-southeast-2",
        Some("http://127.0.0.1:4566".into()),
    );
    let debug = format!("{config:?}");
    assert!(!debug.contains("private-orders-table"));
    assert!(!debug.contains("127.0.0.1"));
    assert!(debug.contains("[REDACTED]"));
}

#[test]
fn provider_descriptor_is_explicit_request_based_and_has_no_business_repository() {
    let descriptor = DynamoDbProviderPlugin.descriptor();
    assert_eq!(descriptor.id.as_str(), "aws-dynamodb");
    assert!(
        descriptor
            .provides
            .iter()
            .any(|capability| capability.name == "aws.dynamodb.client")
    );
    assert_eq!(descriptor.resources.len(), 1);
    assert_eq!(descriptor.resources[0].kind, ResourceKind::DynamoDb);
    assert_eq!(
        descriptor.resources[0].idle_cost,
        IdleCostClass::StorageOnly
    );
    assert!(descriptor.resources[0].wake_sources.is_empty());
}
