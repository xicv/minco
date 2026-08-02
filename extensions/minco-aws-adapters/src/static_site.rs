use async_trait::async_trait;
use aws_sdk_cloudfront::{
    client::Waiters,
    types::{InvalidationBatch, Paths},
};
use aws_sdk_s3::{
    primitives::ByteStream,
    types::{ChecksumMode, ServerSideEncryption},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use minco_plugin_static_site::{
    StaticSiteAsset, StaticSiteError, StaticSitePlan, StaticSitePublication, StaticSitePublisher,
    StaticSiteReleaseManifest,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

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

impl AwsStaticSitePublisher {
    async fn acquire_publication_lock(
        &self,
        key: &str,
        manifest: &StaticSiteReleaseManifest,
    ) -> Result<(), StaticSiteError> {
        self.s3
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .if_none_match("*")
            .body(ByteStream::from(manifest.digest_sha256()?.into_bytes()))
            .content_type("text/plain")
            .cache_control("no-store")
            .server_side_encryption(ServerSideEncryption::Aes256)
            .send()
            .await
            .map_err(|error| {
                publish_error(format!(
                    "failed to acquire the exclusive S3 static-site publication lock: {error}"
                ))
            })?;
        Ok(())
    }

    async fn release_publication_lock(&self, key: &str) -> Result<(), StaticSiteError> {
        self.s3
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|error| {
                publish_error(format!(
                    "failed to release the S3 static-site publication lock: {error}"
                ))
            })?;
        Ok(())
    }

    async fn publish_verified_manifest(
        &self,
        manifest: &StaticSiteReleaseManifest,
        repository_root: &Path,
        lock_key: &str,
    ) -> Result<StaticSitePublication, StaticSiteError> {
        let source = repository_root.join(&manifest.plan.source_directory);
        let files = manifest
            .assets
            .iter()
            .map(|asset| PublicationFile {
                absolute: source.join(&asset.path),
                asset: asset.clone(),
            })
            .collect::<Vec<_>>();

        let mut expected = BTreeSet::from([lock_key.to_owned()]);
        for file in &files {
            let key = self.provider_key(&file.asset.path);
            expected.insert(key.clone());
            let checksum = s3_sha256_checksum(&file.asset.sha256)?;
            let body = ByteStream::from_path(&file.absolute)
                .await
                .map_err(|error| {
                    publish_error(format!(
                        "failed to read static-site file {}: {error}",
                        file.asset.path
                    ))
                })?;
            self.s3
                .put_object()
                .bucket(&self.bucket)
                .key(key)
                .body(body)
                .checksum_sha256(&checksum)
                .content_type(&file.asset.content_type)
                .cache_control(&file.asset.cache_control)
                .server_side_encryption(ServerSideEncryption::Aes256)
                .send()
                .await
                .map_err(|error| publish_error(format!("S3 PutObject failed: {error}")))?;
            let uploaded = self
                .s3
                .head_object()
                .bucket(&self.bucket)
                .key(self.provider_key(&file.asset.path))
                .checksum_mode(ChecksumMode::Enabled)
                .send()
                .await
                .map_err(|error| publish_error(format!("S3 HeadObject failed: {error}")))?;
            verify_uploaded_object(
                &file.asset,
                uploaded.content_length(),
                uploaded.checksum_sha256(),
                uploaded.content_type(),
                uploaded.cache_control(),
            )?;
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
                    .caller_reference(invalidation_caller_reference(manifest)?)
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
                let invalidation_id = output
                    .invalidation()
                    .map(|value| value.id().to_owned())
                    .ok_or_else(|| {
                        publish_error("CloudFront CreateInvalidation returned no invalidation")
                    })?;
                self.cloudfront
                    .wait_until_invalidation_completed()
                    .distribution_id(distribution_id)
                    .id(&invalidation_id)
                    .wait(std::time::Duration::from_mins(15))
                    .await
                    .map_err(|error| {
                        publish_error(format!(
                            "CloudFront invalidation {invalidation_id} did not complete: {error}"
                        ))
                    })?;
                Some(invalidation_id)
            }
            None => None,
        };

        Ok(StaticSitePublication {
            url: self.public_url.clone(),
            release_manifest_digest: manifest.digest_sha256()?,
            assets: manifest.assets.clone(),
            uploaded: files.len(),
            removed: stale.len(),
            invalidation_id,
            invalidation_completed: self.distribution_id.is_some(),
        })
    }
}

#[async_trait]
impl StaticSitePublisher for AwsStaticSitePublisher {
    async fn publish_manifest(
        &self,
        manifest: &StaticSiteReleaseManifest,
        repository_root: &Path,
    ) -> Result<StaticSitePublication, StaticSiteError> {
        manifest.verify_at(repository_root)?;
        let lock_key = self.provider_key(".minco/deployment-lock");
        self.acquire_publication_lock(&lock_key, manifest).await?;
        let publication = self
            .publish_verified_manifest(manifest, repository_root, &lock_key)
            .await;
        match publication {
            Ok(publication) => {
                self.release_publication_lock(&lock_key).await?;
                Ok(publication)
            }
            Err(error) => Err(publish_error(format!(
                "{error}; publication lock retained for explicit recovery because provider state may be partial"
            ))),
        }
    }
}

#[derive(Debug)]
struct PublicationFile {
    absolute: PathBuf,
    asset: StaticSiteAsset,
}

