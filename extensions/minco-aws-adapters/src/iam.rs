use crate::AwsAdapterError;
use minco_core::ApplicationGraph;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AwsRuntimeResources {
    pub object_bucket_arn: Option<String>,
    #[serde(default)]
    pub object_key_prefix: String,
    pub queue_arn: Option<String>,
    pub ses_identity_arn: Option<String>,
    pub cognito_user_pool_arn: Option<String>,
    pub static_site_bucket_arn: Option<String>,
    #[serde(default)]
    pub static_site_key_prefix: String,
    pub cloudfront_distribution_arn: Option<String>,
    pub appsync_channel_namespace_arn: Option<String>,
}

/// Derives a least-privilege identity policy from the capabilities selected in
/// the validated application graph.
///
/// Required resource ARNs fail closed instead of widening to `"*"`.
pub fn runtime_iam_policy(
    graph: &ApplicationGraph,
    resources: &AwsRuntimeResources,
) -> Result<Value, AwsAdapterError> {
    let mut statements = Vec::new();

    if graph.capabilities.contains_key("aws.s3.object-storage") {
        let bucket = required_arn(
            resources.object_bucket_arn.as_deref(),
            "aws.s3.object-storage requires object_bucket_arn",
        )?;
        let prefix = safe_prefix(&resources.object_key_prefix)?;
        let object_resource = match prefix {
            Some(prefix) => format!("{bucket}/{prefix}/*"),
            None => format!("{bucket}/*"),
        };
        let mut list_statement = json!({
            "Sid": "MincoObjectStorageList",
            "Effect": "Allow",
            "Action": "s3:ListBucket",
            "Resource": bucket
        });
        if let Some(prefix) = prefix {
            list_statement["Condition"] = json!({
                "StringLike": {"s3:prefix": [prefix, format!("{prefix}/*")]}
            });
        }
        statements.push(list_statement);
        statements.push(json!({
            "Sid": "MincoObjectStorageObjects",
            "Effect": "Allow",
            "Action": [
                "s3:AbortMultipartUpload",
                "s3:DeleteObject",
                "s3:GetObject",
                "s3:PutObject"
            ],
            "Resource": object_resource
        }));
    }

    if graph.capabilities.contains_key("aws.sqs.event-publication") {
        statements.push(json!({
            "Sid": "MincoEventPublication",
            "Effect": "Allow",
            "Action": "sqs:SendMessage",
            "Resource": required_arn(
                resources.queue_arn.as_deref(),
                "aws.sqs.event-publication requires queue_arn",
            )?
        }));
    }

    if graph
        .capabilities
        .contains_key("aws.ses.email-notifications")
        || graph.capabilities.contains_key("aws.ses.mail-delivery")
    {
        statements.push(json!({
            "Sid": "MincoEmailDelivery",
            "Effect": "Allow",
            "Action": "ses:SendEmail",
            "Resource": required_arn(
                resources.ses_identity_arn.as_deref(),
                "SES delivery requires ses_identity_arn",
            )?
        }));
    }

    if graph
        .capabilities
        .contains_key("aws.cognito.identity-administration")
    {
        statements.push(json!({
            "Sid": "MincoIdentityAdministration",
            "Effect": "Allow",
            "Action": [
                "cognito-idp:AdminCreateUser",
                "cognito-idp:AdminDeleteUser",
                "cognito-idp:AdminDisableUser",
                "cognito-idp:AdminGetUser"
            ],
            "Resource": required_arn(
                resources.cognito_user_pool_arn.as_deref(),
                "identity.admin requires cognito_user_pool_arn",
            )?
        }));
    }

    if graph
        .capabilities
        .contains_key("aws.cloudfront.static-site")
    {
        let bucket = required_arn(
            resources.static_site_bucket_arn.as_deref(),
            "aws.cloudfront.static-site requires static_site_bucket_arn",
        )?;
        let prefix = safe_prefix(&resources.static_site_key_prefix)?;
        let objects = match prefix {
            Some(prefix) => format!("{bucket}/{prefix}/*"),
            None => format!("{bucket}/*"),
        };
        let mut list_statement = json!({
            "Sid": "MincoStaticSiteList",
            "Effect": "Allow",
            "Action": "s3:ListBucket",
            "Resource": bucket
        });
        if let Some(prefix) = prefix {
            list_statement["Condition"] = json!({
                "StringLike": {"s3:prefix": [format!("{prefix}/*")]}
            });
        }
        statements.push(list_statement);
        statements.push(json!({
            "Sid": "MincoStaticSiteObjects",
            "Effect": "Allow",
            "Action": ["s3:DeleteObject", "s3:PutObject"],
            "Resource": objects
        }));
        statements.push(json!({
            "Sid": "MincoStaticSiteInvalidation",
            "Effect": "Allow",
            "Action": "cloudfront:CreateInvalidation",
            "Resource": required_arn(
                resources.cloudfront_distribution_arn.as_deref(),
                "aws.cloudfront.static-site requires cloudfront_distribution_arn",
            )?
        }));
    }

    if graph
        .capabilities
        .contains_key("aws.appsync-events.realtime-publication")
    {
        let namespace = required_arn(
            resources.appsync_channel_namespace_arn.as_deref(),
            "aws.appsync-events.realtime-publication requires appsync_channel_namespace_arn",
        )?;
        statements.push(json!({
            "Sid": "MincoRealtimePublication",
            "Effect": "Allow",
            "Action": "appsync:EventPublish",
            "Resource": namespace
        }));
    }

    Ok(json!({
        "Version": "2012-10-17",
        "Statement": statements
    }))
}

