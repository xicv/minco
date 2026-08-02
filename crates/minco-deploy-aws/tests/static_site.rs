use minco_deploy_aws::{
    CloudFrontBillingModel, FlatRateEligibility, StaticSiteCertificateObservation,
    StaticSiteDistributionStatus, StaticSiteDnsObservation, StaticSiteInvalidationStatus,
    StaticSiteObjectObservation, StaticSitePricingEvidence, StaticSiteProviderObservation,
    StaticSitePublicationReceipt, StaticSitePublicationReceiptInput, StaticSiteVerificationError,
    StaticSiteVerificationInput, StaticSiteVerificationReport,
};
use minco_plugin_static_site::{
    StaticSiteAsset, StaticSitePlan, StaticSitePublication, StaticSiteReleaseManifest,
};
use minco_release::FileDigest;
use std::fs;

const RELEASE_DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const ASSET_DIGEST: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

fn manifest() -> StaticSiteReleaseManifest {
    StaticSiteReleaseManifest {
        schema_version: 1,
        plan: StaticSitePlan {
            source_directory: "dist".into(),
            index_document: "index.html".into(),
            spa_fallback: true,
            immutable_cache_seconds: 31_536_000,
            html_cache_seconds: 0,
            price_class: "PriceClass_100".into(),
            ipv6_enabled: true,
            custom_domain: Some("app.example.com".into()),
            manage_dns_alias: true,
        },
        assets: vec![StaticSiteAsset {
            path: "index.html".into(),
            bytes: 5,
            sha256: ASSET_DIGEST.into(),
            content_type: "text/html".into(),
            cache_control: "public,max-age=0,must-revalidate".into(),
        }],
    }
}

fn input() -> StaticSiteVerificationInput {
    StaticSiteVerificationInput {
        release_digest: RELEASE_DIGEST.into(),
        expected_account_id: "123456789012".into(),
        deployment_region: "ap-southeast-2".into(),
        manifest: manifest(),
        observation: StaticSiteProviderObservation {
            bucket: "minco-static-example".into(),
            distribution_id: "E1234567890ABC".into(),
            distribution_domain: "d111111abcdef8.cloudfront.net".into(),
            distribution_status: StaticSiteDistributionStatus::Deployed,
            distribution_aliases: vec!["app.example.com".into()],
            distribution_certificate_arn: Some(
                "arn:aws:acm:us-east-1:123456789012:certificate/example".into(),
            ),
            origin_domain: "minco-static-example.s3.ap-southeast-2.amazonaws.com".into(),
            origin_access_control_id: "EEEEEEEE".into(),
            invalidation_id: "I1234567890ABC".into(),
            invalidation_status: StaticSiteInvalidationStatus::Completed,
            certificate: Some(StaticSiteCertificateObservation {
                arn: "arn:aws:acm:us-east-1:123456789012:certificate/example".into(),
                status: "ISSUED".into(),
                names: vec!["app.example.com".into()],
            }),
            dns: Some(StaticSiteDnsObservation {
                hosted_zone_id: "Z1234567890ABC".into(),
                hosted_zone_name: "example.com".into(),
                private_zone: false,
                a_target: "d111111abcdef8.cloudfront.net".into(),
                aaaa_target: Some("d111111abcdef8.cloudfront.net".into()),
            }),
            objects: vec![StaticSiteObjectObservation {
                path: "index.html".into(),
                s3_bytes: 5,
                s3_sha256: ASSET_DIGEST.into(),
                s3_content_type: "text/html".into(),
                s3_cache_control: "public,max-age=0,must-revalidate".into(),
                cloudfront_bytes: 5,
                cloudfront_sha256: ASSET_DIGEST.into(),
                cloudfront_content_type: "text/html".into(),
                cloudfront_cache_control: "public,max-age=0,must-revalidate".into(),
            }],
            pricing: StaticSitePricingEvidence {
                checked_on: "2026-08-02".into(),
                source: "https://aws.amazon.com/cloudfront/pricing/".into(),
                billing_model: CloudFrontBillingModel::RequestAndTransfer,
                price_class: "PriceClass_100".into(),
                flat_rate_eligibility: FlatRateEligibility::Ineligible,
            },
        },
    }
}

