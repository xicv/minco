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
"#;

    let catalog = DeploymentTargetCatalog::from_toml(source).expect("target catalog");
    let selected = catalog.select(None).expect("default target");
    assert_eq!(selected.environment, "dev");
    assert!(!selected.target.enabled);
    assert_eq!(selected.target.expected_region, "ap-southeast-2");

    let output = serde_json::to_string(&selected).expect("serialize target");
    assert!(output.contains("database_url_parameter_name"));
    assert!(!output.contains("password"));
    assert!(!output.contains("secret_value"));

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
}