fn safe_prefix(value: &str) -> Result<Option<&str>, AwsAdapterError> {
    let value = value.trim_matches('/');
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().any(char::is_control)
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(AwsAdapterError::InvalidConfiguration(
            "AWS object prefix must be a safe relative key prefix".into(),
        ));
    }
    Ok(Some(value))
}

fn required_arn<'a>(value: Option<&'a str>, context: &str) -> Result<&'a str, AwsAdapterError> {
    let value = value
        .filter(|value| value.starts_with("arn:"))
        .ok_or_else(|| {
            AwsAdapterError::InvalidConfiguration(format!(
                "{context}; wildcard IAM is not permitted"
            ))
        })?;
    if value.chars().any(char::is_control) {
        return Err(AwsAdapterError::InvalidConfiguration(format!(
            "{context}; ARN contains control characters"
        )));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_uses_exact_resources_for_selected_capabilities() {
        let mut graph = ApplicationGraph::default();
        for capability in [
            "aws.s3.object-storage",
            "aws.sqs.event-publication",
            "aws.ses.email-notifications",
            "aws.ses.mail-delivery",
            "aws.cognito.identity-administration",
            "aws.cloudfront.static-site",
            "aws.appsync-events.realtime-publication",
        ] {
            graph
                .capabilities
                .insert(capability.into(), "1.0.0".parse().unwrap());
        }
        let policy = runtime_iam_policy(
            &graph,
            &AwsRuntimeResources {
                object_bucket_arn: Some("arn:aws:s3:::minco-objects".into()),
                object_key_prefix: "feedback".into(),
                queue_arn: Some("arn:aws:sqs:ap-southeast-2:123456789012:minco-events".into()),
                ses_identity_arn: Some(
                    "arn:aws:ses:ap-southeast-2:123456789012:identity/example.com".into(),
                ),
                cognito_user_pool_arn: Some(
                    "arn:aws:cognito-idp:ap-southeast-2:123456789012:userpool/ap-southeast-2_test"
                        .into(),
                ),
                static_site_bucket_arn: Some("arn:aws:s3:::minco-static".into()),
                static_site_key_prefix: "site".into(),
                cloudfront_distribution_arn: Some(
                    "arn:aws:cloudfront::123456789012:distribution/EXAMPLE".into(),
                ),
                appsync_channel_namespace_arn: Some(
                    "arn:aws:appsync:ap-southeast-2:123456789012:apis/example/channelNamespace/orders"
                        .into(),
                ),
            },
        )
        .unwrap();
        let encoded = serde_json::to_string(&policy).unwrap();
        assert!(!encoded.contains("\"Resource\":\"*\""));
        assert!(encoded.contains("arn:aws:s3:::minco-objects/feedback/*"));
        assert!(encoded.contains("\"Sid\":\"MincoObjectStorageList\""));
        assert!(encoded.contains("\"s3:prefix\":[\"feedback\",\"feedback/*\"]"));
        assert!(encoded.contains("s3:AbortMultipartUpload"));
        assert!(encoded.contains("cognito-idp:AdminCreateUser"));
        assert!(encoded.contains("cloudfront:CreateInvalidation"));
        assert!(encoded.contains("arn:aws:s3:::minco-static/site/*"));
        assert!(encoded.contains("appsync:EventPublish"));
        assert!(encoded.contains(
            "arn:aws:appsync:ap-southeast-2:123456789012:apis/example/channelNamespace/orders"
        ));
    }

    #[test]
    fn selected_capability_without_an_arn_fails_closed() {
        let mut graph = ApplicationGraph::default();
        graph
            .capabilities
            .insert("aws.sqs.event-publication".into(), "1.0.0".parse().unwrap());
        assert!(matches!(
            runtime_iam_policy(&graph, &AwsRuntimeResources::default()),
            Err(AwsAdapterError::InvalidConfiguration(_))
        ));
    }
}
