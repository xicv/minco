use minco_deploy_aws::DeploymentTargetCatalog;

#[test]
fn reviewed_target_catalog_is_strict_and_contains_names_not_secret_values() {
    let source = r#"
schema_version = 1
default_environment = "dev"

[environments.dev]
enabled = false
expected_account_id = "000000000000"
expected_region = "ap-southeast-2"
expected_role_arn = "arn:aws:iam::000000000000:role/minco-dev"
stack_name = "minco-dev"
artifact_bucket = "minco-dev-artifacts-placeholder"
database_url_parameter_name = "/minco/dev/database-url"
static_site_certificate_arn = "arn:aws:acm:us-east-1:000000000000:certificate/example"
static_site_hosted_zone_id = "Z1234567890ABC"
static_site_pricing_checked_on = "2026-08-02"
static_site_pricing_source = "https://aws.amazon.com/cloudfront/pricing/"
static_site_billing_model = "request_and_transfer"
static_site_flat_rate_eligibility = "ineligible"
stack_tags = { "minco:managed" = "true", "minco:purpose" = "bounded-smoke", "minco:run-id" = "run-123" }
"#;

    let catalog = DeploymentTargetCatalog::from_toml(source).expect("target catalog");
    let selected = catalog.select(None).expect("default target");
    assert_eq!(selected.environment, "dev");
    assert!(!selected.target.enabled);
    assert_eq!(selected.target.expected_region, "ap-southeast-2");
    assert_eq!(
        selected.target.stack_tags.get("minco:run-id"),
        Some(&"run-123".to_owned())
    );

    let output = serde_json::to_string(&selected).expect("serialize target");
    assert!(output.contains("database_url_parameter_name"));
    assert!(!output.contains("password"));
    assert!(!output.contains("secret_value"));

    let wrong_static_certificate = source.replace(
        "arn:aws:acm:us-east-1:000000000000:certificate/example",
        "arn:aws:acm:ap-southeast-2:000000000000:certificate/example",
    );
    assert!(DeploymentTargetCatalog::from_toml(&wrong_static_certificate).is_err());

    let partial_pricing = source.replace(
        "static_site_pricing_source = \"https://aws.amazon.com/cloudfront/pricing/\"",
        "",
    );
    assert!(DeploymentTargetCatalog::from_toml(&partial_pricing).is_err());

    let with_secret_value = source.replace(
        "database_url_parameter_name = \"/minco/dev/database-url\"",
        "database_url_parameter_name = \"/minco/dev/database-url\"\nsecret_value = \"must-reject\"",
    );
    assert!(DeploymentTargetCatalog::from_toml(&with_secret_value).is_err());

    let invalid_network = source.replace(
        "database_url_parameter_name = \"/minco/dev/database-url\"",
        "database_url_parameter_name = \"/minco/dev/database-url\"\nlambda_subnet_ids = [\"not-a-subnet\"]\nlambda_security_group_ids = [\"sg-good123\"]",
    );
    assert!(DeploymentTargetCatalog::from_toml(&invalid_network).is_err());

    let wrong_kms_target = source.replace(
        "database_url_parameter_name = \"/minco/dev/database-url\"",
        "database_url_parameter_name = \"/minco/dev/database-url\"\ndatabase_kms_key_arn = \"arn:aws:kms:us-east-1:999900001111:key/not-reviewed\"",
    );
    assert!(DeploymentTargetCatalog::from_toml(&wrong_kms_target).is_err());

    let reserved_release_tag = source.replace(
        "\"minco:run-id\" = \"run-123\"",
        "\"MincoReleaseDigest\" = \"operator-controlled\"",
    );
    assert!(DeploymentTargetCatalog::from_toml(&reserved_release_tag).is_err());

    let aws_reserved_tag = source.replace(
        "\"minco:run-id\" = \"run-123\"",
        "\"aws:operator\" = \"run-123\"",
    );
    assert!(DeploymentTargetCatalog::from_toml(&aws_reserved_tag).is_err());
}

#[test]
fn deployment_target_stack_tags_enforce_provider_limits() {
    let source = r#"
schema_version = 1
default_environment = "dev"

[environments.dev]
enabled = false
expected_account_id = "000000000000"
expected_region = "ap-southeast-2"
expected_role_arn = "arn:aws:iam::000000000000:role/minco-dev"
stack_name = "minco-dev"
artifact_bucket = "minco-dev-artifacts-placeholder"
database_url_parameter_name = "/minco/dev/database-url"
stack_tags = { "minco:run-id" = "run-123" }
"#;

    for invalid in [
        source.replace("\"minco:run-id\"", "\"\""),
        source.replace("\"minco:run-id\"", &format!("\"{}\"", "k".repeat(129))),
        source.replace("\"run-123\"", &format!("\"{}\"", "v".repeat(257))),
        source.replace("\"run-123\"", "\"run\\n123\""),
    ] {
        assert!(DeploymentTargetCatalog::from_toml(&invalid).is_err());
    }

    let too_many = (0..48)
        .map(|index| format!("\"tag-{index}\" = \"value\""))
        .collect::<Vec<_>>()
        .join(", ");
    let too_many = source.replace(
        "stack_tags = { \"minco:run-id\" = \"run-123\" }",
        &format!("stack_tags = {{ {too_many} }}"),
    );
    assert!(DeploymentTargetCatalog::from_toml(&too_many).is_err());
}