fn s3_sha256_checksum(hex_digest: &str) -> Result<String, StaticSiteError> {
    if hex_digest.len() != 64 || !hex_digest.bytes().all(|value| value.is_ascii_hexdigit()) {
        return Err(publish_error(
            "static-site release manifest contains an invalid SHA-256 digest",
        ));
    }
    let bytes = hex_digest
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair)
                .map_err(|_| publish_error("static-site SHA-256 digest is not ASCII"))?;
            u8::from_str_radix(pair, 16)
                .map_err(|_| publish_error("static-site SHA-256 digest is invalid"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(STANDARD.encode(bytes))
}

fn verify_uploaded_object(
    asset: &StaticSiteAsset,
    content_length: Option<i64>,
    checksum_sha256: Option<&str>,
    content_type: Option<&str>,
    cache_control: Option<&str>,
) -> Result<(), StaticSiteError> {
    let expected_length = i64::try_from(asset.bytes)
        .map_err(|_| publish_error(format!("static-site asset {} is too large", asset.path)))?;
    if content_length != Some(expected_length) {
        return Err(publish_error(format!(
            "S3 object {} has an unexpected content length",
            asset.path
        )));
    }
    if checksum_sha256 != Some(s3_sha256_checksum(&asset.sha256)?.as_str()) {
        return Err(publish_error(format!(
            "S3 object {} failed SHA-256 checksum verification",
            asset.path
        )));
    }
    if content_type != Some(asset.content_type.as_str()) {
        return Err(publish_error(format!(
            "S3 object {} has an unexpected content type",
            asset.path
        )));
    }
    if cache_control != Some(asset.cache_control.as_str()) {
        return Err(publish_error(format!(
            "S3 object {} has unexpected cache metadata",
            asset.path
        )));
    }
    Ok(())
}

fn invalidation_caller_reference(
    manifest: &StaticSiteReleaseManifest,
) -> Result<String, StaticSiteError> {
    Ok(format!("minco-static-{}", manifest.digest_sha256()?))
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
        "StaticSiteCachePolicy".into(),
        json!({
            "Type": "AWS::CloudFront::CachePolicy",
            "Properties": {
                "CachePolicyConfig": {
                    "Comment": "Minco static-site cache policy; asset Cache-Control headers remain authoritative",
                    "DefaultTTL": 0,
                    "MaxTTL": plan.immutable_cache_seconds,
                    "MinTTL": 0,
                    "Name": {"Fn::Sub": "minco-static-${AWS::StackName}"},
                    "ParametersInCacheKeyAndForwardedToOrigin": {
                        "CookiesConfig": {"CookieBehavior": "none"},
                        "EnableAcceptEncodingBrotli": true,
                        "EnableAcceptEncodingGzip": true,
                        "HeadersConfig": {"HeaderBehavior": "none"},
                        "QueryStringsConfig": {"QueryStringBehavior": "none"}
                    }
                }
            }
        }),
    );
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
                        "CachePolicyId": {"Ref": "StaticSiteCachePolicy"},
                        "Compress": true,
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
    use minco_plugin_static_site::StaticSiteAsset;

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
    fn uploaded_object_must_match_the_release_manifest_metadata() {
        let asset = StaticSiteAsset {
            path: "index.html".into(),
            bytes: 5,
            sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".into(),
            content_type: "text/html".into(),
            cache_control: "public,max-age=0,must-revalidate".into(),
        };
        let checksum = s3_sha256_checksum(&asset.sha256).expect("valid release checksum");
        assert_eq!(checksum, "LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ=");
        verify_uploaded_object(
            &asset,
            Some(5),
            Some(&checksum),
            Some("text/html"),
            Some("public,max-age=0,must-revalidate"),
        )
        .expect("matching provider metadata");

        let error = verify_uploaded_object(
            &asset,
            Some(5),
            Some("wrong"),
            Some("text/html"),
            Some("public,max-age=0,must-revalidate"),
        )
        .expect_err("provider checksum mismatch must fail");
        assert!(error.to_string().contains("checksum"));
    }

    #[test]
    fn invalidation_reference_is_deterministic_for_the_exact_release() {
        let manifest = StaticSiteReleaseManifest {
            schema_version: 1,
            plan: plan(),
            assets: vec![StaticSiteAsset {
                path: "index.html".into(),
                bytes: 5,
                sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".into(),
                content_type: "text/html".into(),
                cache_control: "public,max-age=0,must-revalidate".into(),
            }],
        };
        let first = invalidation_caller_reference(&manifest).expect("release reference");
        let second = invalidation_caller_reference(&manifest).expect("stable release reference");
        assert_eq!(first, second);
        assert!(first.starts_with("minco-static-"));
        assert_eq!(first.len(), "minco-static-".len() + 64);
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
        assert_eq!(
            template["Resources"]["StaticSiteDistribution"]["Properties"]["DistributionConfig"]["DefaultCacheBehavior"]
                ["CachePolicyId"]["Ref"],
            "StaticSiteCachePolicy"
        );
        assert!(
            template["Resources"]["StaticSiteDistribution"]["Properties"]
                ["DistributionConfig"]["DefaultCacheBehavior"]
                .get("ForwardedValues")
                .is_none()
        );
        assert_eq!(
            template["Resources"]["StaticSiteCachePolicy"]["Properties"]["CachePolicyConfig"]["ParametersInCacheKeyAndForwardedToOrigin"]
                ["CookiesConfig"]["CookieBehavior"],
            "none"
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
