use async_trait::async_trait;
use aws_sdk_cloudfront::types::{InvalidationBatch, Paths};
use aws_sdk_s3::{primitives::ByteStream, types::ServerSideEncryption};
use minco_plugin_static_site::{
    StaticSiteError, StaticSitePlan, StaticSitePublication, StaticSitePublisher,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};
use tokio::fs;
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct AwsStaticSitePublisher {
    s3: aws_sdk_s3::Client,
    cloudfront: aws_sdk_cloudfront::Client,
    bucket: String,
    key_prefix: String,
    distribution_id: Option<String>,
    public_url: String,
    allow_bucket_root: bool,
}

impl AwsStaticSitePublisher {
    pub fn new(
        s3: aws_sdk_s3::Client,
        cloudfront: aws_sdk_cloudfront::Client,
        bucket: impl Into<String>,
        key_prefix: impl Into<String>,
        distribution_id: Option<String>,
        public_url: impl Into<String>,
        allow_bucket_root: bool,
    ) -> Result<Self, StaticSiteError> {
        let bucket = bucket.into();
        let key_prefix = normalize_prefix(&key_prefix.into())?;
        let public_url = public_url.into().trim_end_matches('/').to_owned();
        if !crate::s3::valid_bucket_name(&bucket) || !valid_public_url(&public_url) {
            return Err(publish_error("AWS static-site destination is invalid"));
        }
        if key_prefix.is_empty() && !allow_bucket_root {
            return Err(publish_error(
                "publishing to an S3 bucket root requires allow_bucket_root=true",
            ));
        }
        if distribution_id
            .as_deref()
            .is_some_and(|value| value.trim().is_empty() || value.chars().any(char::is_control))
        {
            return Err(publish_error("CloudFront distribution ID is invalid"));
        }
        Ok(Self {
            s3,
            cloudfront,
            bucket,
            key_prefix,
            distribution_id,
            public_url,
            allow_bucket_root,
        })
    }

    fn provider_key(&self, relative: &str) -> String {
        if self.key_prefix.is_empty() {
            relative.to_owned()
        } else {
            format!("{}/{relative}", self.key_prefix)
        }
    }

    async fn existing_keys(&self) -> Result<BTreeSet<String>, StaticSiteError> {
        let list_prefix = if self.key_prefix.is_empty() {
            None
        } else {
            Some(format!("{}/", self.key_prefix))
        };
        let mut continuation = None;
        let mut keys = BTreeSet::new();
        loop {
            let mut request = self.s3.list_objects_v2().bucket(&self.bucket);
            if let Some(prefix) = &list_prefix {
                request = request.prefix(prefix);
            }
            if let Some(token) = continuation {
                request = request.continuation_token(token);
            }
            let output = request
                .send()
                .await
                .map_err(|error| publish_error(format!("S3 ListObjectsV2 failed: {error}")))?;
            for object in output.contents() {
                if let Some(key) = object.key()
                    && self.owns_key(key)
                {
                    keys.insert(key.to_owned());
                }
            }
            if !output.is_truncated().unwrap_or(false) {
                break;
            }
            continuation = Some(
                output
                    .next_continuation_token()
                    .ok_or_else(|| {
                        publish_error("S3 truncated a listing without a continuation token")
                    })?
                    .to_owned(),
            );
        }
        Ok(keys)
    }

    fn owns_key(&self, key: &str) -> bool {
        self.allow_bucket_root && self.key_prefix.is_empty()
            || (!self.key_prefix.is_empty()
                && key
                    .strip_prefix(&self.key_prefix)
                    .is_some_and(|suffix| suffix.starts_with('/') && suffix.len() > 1))
    }
}

