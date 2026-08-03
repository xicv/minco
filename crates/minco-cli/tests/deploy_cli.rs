use std::{path::Path, process::Command};

#[test]
fn preview_deployment_plan_is_repository_native_and_has_no_default_wakeup() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .args([
            "--root",
            root.to_str().expect("UTF-8 root"),
            "deploy",
            "plan",
            "--environment",
            "preview",
            "--stdout",
        ])
        .output()
        .expect("execute preview deployment plan");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("preview plan");
    assert_eq!(plan["environment"], "preview");
    assert_eq!(plan["preview"]["owner"], "repository-reviewer");
    assert_eq!(plan["preview"]["ttl_seconds"], 86_400);
    assert_eq!(plan["preview"]["expected_region"], "ap-southeast-2");
    assert_eq!(plan["preview"]["pricing_complete"], false);
    assert!(plan["preview"].get("cleanup_schedule").is_none());
    assert_eq!(plan["scheduled_wakeups"], serde_json::json!([]));
}

#[test]
fn preview_destroy_dry_run_is_local_and_shows_exact_cleanup_authority() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .args([
            "--root",
            root.to_str().expect("UTF-8 root"),
            "--json",
            "destroy",
            "--environment",
            "preview",
            "--dry-run",
        ])
        .output()
        .expect("execute preview destroy dry-run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("cleanup plan");
    assert_eq!(plan["external_aws_contact"], false);
    assert_eq!(plan["infrastructure_change"], false);
    assert_eq!(plan["cleanup_receipt_written"], false);
    assert_eq!(plan["target"]["environment"], "preview");
    assert_eq!(plan["target"]["lifecycle"], "preview");
    assert_eq!(plan["target"]["enabled"], false);
    assert_eq!(plan["deleted_resources"].as_array().map(Vec::len), Some(7));
    assert_eq!(plan["retained_resources"], serde_json::json!([]));
    assert_eq!(
        plan["blockers"],
        serde_json::json!([
            "target_disabled",
            "review_manifest_missing",
            "review_approval_missing"
        ])
    );
}

#[test]
fn persistent_target_destroy_is_visible_but_never_authorized() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .args([
            "--root",
            root.to_str().expect("UTF-8 root"),
            "--json",
            "destroy",
            "--environment",
            "dev",
            "--dry-run",
        ])
        .output()
        .expect("execute persistent destroy dry-run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("cleanup plan");
    assert_eq!(plan["external_aws_contact"], false);
    assert_eq!(plan["infrastructure_change"], false);
    assert_eq!(plan["target"]["lifecycle"], "persistent");
    assert!(
        plan["blockers"]
            .as_array()
            .expect("blockers")
            .contains(&serde_json::json!("target_not_preview"))
    );
    assert_eq!(plan["deleted_resources"], serde_json::json!([]));
}

#[test]
fn preview_review_dry_run_is_local_and_requires_exact_deployment_evidence() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .args([
            "--root",
            root.to_str().expect("UTF-8 root"),
            "--json",
            "deploy",
            "review",
            "--environment",
            "preview",
            "--dry-run",
        ])
        .output()
        .expect("execute preview review dry-run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("review plan");
    assert_eq!(plan["external_aws_contact"], false);
    assert_eq!(plan["review_manifest_written"], false);
    assert_eq!(plan["target"]["lifecycle"], "preview");
    assert_eq!(
        plan["blockers"],
        serde_json::json!([
            "target_disabled",
            "release_manifest_missing",
            "deployment_receipt_missing"
        ])
    );
}

