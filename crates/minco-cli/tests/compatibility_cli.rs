use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::TempDir;

const fn cargo_minco() -> &'static str {
    env!("CARGO_BIN_EXE_cargo-minco")
}

fn create_versioned_application() -> (TempDir, PathBuf) {
    let temporary = tempfile::tempdir().expect("temporary application parent");
    let root = temporary.path().join("compatibility-api");
    let output = Command::new(cargo_minco())
        .args([
            "new",
            "compatibility-api",
            "--directory",
            root.to_str().expect("UTF-8 application root"),
            "--database",
            "sqlite",
            "--vcs",
            "jj",
        ])
        .output()
        .expect("create versioned application");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let describe = Command::new("jj")
        .args(["describe", "-m", "baseline contract"])
        .current_dir(&root)
        .output()
        .expect("describe baseline");
    assert!(describe.status.success());
    let new = Command::new("jj")
        .arg("new")
        .current_dir(&root)
        .output()
        .expect("start candidate contract");
    assert!(new.status.success());
    (temporary, root)
}

fn run(root: &Path, arguments: &[&str]) -> Output {
    let mut command = Command::new(cargo_minco());
    command.args([
        "--root",
        root.to_str().expect("UTF-8 application root"),
        "--json",
    ]);
    command.args(arguments);
    command.output().expect("run cargo minco compatibility")
}

#[test]
fn contract_diff_reads_a_validated_baseline_without_checkout() {
    let (_temporary, root) = create_versioned_application();
    let contract_path = root.join("openapi/openapi.yaml");
    let baseline = fs::read_to_string(&contract_path).expect("baseline contract");
    fs::write(
        &contract_path,
        baseline.replacen("operationId: getPlatform", "operationId: readPlatform", 1),
    )
    .expect("candidate contract");

    let output = run(&root, &["contract", "diff", "--against", "@-"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("JSON compatibility report");
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["baseline_source"], "@-:openapi/openapi.yaml");
    assert_eq!(report["candidate_source"], "openapi/openapi.yaml");
    assert_eq!(report["classification"], "breaking");
    assert_eq!(report["operation_changes"][0]["code"], "operation.added");
    assert_eq!(report["operation_changes"][1]["code"], "operation.removed");
}

#[test]
fn upgrade_report_preserves_version_and_feature_boundary_evidence() {
    let (_temporary, root) = create_versioned_application();
    let cargo_path = root.join("Cargo.toml");
    let cargo = fs::read_to_string(&cargo_path).expect("generated Cargo.toml");
    fs::write(
        &cargo_path,
        cargo.replacen(
            r#"features = ["test", "plan", "sqlx-sqlite"]"#,
            r#"features = ["test", "plan", "sqlx-sqlite", "aws-worker"]"#,
            1,
        ),
    )
    .expect("feature-boundary fixture");
    let manifest_path = root.join("minco.toml");
    let manifest = fs::read_to_string(&manifest_path).expect("generated minco.toml");
    fs::write(
        &manifest_path,
        manifest.replacen("schema = 1", "schema = 7", 1),
    )
    .expect("versioned manifest fixture");

    let first = run(&root, &["upgrade", "report"]);
    let second = run(&root, &["upgrade", "report"]);

    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(first.stdout, second.stdout);
    let report: Value = serde_json::from_slice(&first.stdout).expect("JSON upgrade report");
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["application"], "compatibility-api");
    assert_eq!(report["assessment"], "review_required");
    assert_eq!(report["rust"]["application_minimum"], "1.97.1");
    assert_eq!(report["cli"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        report["cargo_features"]["selected"],
        serde_json::json!(["aws-worker", "plan", "sqlx-sqlite", "test"])
    );
    assert_eq!(report["configuration"]["supported_schema_version"], 1);
    assert_eq!(
        report["configuration"]["fields"][2]["key"],
        "runtime.log_level"
    );
    assert!(
        report["configuration"]["fields"][0]
            .get("default")
            .is_none()
    );
    assert_eq!(report["plugins"]["catalog_schema_version"], 1);
    assert_eq!(
        report["plugins"]["enabled"],
        serde_json::json!(["health", "idempotency", "observability"])
    );
    assert_eq!(report["serialized"]["manifest_schema_version"], 7);
    assert_eq!(report["serialized"]["deployment_plan_schema_version"], 1);
    assert_eq!(report["serialized"]["contract_openapi_version"], "3.1.0");
    assert_eq!(
        report["diagnostics"][0]["code"],
        "upgrade.manifest_schema.unsupported"
    );
    let serialized = String::from_utf8(first.stdout).expect("UTF-8 upgrade report");
    assert!(!serialized.contains(r#""default":"info""#));
    assert!(!serialized.contains("DATABASE_URL"));
}

#[test]
fn upgrade_report_keeps_malformed_auxiliary_boundaries_as_diagnostics() {
    let (_temporary, root) = create_versioned_application();
    fs::write(root.join("plugins/catalog.toml"), "schema =")
        .expect("malformed plugin-catalog fixture");
    fs::write(root.join("environments/dev.toml"), "schema_version =")
        .expect("malformed deployment fixture");

    let first = run(&root, &["upgrade", "report"]);
    let second = run(&root, &["upgrade", "report"]);

    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(first.stdout, second.stdout);
    let report: Value = serde_json::from_slice(&first.stdout).expect("JSON upgrade report");
    assert_eq!(
        report["diagnostics"]
            .as_array()
            .expect("diagnostic list")
            .iter()
            .map(|diagnostic| diagnostic["code"].as_str().expect("diagnostic code"))
            .collect::<Vec<_>>(),
        vec![
            "upgrade.deployment.invalid",
            "upgrade.plugin_catalog.invalid"
        ]
    );
    assert!(report["serialized"]["deployment_plan_schema_version"].is_null());
    assert!(report["plugins"]["catalog_schema_version"].is_null());
}