#[async_trait]
impl StaticSitePublisher for AwsStaticSitePublisher {
    async fn publish(
        &self,
        plan: &StaticSitePlan,
        repository_root: &Path,
    ) -> Result<StaticSitePublication, StaticSiteError> {
        plan.validate()?;
        let files = publication_files(plan, repository_root).await?;
        if !files
            .iter()
            .any(|file| file.relative == plan.index_document)
        {
            return Err(publish_error(format!(
                "static-site index document {} is missing",
                plan.index_document
            )));
        }

        let mut expected = BTreeSet::new();
        for file in &files {
            let key = self.provider_key(&file.relative);
            expected.insert(key.clone());
            let body = ByteStream::from_path(&file.absolute)
                .await
                .map_err(|error| {
                    publish_error(format!(
                        "failed to read static-site file {}: {error}",
                        file.relative
                    ))
                })?;
            let content_type = mime_guess::from_path(&file.relative)
                .first_or_octet_stream()
                .essence_str()
                .to_owned();
            self.s3
                .put_object()
                .bucket(&self.bucket)
                .key(key)
                .body(body)
                .content_type(content_type)
                .cache_control(cache_control(plan, &file.relative))
                .server_side_encryption(ServerSideEncryption::Aes256)
                .send()
                .await
                .map_err(|error| publish_error(format!("S3 PutObject failed: {error}")))?;
        }

        let existing = self.existing_keys().await?;
        let stale = existing.difference(&expected).cloned().collect::<Vec<_>>();
        for key in &stale {
            if !self.owns_key(key) {
                return Err(publish_error(
                    "refused to delete an S3 object outside the owned static-site prefix",
                ));
            }
            self.s3
                .delete_object()
                .bucket(&self.bucket)
                .key(key)
                .send()
                .await
                .map_err(|error| publish_error(format!("S3 DeleteObject failed: {error}")))?;
        }

        let invalidation_id = match &self.distribution_id {
            Some(distribution_id) => {
                let paths = Paths::builder()
                    .quantity(1)
                    .items("/*")
                    .build()
                    .map_err(|error| publish_error(error.to_string()))?;
                let batch = InvalidationBatch::builder()
                    .paths(paths)
                    .caller_reference(format!("minco-{}", Uuid::new_v4()))
                    .build()
                    .map_err(|error| publish_error(error.to_string()))?;
                let output = self
                    .cloudfront
                    .create_invalidation()
                    .distribution_id(distribution_id)
                    .invalidation_batch(batch)
                    .send()
                    .await
                    .map_err(|error| {
                        publish_error(format!("CloudFront CreateInvalidation failed: {error}"))
                    })?;
                Some(
                    output
                        .invalidation()
                        .map(|value| value.id().to_owned())
                        .ok_or_else(|| {
                            publish_error("CloudFront CreateInvalidation returned no invalidation")
                        })?,
                )
            }
            None => None,
        };

        Ok(StaticSitePublication {
            url: self.public_url.clone(),
            uploaded: files.len(),
            removed: stale.len(),
            invalidation_id,
        })
    }
}

#[derive(Debug)]
struct PublicationFile {
    absolute: PathBuf,
    relative: String,
}

async fn publication_files(
    plan: &StaticSitePlan,
    repository_root: &Path,
) -> Result<Vec<PublicationFile>, StaticSiteError> {
    validate_relative(&plan.source_directory)?;
    validate_relative(&plan.index_document)?;
    let repository_root = fs::canonicalize(repository_root)
        .await
        .map_err(|error| publish_error(format!("repository root is unavailable: {error}")))?;
    let source = fs::canonicalize(repository_root.join(&plan.source_directory))
        .await
        .map_err(|error| publish_error(format!("static-site source is unavailable: {error}")))?;
    if !source.starts_with(&repository_root) {
        return Err(publish_error(
            "static-site source resolves outside the repository root",
        ));
    }
    let mut files = Vec::new();
    for entry in WalkDir::new(&source).follow_links(false) {
        let entry =
            entry.map_err(|error| publish_error(format!("static-site walk failed: {error}")))?;
        if entry.file_type().is_symlink() {
            return Err(publish_error(format!(
                "static-site source contains a symlink: {}",
                entry.path().display()
            )));
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let absolute = fs::canonicalize(entry.path())
            .await
            .map_err(|error| publish_error(format!("static-site file is unavailable: {error}")))?;
        if !absolute.starts_with(&source) {
            return Err(publish_error(
                "static-site file resolves outside the source directory",
            ));
        }
        let relative = absolute
            .strip_prefix(&source)
            .map_err(|_| publish_error("static-site path prefix changed during publication"))?;
        let relative = slash_path(relative)?;
        files.push(PublicationFile { absolute, relative });
    }
    files.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(files)
}

fn validate_relative(value: &str) -> Result<(), StaticSiteError> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || value.chars().any(char::is_control)
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(publish_error(
            "static-site path must be a safe relative path",
        ));
    }
    Ok(())
}

