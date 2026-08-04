use minco_aws_adapters::{
    iam::{AwsRuntimeResources, runtime_iam_policy},
    static_site::{StaticSiteInfrastructure, render_cloudformation},
};
use minco_core::ApplicationGraph;
use minco_plugin_static_site::StaticSitePlan;
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let policy_path = required_path("MINCO_AWS_POLICY_PATH")?;
    let template_path = required_path("MINCO_AWS_TEMPLATE_PATH")?;
    let mut graph = ApplicationGraph::default();
    for capability in [
        "aws.s3.object-storage",
        "aws.sqs.event-publication",
        "aws.cognito.identity-administration",
        "aws.cloudfront.static-site",
    ] {
        graph
            .capabilities
            .insert(capability.into(), "1.0.0".parse()?);
    }
    let ses_identity_arn = env::var("MINCO_AWS_SES_IDENTITY_ARN")
        .ok()
        .filter(|value| !value.is_empty());
    if ses_identity_arn.is_some() {
        graph
            .capabilities
            .insert("aws.ses.email-notifications".into(), "1.0.0".parse()?);
    }
    let resources = AwsRuntimeResources {
        object_bucket_arn: Some(required("MINCO_AWS_BUCKET_ARN")?),
        object_key_prefix: "objects".into(),
        queue_arn: Some(required("MINCO_AWS_QUEUE_ARN")?),
        ses_identity_arn,
        cognito_user_pool_arn: Some(required("MINCO_AWS_USER_POOL_ARN")?),
        static_site_bucket_arn: Some(required("MINCO_AWS_BUCKET_ARN")?),
        static_site_key_prefix: "site".into(),
        cloudfront_distribution_arn: Some(required("MINCO_AWS_CLOUDFRONT_DISTRIBUTION_ARN")?),
        appsync_channel_namespace_arn: None,
    };
    fs::write(
        policy_path,
        serde_json::to_vec_pretty(&runtime_iam_policy(&graph, &resources)?)?,
    )?;

    let custom_domain = optional("MINCO_AWS_STATIC_SITE_DOMAIN");
    let manage_dns_alias = custom_domain.is_some();
    let plan = StaticSitePlan {
        source_directory: "dist".into(),
        index_document: "index.html".into(),
        spa_fallback: true,
        immutable_cache_seconds: 31_536_000,
        html_cache_seconds: 0,
        price_class: "PriceClass_100".into(),
        ipv6_enabled: true,
        custom_domain,
        manage_dns_alias,
    };
    fs::write(
        template_path,
        serde_json::to_vec_pretty(&render_cloudformation(
            &plan,
            &StaticSiteInfrastructure {
                bucket_name: None,
                certificate_arn: optional("MINCO_AWS_STATIC_SITE_CERTIFICATE_ARN"),
                hosted_zone_id: optional("MINCO_AWS_STATIC_SITE_HOSTED_ZONE_ID"),
            },
        )?)?,
    )?;
    Ok(())
}

fn required(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(env::var(name)?)
}

fn optional(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}

fn required_path(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(PathBuf::from(required(name)?))
}