#[test]
fn default_change_set_dry_run_is_local_redacted_and_blocked() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .args([
            "--root",
            root.to_str().expect("UTF-8 root"),
            "--json",
            "deploy",
            "changeset",
            "--dry-run",
        ])
        .output()
        .expect("execute change-set dry-run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON plan");
    assert_eq!(plan["external_aws_contact"], false);
    assert_eq!(plan["infrastructure_apply"], false);
    assert_eq!(plan["target"]["environment"], "dev");
    assert_eq!(plan["target"]["enabled"], false);
    assert_eq!(
        plan["blockers"],
        serde_json::json!([
            "target_disabled",
            "release_manifest_missing",
            "release_approval_missing"
        ])
    );
    let serialized = serde_json::to_string(&plan).expect("serialize plan");
    assert!(!serialized.contains("database_url_parameter_name"));
    assert!(!serialized.contains("secret_value"));
}

#[test]
fn default_apply_dry_run_is_local_and_requires_separate_evidence() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .args([
            "--root",
            root.to_str().expect("UTF-8 root"),
            "--json",
            "deploy",
            "apply",
            "--dry-run",
        ])
        .output()
        .expect("execute deployment apply dry-run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON plan");
    assert_eq!(plan["external_aws_contact"], false);
    assert_eq!(plan["infrastructure_apply"], false);
    assert_eq!(
        plan["blockers"],
        serde_json::json!([
            "change_set_receipt_missing",
            "migration_plan_missing",
            "migration_receipt_missing",
            "changeset_approval_missing"
        ])
    );
}

#[test]
fn default_hosted_verification_dry_run_is_local_and_requires_deployment_evidence() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .args([
            "--root",
            root.to_str().expect("UTF-8 root"),
            "--json",
            "deploy",
            "verify",
            "--dry-run",
        ])
        .output()
        .expect("execute hosted verification dry-run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON plan");
    assert_eq!(plan["external_aws_contact"], false);
    assert_eq!(plan["external_http_contact"], false);
    assert_eq!(plan["deployment_receipt_transition"], false);
    assert_eq!(
        plan["blockers"],
        serde_json::json!(["release_manifest_missing", "deployment_receipt_missing"])
    );
}

#[test]
fn static_site_publication_plan_is_local_and_names_destructive_ordering() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .args([
            "--root",
            root.to_str().expect("UTF-8 root"),
            "--json",
            "deploy",
            "static-site",
            "plan",
        ])
        .output()
        .expect("execute static-site publication plan");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON plan");
    assert_eq!(plan["external_aws_contact"], false);
    assert_eq!(plan["infrastructure_change"], false);
    assert_eq!(
        plan["stale_object_deletion_after_checksum_verification"],
        true
    );
    assert_eq!(plan["cloudfront_invalidation_wait"], true);
    assert_eq!(
        plan["blockers"],
        serde_json::json!(["release_manifest_missing", "deployment_receipt_missing"])
    );
}

#[test]
fn static_site_verification_dry_run_requires_both_runtime_and_publication_evidence() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .args([
            "--root",
            root.to_str().expect("UTF-8 root"),
            "--json",
            "deploy",
            "verify",
            "--static-site",
            "--dry-run",
        ])
        .output()
        .expect("execute static-site verification plan");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON plan");
    assert_eq!(plan["external_aws_contact"], false);
    assert_eq!(plan["external_http_contact"], false);
    assert_eq!(plan["static_site"], true);
    assert_eq!(
        plan["blockers"],
        serde_json::json!([
            "release_manifest_missing",
            "deployment_receipt_missing",
            "static_site_publication_missing"
        ])
    );
}

#[test]
fn default_promotion_dry_run_never_contacts_aws_or_rebuilds() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .args([
            "--root",
            root.to_str().expect("UTF-8 root"),
            "--json",
            "promote",
            "--dry-run",
        ])
        .output()
        .expect("execute promotion dry-run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON plan");
    assert_eq!(plan["external_aws_contact"], false);
    assert_eq!(plan["rebuild"], false);
    assert_eq!(plan["replan"], false);
    assert_eq!(plan["routing_boundary"], "lambda_alias");
    assert_eq!(
        plan["blockers"],
        serde_json::json!([
            "release_manifest_missing",
            "deployment_receipt_missing",
            "hosted_verification_missing",
            "verification_approval_missing"
        ])
    );
}