#[test]
fn exact_provider_state_completes_static_site_verification() {
    let report = StaticSiteVerificationReport::complete(input()).expect("verified static site");
    assert_eq!(report.release_digest, RELEASE_DIGEST);
    assert_eq!(report.observation.objects[0].path, "index.html");
    report.verify_structure().expect("self-verifying report");
}

#[test]
fn provider_byte_mismatch_fails_closed() {
    let mut input = input();
    input.observation.objects[0].cloudfront_sha256 = "b".repeat(64);
    assert_eq!(
        StaticSiteVerificationReport::complete(input),
        Err(StaticSiteVerificationError::AssetMismatch {
            path: "index.html".into()
        })
    );
}

#[test]
fn certificate_and_dns_must_own_the_custom_domain() {
    let mut certificate = input();
    certificate.observation.certificate.as_mut().unwrap().arn =
        "arn:aws:acm:ap-southeast-2:123456789012:certificate/example".into();
    assert_eq!(
        StaticSiteVerificationReport::complete(certificate),
        Err(StaticSiteVerificationError::CertificateMismatch)
    );

    let mut dns = input();
    dns.observation.dns.as_mut().unwrap().a_target = "other.cloudfront.net".into();
    assert_eq!(
        StaticSiteVerificationReport::complete(dns),
        Err(StaticSiteVerificationError::DnsMismatch)
    );

    let mut origin = input();
    origin.observation.origin_domain = "minco-static-example.s3.us-east-1.amazonaws.com".into();
    assert_eq!(
        StaticSiteVerificationReport::complete(origin),
        Err(StaticSiteVerificationError::DistributionMismatch)
    );
}

#[test]
fn flat_rate_selection_requires_observed_account_eligibility() {
    let mut input = input();
    input.observation.pricing.billing_model = CloudFrontBillingModel::FlatRate;
    assert_eq!(
        StaticSiteVerificationReport::complete(input),
        Err(StaticSiteVerificationError::PricingEvidenceInvalid)
    );

    let mut impossible_date = self::input();
    impossible_date.observation.pricing.checked_on = "2026-02-31".into();
    assert_eq!(
        StaticSiteVerificationReport::complete(impossible_date),
        Err(StaticSiteVerificationError::PricingEvidenceInvalid)
    );
}

#[test]
fn publication_receipt_binds_the_release_manifest_and_completed_invalidation() {
    let root = tempfile::tempdir().expect("project root");
    fs::create_dir_all(root.path().join("dist")).expect("dist");
    fs::create_dir_all(root.path().join("target/minco")).expect("receipt directory");
    fs::write(root.path().join("dist/index.html"), b"hello").expect("asset");
    let manifest =
        StaticSiteReleaseManifest::build(&manifest().plan, root.path()).expect("release manifest");
    let manifest_path = root.path().join("target/minco/static-site-release.json");
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("manifest JSON"),
    )
    .expect("manifest file");

    let receipt = StaticSitePublicationReceipt::seal(StaticSitePublicationReceiptInput {
        release_digest: RELEASE_DIGEST.into(),
        manifest_file: FileDigest::from_rooted_path(root.path(), &manifest_path)
            .expect("manifest digest"),
        bucket: "minco-static-example".into(),
        distribution_id: "E1234567890ABC".into(),
        distribution_domain: "d111111abcdef8.cloudfront.net".into(),
        publication: StaticSitePublication {
            url: "https://app.example.com".into(),
            release_manifest_digest: manifest.digest_sha256().expect("semantic digest"),
            assets: manifest.assets.clone(),
            uploaded: manifest.assets.len(),
            removed: 0,
            invalidation_id: Some("I1234567890ABC".into()),
            invalidation_completed: true,
        },
    })
    .expect("publication receipt");

    receipt.verify_at(root.path()).expect("bound publication");
    let mut missing_invalidation = receipt.clone();
    missing_invalidation.publication.invalidation_id = None;
    assert!(missing_invalidation.verify_structure().is_err());

    let mut tampered = receipt;
    tampered.publication.assets[0].sha256 = "b".repeat(64);
    assert!(tampered.verify_at(root.path()).is_err());
}
