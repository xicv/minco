use aws_sdk_ssm::types::ParameterType;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test]
#[ignore = "requires Rustack on AWS_ENDPOINT_URL"]
async fn secure_parameter_round_trip_uses_standard_endpoint_override() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after the Unix epoch")
        .as_nanos();
    let name = format!("/minco/rustack/{}/{suffix}", std::process::id());
    let expected = "rustack-secure-parameter";

    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let client = aws_sdk_ssm::Client::new(&config);
    client
        .put_parameter()
        .name(&name)
        .value(expected)
        .r#type(ParameterType::SecureString)
        .send()
        .await
        .expect("put the Rustack SSM parameter through the AWS SDK");

    let actual = minco_aws_lambda::load_secure_parameter(&name).await;
    let cleanup = client.delete_parameter().name(&name).send().await;

    cleanup.expect("delete the Rustack SSM parameter through the AWS SDK");
    assert_eq!(
        actual.expect("load the Rustack SSM parameter through the Minco adapter"),
        expected
    );
}
