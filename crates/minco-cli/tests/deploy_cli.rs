use std::{path::Path, process::Command};

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
