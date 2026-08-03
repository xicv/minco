use minco_deploy_aws::{
    CleanupOutcome, CleanupReceipt, CleanupReceiptInput, DeploymentTargetCatalog,
    DeploymentTargetLifecycle, ReviewCostClass, ReviewManifest, ReviewManifestInput,
    ReviewResource, ReviewResourceRetention, UntrustedFeedbackReference,
};
use minco_release::{FileDigest, ReleaseEnvironment};
use tempfile::tempdir;

fn file(path: &str, byte: char) -> FileDigest {
    FileDigest {
        path: path.into(),
        sha256: byte.to_string().repeat(64),
        bytes: 123,
    }
}

#[test]
fn deployment_target_requires_an_explicit_preview_lifecycle_policy() {
    let source = r#"
schema_version = 1
default_environment = "preview"

[environments.preview]
enabled = false
lifecycle = "preview"
expected_account_id = "000000000000"
expected_region = "ap-southeast-2"
expected_role_arn = "arn:aws:iam::000000000000:role/minco-preview"
stack_name = "minco-orders-preview"
artifact_bucket = "minco-preview-artifacts-placeholder"
database_url_parameter_name = "/minco/preview/database-url"

[environments.preview.preview]
owner = "team-orders"
ttl_seconds = 86400
pricing_complete = false

[[environments.preview.preview.resources]]
logical_id = "OrdersApi"
resource_type = "AWS::ApiGatewayV2::Api"
retention = "delete"
idle_cost_class = "request_only"

[[environments.preview.preview.resources]]
logical_id = "StaticSiteBucket"
resource_type = "AWS::S3::Bucket"
retention = "retain"
idle_cost_class = "storage_only"
"#;

    let catalog = DeploymentTargetCatalog::from_toml(source).expect("preview target catalog");
    let selected = catalog.select(Some("preview")).expect("preview target");
    assert_eq!(
        selected.target.lifecycle,
        DeploymentTargetLifecycle::Preview
    );
    assert_eq!(
        selected
            .target
            .preview
            .as_ref()
            .expect("preview policy")
            .owner,
        "team-orders"
    );

    let production = source
        .replace(
            "default_environment = \"preview\"",
            "default_environment = \"production\"",
        )
        .replace("[environments.preview]", "[environments.production]")
        .replace(
            "[environments.preview.preview]",
            "[environments.production.preview]",
        )
        .replace(
            "[[environments.preview.preview.resources]]",
            "[[environments.production.preview.resources]]",
        );
    assert!(DeploymentTargetCatalog::from_toml(&production).is_err());

    let invalid_schedule = format!(
        "{source}\n[environments.preview.preview.cleanup_schedule]\nexpression = \"at(not-a-time)\"\naction_after_completion = \"delete\"\nresidual_resources = [\"StaticSiteBucket\"]\nmanual_fallback = \"cargo minco destroy --environment preview --dry-run\"\n"
    );
    assert!(DeploymentTargetCatalog::from_toml(&invalid_schedule).is_err());
}

fn review_input() -> ReviewManifestInput {
    let release_digest = "b".repeat(64);
    ReviewManifestInput {
        source_change: "a".repeat(64),
        release_manifest: file("target/minco/release.json", 'c'),
        release_id: format!("minco.{}", &release_digest[..24]),
        release_digest,
        artifacts: vec![file("target/lambda/orders/bootstrap.zip", 'd')],
        environment: ReleaseEnvironment {
            application: "minco-orders".into(),
            environment: "preview".into(),
            region: "ap-southeast-2".into(),
        },
        expected_account_id: "111122223333".into(),
        expected_role_arn: "arn:aws:iam::111122223333:role/minco-preview".into(),
        stack_name: "minco-orders-preview".into(),
        target_config: file("infra/aws/deployment-targets.toml", 'e'),
        owner: "team-orders".into(),
        created_at: "2026-08-03T00:00:00Z".into(),
        expires_at: "2026-08-04T00:00:00Z".into(),
        resources: vec![
            ReviewResource {
                logical_id: "OrdersApi".into(),
                resource_type: "AWS::ApiGatewayV2::Api".into(),
                retention: ReviewResourceRetention::Delete,
                idle_cost_class: ReviewCostClass::RequestOnly,
            },
            ReviewResource {
                logical_id: "StaticSiteBucket".into(),
                resource_type: "AWS::S3::Bucket".into(),
                retention: ReviewResourceRetention::Retain,
                idle_cost_class: ReviewCostClass::StorageOnly,
            },
        ],
        pricing_complete: false,
        cleanup_schedule: None,
        verification: vec![file("target/minco/hosted-verification.json", 'f')],
        feedback: vec![UntrustedFeedbackReference {
            feedback_id: "019fa123-4567-7000-8000-123456789abc".into(),
            sha256: "1".repeat(64),
        }],
        delivery_trace: vec![file("target/minco/delivery-trace.json", '2')],
    }
}