fn slash_path(path: &Path) -> Result<String, StaticSiteError> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(
                value
                    .to_str()
                    .ok_or_else(|| publish_error("static-site paths must be valid UTF-8"))?,
            ),
            _ => return Err(publish_error("static-site path is not relative")),
        }
    }
    if parts.is_empty() {
        return Err(publish_error("static-site file path is empty"));
    }
    Ok(parts.join("/"))
}

fn normalize_prefix(value: &str) -> Result<String, StaticSiteError> {
    let prefix = value.trim_matches('/');
    if prefix.chars().any(char::is_control)
        || (!prefix.is_empty()
            && prefix
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == ".."))
    {
        return Err(publish_error("static-site S3 prefix is invalid"));
    }
    Ok(prefix.to_owned())
}

fn cache_control(plan: &StaticSitePlan, relative: &str) -> String {
    if is_fingerprinted(relative) {
        format!("public,max-age={},immutable", plan.immutable_cache_seconds)
    } else {
        format!("public,max-age={},must-revalidate", plan.html_cache_seconds)
    }
}

fn is_fingerprinted(relative: &str) -> bool {
    relative
        .split(|character: char| !character.is_ascii_hexdigit())
        .any(|token| token.len() >= 8)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticSiteInfrastructure {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bucket_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate_arn: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hosted_zone_id: Option<String>,
}

pub fn render_cloudformation(
    plan: &StaticSitePlan,
    infrastructure: &StaticSiteInfrastructure,
) -> Result<Value, StaticSiteError> {
    plan.validate()?;
    let custom_domain = plan.custom_domain.as_deref();
    if custom_domain.is_some() && infrastructure.certificate_arn.is_none() {
        return Err(publish_error(
            "a custom domain requires an existing ACM certificate ARN from us-east-1",
        ));
    }
    if plan.manage_dns_alias && infrastructure.hosted_zone_id.is_none() {
        return Err(publish_error(
            "managed DNS requires an existing Route 53 hosted-zone ID",
        ));
    }
    if infrastructure
        .certificate_arn
        .as_deref()
        .is_some_and(|arn| !valid_cloudfront_certificate_arn(arn))
    {
        return Err(publish_error(
            "CloudFront ACM certificates must be in us-east-1",
        ));
    }

    let mut resources = serde_json::Map::new();
    let mut bucket_properties = json!({
        "BucketEncryption": {
            "ServerSideEncryptionConfiguration": [{
                "ServerSideEncryptionByDefault": {"SSEAlgorithm": "AES256"}
            }]
        },
        "OwnershipControls": {"Rules": [{"ObjectOwnership": "BucketOwnerEnforced"}]},
        "PublicAccessBlockConfiguration": {
            "BlockPublicAcls": true,
            "BlockPublicPolicy": true,
            "IgnorePublicAcls": true,
            "RestrictPublicBuckets": true
        }
    });
    if let Some(bucket_name) = &infrastructure.bucket_name {
        bucket_properties["BucketName"] = json!(bucket_name);
    }
    resources.insert(
        "StaticSiteBucket".into(),
        json!({
            "Type": "AWS::S3::Bucket",
            "DeletionPolicy": "Retain",
            "UpdateReplacePolicy": "Retain",
            "Properties": bucket_properties
        }),
    );
    resources.insert(
        "StaticSiteOriginAccessControl".into(),
        json!({
            "Type": "AWS::CloudFront::OriginAccessControl",
            "Properties": {
                "OriginAccessControlConfig": {
                    "Description": "Minco private static-site S3 origin",
                    "Name": {"Fn::Sub": "minco-static-${AWS::StackName}"},
                    "OriginAccessControlOriginType": "s3",
                    "SigningBehavior": "always",
                    "SigningProtocol": "sigv4"
                }
            }
        }),
    );
    let viewer_certificate = match (custom_domain, &infrastructure.certificate_arn) {
        (Some(_), Some(certificate_arn)) => json!({
            "AcmCertificateArn": certificate_arn,
            "MinimumProtocolVersion": "TLSv1.2_2021",
            "SslSupportMethod": "sni-only"
        }),
        _ => json!({"CloudFrontDefaultCertificate": true}),
    };
    let aliases = custom_domain.map_or_else(|| json!([]), |domain| json!([domain]));
    let custom_errors = if plan.spa_fallback {
        json!([
            {
                "ErrorCode": 403,
                "ResponseCode": 200,
                "ResponsePagePath": format!("/{}", plan.index_document),
                "ErrorCachingMinTTL": 0
            },
            {
                "ErrorCode": 404,
                "ResponseCode": 200,
                "ResponsePagePath": format!("/{}", plan.index_document),
                "ErrorCachingMinTTL": 0
            }
        ])
    } else {
        json!([])
    };
    resources.insert(
        "StaticSiteDistribution".into(),
        json!({
            "Type": "AWS::CloudFront::Distribution",
            "Properties": {
                "DistributionConfig": {
                    "Aliases": aliases,
                    "Comment": {"Fn::Sub": "Minco static site ${AWS::StackName}"},
                    "CustomErrorResponses": custom_errors,
                    "DefaultCacheBehavior": {
                        "AllowedMethods": ["GET", "HEAD", "OPTIONS"],
                        "CachedMethods": ["GET", "HEAD"],
                        "Compress": true,
                        "ForwardedValues": {
                            "Cookies": {"Forward": "none"},
                            "QueryString": false
                        },
                        "TargetOriginId": "StaticSiteS3Origin",
                        "ViewerProtocolPolicy": "redirect-to-https"
                    },
                    "DefaultRootObject": plan.index_document,
                    "Enabled": true,
                    "HttpVersion": "http2and3",
                    "IPV6Enabled": plan.ipv6_enabled,
                    "Origins": [{
                        "DomainName": {"Fn::GetAtt": ["StaticSiteBucket", "RegionalDomainName"]},
                        "Id": "StaticSiteS3Origin",
                        "OriginAccessControlId": {"Ref": "StaticSiteOriginAccessControl"},
                        "S3OriginConfig": {"OriginAccessIdentity": ""}
                    }],
                    "PriceClass": plan.price_class,
                    "ViewerCertificate": viewer_certificate
                }
            }
        }),
    );
    resources.insert(
        "StaticSiteBucketPolicy".into(),
        json!({
            "Type": "AWS::S3::BucketPolicy",
            "Properties": {
                "Bucket": {"Ref": "StaticSiteBucket"},
                "PolicyDocument": {
                    "Version": "2012-10-17",
                    "Statement": [{
                        "Sid": "AllowCloudFrontReadOnly",
                        "Effect": "Allow",
                        "Principal": {"Service": "cloudfront.amazonaws.com"},
                        "Action": "s3:GetObject",
                        "Resource": {"Fn::Sub": "${StaticSiteBucket.Arn}/*"},
                        "Condition": {
                            "StringEquals": {
                                "AWS:SourceArn": {
                                    "Fn::Sub": "arn:${AWS::Partition}:cloudfront::${AWS::AccountId}:distribution/${StaticSiteDistribution}"
                                }
                            }
                        }
                    }]
                }
            }
        }),
    );
    if plan.manage_dns_alias {
        resources.insert(
            "StaticSiteDnsAlias".into(),
            json!({
                "Type": "AWS::Route53::RecordSet",
                "Properties": {
                    "AliasTarget": {
                        "DNSName": {"Fn::GetAtt": ["StaticSiteDistribution", "DomainName"]},
                        "EvaluateTargetHealth": false,
                        "HostedZoneId": "Z2FDTNDATAQYW2"
                    },
                    "HostedZoneId": infrastructure.hosted_zone_id,
                    "Name": custom_domain,
                    "Type": "A"
                }
            }),
        );
        if plan.ipv6_enabled {
            resources.insert(
                "StaticSiteDnsIpv6Alias".into(),
                json!({
                    "Type": "AWS::Route53::RecordSet",
                    "Properties": {
                        "AliasTarget": {
                            "DNSName": {"Fn::GetAtt": ["StaticSiteDistribution", "DomainName"]},
                            "EvaluateTargetHealth": false,
                            "HostedZoneId": "Z2FDTNDATAQYW2"
                        },
                        "HostedZoneId": infrastructure.hosted_zone_id,
                        "Name": custom_domain,
                        "Type": "AAAA"
                    }
                }),
            );
        }
    }

    Ok(json!({
        "AWSTemplateFormatVersion": "2010-09-09",
        "Description": "Minco private S3 and CloudFront OAC static-site profile",
        "Resources": resources,
        "Outputs": {
            "BucketName": {"Value": {"Ref": "StaticSiteBucket"}},
            "DistributionId": {"Value": {"Ref": "StaticSiteDistribution"}},
            "DistributionDomainName": {
                "Value": {"Fn::GetAtt": ["StaticSiteDistribution", "DomainName"]}
            }
        }
    }))
}

fn valid_public_url(value: &str) -> bool {
    crate::validated_service_uri(value).is_some()
}

fn valid_cloudfront_certificate_arn(value: &str) -> bool {
    let parts = value.splitn(6, ':').collect::<Vec<_>>();
    parts.len() == 6
        && parts[0] == "arn"
        && !parts[1].is_empty()
        && parts[2] == "acm"
        && parts[3] == "us-east-1"
        && parts[4].len() == 12
        && parts[4].bytes().all(|byte| byte.is_ascii_digit())
        && parts[5].starts_with("certificate/")
        && parts[5].len() > "certificate/".len()
        && !value.chars().any(char::is_control)
}

fn publish_error(message: impl Into<String>) -> StaticSiteError {
    StaticSiteError::Publish(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> StaticSitePlan {
        StaticSitePlan {
            source_directory: "dist".into(),
            index_document: "index.html".into(),
            spa_fallback: true,
            immutable_cache_seconds: 31_536_000,
            html_cache_seconds: 0,
            price_class: "PriceClass_100".into(),
            ipv6_enabled: true,
            custom_domain: None,
            manage_dns_alias: false,
        }
    }

    #[test]
    fn immutable_caching_requires_a_hash_token() {
        assert!(is_fingerprinted("assets/app.0123abcd.js"));
        assert!(!is_fingerprinted("assets/application.js"));
        assert_eq!(
            cache_control(&plan(), "assets/application.js"),
            "public,max-age=0,must-revalidate"
        );
    }

    #[test]
    fn cloudformation_uses_private_s3_oac_and_exact_distribution_source() {
        let template = render_cloudformation(
            &plan(),
            &StaticSiteInfrastructure {
                bucket_name: None,
                certificate_arn: None,
                hosted_zone_id: None,
            },
        )
        .unwrap();
        assert_eq!(
            template["Resources"]["StaticSiteOriginAccessControl"]["Properties"]["OriginAccessControlConfig"]
                ["SigningBehavior"],
            "always"
        );
        assert_eq!(
            template["Resources"]["StaticSiteBucket"]["Properties"]["PublicAccessBlockConfiguration"]
                ["RestrictPublicBuckets"],
            true
        );
        assert!(
            template["Resources"]["StaticSiteBucketPolicy"]
                .to_string()
                .contains("AWS:SourceArn")
        );
        assert!(!template.to_string().contains("WebsiteConfiguration"));
    }

    #[test]
    fn custom_domain_fails_closed_without_us_east_1_certificate() {
        let mut custom = plan();
        custom.custom_domain = Some("app.example.test".into());
        assert!(
            render_cloudformation(
                &custom,
                &StaticSiteInfrastructure {
                    bucket_name: None,
                    certificate_arn: None,
                    hosted_zone_id: None,
                },
            )
            .is_err()
        );
    }

    #[test]
    fn managed_dns_matches_the_cloudfront_ipv6_setting() {
        let mut custom = plan();
        custom.custom_domain = Some("app.example.test".into());
        custom.manage_dns_alias = true;
        let infrastructure = StaticSiteInfrastructure {
            bucket_name: None,
            certificate_arn: Some("arn:aws:acm:us-east-1:123456789012:certificate/example".into()),
            hosted_zone_id: Some("Z123456789".into()),
        };

        let dual_stack = render_cloudformation(&custom, &infrastructure).unwrap();
        assert_eq!(
            dual_stack["Resources"]["StaticSiteDnsAlias"]["Properties"]["Type"],
            "A"
        );
        assert_eq!(
            dual_stack["Resources"]["StaticSiteDnsIpv6Alias"]["Properties"]["Type"],
            "AAAA"
        );

        custom.ipv6_enabled = false;
        let ipv4_only = render_cloudformation(&custom, &infrastructure).unwrap();
        assert!(
            ipv4_only["Resources"]
                .get("StaticSiteDnsIpv6Alias")
                .is_none()
        );
    }

    #[test]
    fn destination_validation_rejects_lookalike_local_urls_and_reserved_buckets() {
        assert!(!valid_public_url("http://localhost.attacker.example"));
        assert!(valid_public_url("http://localhost:4173/site"));
        assert!(!valid_public_url("https://user@example.com/site"));
        assert!(!valid_public_url("https://example.com/site?token=value"));
        assert!(!crate::s3::valid_bucket_name("192.0.2.1"));
    }
}