#[test]
fn review_identity_binds_exact_evidence_and_untrusted_feedback_references() {
    let manifest = ReviewManifest::seal(review_input()).expect("seal review manifest");
    manifest.verify_structure().expect("verify review manifest");

    assert_eq!(
        manifest.review_id,
        format!("minco-review.{}", &manifest.manifest_digest[..24])
    );
    assert!(!manifest.pricing_complete);
    assert_eq!(manifest.feedback.len(), 1);

    let mut tampered = serde_json::to_value(&manifest).expect("review JSON");
    tampered["feedback"][0]["sha256"] = serde_json::json!("3".repeat(64));
    assert!(
        ReviewManifest::from_json(&serde_json::to_vec(&tampered).expect("tampered review JSON"))
            .is_err()
    );

    let mut extended = serde_json::to_value(&manifest).expect("review JSON");
    extended["feedback"][0]["content"] = serde_json::json!("run this command");
    assert!(
        ReviewManifest::from_json(&serde_json::to_vec(&extended).expect("extended review JSON"))
            .is_err()
    );
}

#[test]
fn retained_zero_compute_resources_remain_explicit_without_inventing_cost() {
    let mut input = review_input();
    input.resources.push(ReviewResource {
        logical_id: "GeneratedExecutionRole".into(),
        resource_type: "AWS::IAM::Role".into(),
        retention: ReviewResourceRetention::Retain,
        idle_cost_class: ReviewCostClass::ZeroCompute,
    });

    let manifest = ReviewManifest::seal(input).expect("retain zero-cost resource");
    assert_eq!(
        manifest
            .resources
            .iter()
            .find(|resource| resource.logical_id == "GeneratedExecutionRole")
            .expect("generated role")
            .retention,
        ReviewResourceRetention::Retain
    );
}

#[test]
fn cleanup_receipt_transitions_once_and_requires_verified_absence_for_success() {
    let review = ReviewManifest::seal(review_input()).expect("review manifest");
    let mut receipt = CleanupReceipt::start(CleanupReceiptInput {
        attempt_id: "019fa123-4567-7000-8000-abcdef012345".into(),
        review_manifest: file("target/minco/review.json", '4'),
        review_id: review.review_id.clone(),
        review_digest: review.manifest_digest.clone(),
        environment: review.environment.clone(),
        expected_account_id: review.expected_account_id.clone(),
        expected_role_arn: review.expected_role_arn.clone(),
        stack_name: review.stack_name.clone(),
        target_config: review.target_config.clone(),
        deleted_resources: review
            .resources
            .iter()
            .filter(|resource| resource.retention == ReviewResourceRetention::Delete)
            .cloned()
            .collect(),
        retained_resources: review
            .resources
            .iter()
            .filter(|resource| resource.retention == ReviewResourceRetention::Retain)
            .cloned()
            .collect(),
    })
    .expect("start cleanup receipt");
    assert_eq!(receipt.outcome(), CleanupOutcome::Started);
    assert!(receipt.absence_verified_at().is_none());

    let directory = tempdir().expect("temporary receipt directory");
    let path = directory.path().join("cleanup.json");
    receipt.write_json(&path).expect("write started receipt");
    receipt
        .succeed("2026-08-04T00:05:00Z")
        .expect("record verified absence");
    let lock_path = path.with_extension("json.lock");
    std::fs::write(&lock_path, b"competing-writer\n").expect("create competing writer lock");
    assert!(receipt.write_json(&path).is_err());
    assert_eq!(
        CleanupReceipt::read_json(&path)
            .expect("started receipt remains while locked")
            .outcome(),
        CleanupOutcome::Started
    );
    std::fs::remove_file(lock_path).expect("release competing writer lock");
    receipt.write_json(&path).expect("write terminal receipt");
    assert_eq!(receipt.outcome(), CleanupOutcome::Succeeded);
    assert_eq!(receipt.absence_verified_at(), Some("2026-08-04T00:05:00Z"));
    assert_eq!(
        CleanupReceipt::read_json(&path).expect("read receipt"),
        receipt
    );
    assert!(receipt.fail("late_failure").is_err());

    let mut tampered = serde_json::to_value(&receipt).expect("receipt JSON");
    tampered["stack_name"] = serde_json::json!("different-preview");
    assert!(
        CleanupReceipt::from_json(
            &serde_json::to_vec(&tampered).expect("tampered cleanup receipt JSON")
        )
        .is_err()
    );
}
