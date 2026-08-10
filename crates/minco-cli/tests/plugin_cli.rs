use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;

const fn cargo_minco() -> &'static str {
    env!("CARGO_BIN_EXE_cargo-minco")
}

const CURRENT_MINCO_VERSION: &str = env!("CARGO_PKG_VERSION");

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}

fn run(arguments: &[&str]) -> std::process::Output {
    run_at(workspace_root(), arguments)
}

fn run_at(root: &Path, arguments: &[&str]) -> std::process::Output {
    let mut command = Command::new(cargo_minco());
    command.args([
        "--root",
        root.to_str().expect("UTF-8 workspace root"),
        "--json",
    ]);
    command.args(arguments);
    command.output().expect("run cargo minco plugin command")
}

fn create_application() -> (TempDir, PathBuf) {
    let temporary = tempfile::tempdir().expect("temporary application parent");
    let root = temporary.path().join("sample-api");
    let output = Command::new(cargo_minco())
        .args([
            "new",
            "sample-api",
            "--directory",
            root.to_str().expect("UTF-8 application root"),
            "--database",
            "sqlite",
            "--vcs",
            "none",
        ])
        .output()
        .expect("create generated application");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    (temporary, root)
}

fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn visit(root: &Path, current: &Path, output: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in fs::read_dir(current).expect("read generated application") {
            let entry = entry.expect("directory entry");
            let path = entry.path();
            if path.is_dir() {
                visit(root, &path, output);
            } else {
                output.push((
                    path.strip_prefix(root)
                        .expect("file inside application")
                        .to_path_buf(),
                    fs::read(path).expect("read generated file"),
                ));
            }
        }
    }

    let mut output = Vec::new();
    visit(root, root, &mut output);
    output.sort_by(|left, right| left.0.cmp(&right.0));
    output
}

#[test]
fn plugin_add_dry_run_resolves_an_explicit_version_without_writing() {
    let (_temporary, root) = create_application();
    let before = snapshot(&root);

    let first = run_at(
        &root,
        &["plugin", "add", "minco-plugin-health", "--dry-run"],
    );
    let second = run_at(
        &root,
        &["plugin", "add", "minco-plugin-health", "--dry-run"],
    );

    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(snapshot(&root), before);

    let plan: Value = serde_json::from_slice(&first.stdout).expect("plugin add JSON plan");
    assert_eq!(plan["schema_version"], 1);
    assert_eq!(plan["operation"], "add");
    assert_eq!(plan["plugin"]["id"], "health");
    assert_eq!(plan["plugin"]["crate"], "minco-plugin-health");
    assert_eq!(plan["plugin"]["feature"], "plugin-health");
    assert_eq!(plan["plugin"]["resolved_version"], CURRENT_MINCO_VERSION);
    assert_eq!(plan["dry_run"], true);
    assert_eq!(plan["applied"], false);
    assert_eq!(
        plan["registration"]["strategy"],
        "minco_facade_static_registration"
    );
    assert_eq!(
        plan["registration"]["composition_root"],
        "services/app/src/lib.rs"
    );
    assert_eq!(
        plan["changes"]
            .as_array()
            .expect("semantic changes only")
            .iter()
            .map(|change| change["path"].as_str().expect("change path"))
            .collect::<Vec<_>>(),
        ["Cargo.toml"]
    );
}

#[test]
fn framework_catalog_add_is_idempotent_when_the_facade_already_declares_the_feature() {
    let output = run(&["plugin", "add", "minco-plugin-health", "--dry-run"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: Value = serde_json::from_slice(&output.stdout).expect("framework add plan");
    assert_eq!(plan["plugin"]["resolved_version"], CURRENT_MINCO_VERSION);
    assert_eq!(plan["changes"], serde_json::json!([]));
}

#[test]
fn plugin_add_does_not_route_runtime_components_into_plugin_selection() {
    let output = run(&["plugin", "add", "aws-lambda", "--dry-run"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 component kind error");
    assert!(stderr.contains("aws-lambda is a runtime, not a composable plugin"));
}

#[test]
fn plugin_add_rejects_a_cli_and_application_version_mismatch_before_writing() {
    let (_temporary, root) = create_application();
    let cargo_path = root.join("Cargo.toml");
    let cargo = fs::read_to_string(&cargo_path)
        .expect("workspace Cargo.toml")
        .replace(
            &format!("version = \"{CURRENT_MINCO_VERSION}\""),
            "version = \"0.5.0\"",
        );
    fs::write(&cargo_path, cargo).expect("set older Minco dependency");
    let before = snapshot(&root);

    let output = run_at(
        &root,
        &["plugin", "add", "minco-plugin-health", "--dry-run"],
    );

    assert!(!output.status.success());
    assert_eq!(snapshot(&root), before);
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 version mismatch");
    assert!(stderr.contains("application Minco version 0.5.0"));
    assert!(stderr.contains(&format!("cargo-minco {CURRENT_MINCO_VERSION}")));
}

#[test]
fn plugin_add_plans_then_applies_cargo_and_selection_edits() {
    let (_temporary, root) = create_application();
    let cargo_path = root.join("Cargo.toml");
    let cargo = fs::read_to_string(&cargo_path)
        .expect("workspace Cargo.toml")
        .replace(
            &format!("minco = {{ version = \"{CURRENT_MINCO_VERSION}\", features = ["),
            &format!(
                "minco = {{ version = \"{CURRENT_MINCO_VERSION}\", default-features = false, features = ["
            ),
        );
    fs::write(&cargo_path, cargo).expect("disable Minco default features");

    let manifest_path = root.join("minco.toml");
    let mut manifest: toml::Value =
        toml::from_str(&fs::read_to_string(&manifest_path).expect("minco.toml"))
            .expect("valid minco.toml");
    manifest["plugins"]["enabled"] = toml::Value::Array(vec![
        toml::Value::String("idempotency".into()),
        toml::Value::String("observability".into()),
    ]);
    manifest["plugins"]["disabled"] =
        toml::Value::Array(vec![toml::Value::String("health".into())]);
    fs::write(
        &manifest_path,
        toml::to_string_pretty(&manifest).expect("render minco.toml"),
    )
    .expect("disable health selection");
    let before = snapshot(&root);

    let dry_run = run_at(
        &root,
        &["plugin", "add", "minco-plugin-health", "--dry-run"],
    );
    assert!(
        dry_run.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    assert_eq!(snapshot(&root), before);
    let plan: Value = serde_json::from_slice(&dry_run.stdout).expect("plugin add dry-run plan");
    assert_eq!(
        plan["changes"]
            .as_array()
            .expect("planned changes")
            .iter()
            .map(|change| change["path"].as_str().expect("change path"))
            .collect::<Vec<_>>(),
        ["Cargo.toml", "minco.toml"]
    );

    let applied = run_at(&root, &["plugin", "add", "minco-plugin-health"]);
    assert!(
        applied.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let plan: Value = serde_json::from_slice(&applied.stdout).expect("applied plugin add plan");
    assert_eq!(plan["applied"], true);

    let cargo: toml::Value =
        toml::from_str(&fs::read_to_string(&cargo_path).expect("updated workspace Cargo.toml"))
            .expect("valid updated Cargo.toml");
    let features = cargo["workspace"]["dependencies"]["minco"]["features"]
        .as_array()
        .expect("Minco features");
    assert!(
        features
            .iter()
            .any(|feature| feature.as_str() == Some("plugin-health"))
    );
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(&manifest_path).expect("updated minco.toml"))
            .expect("valid updated minco.toml");
    assert!(
        manifest["plugins"]["enabled"]
            .as_array()
            .expect("enabled plugins")
            .iter()
            .any(|plugin| plugin.as_str() == Some("health"))
    );
    assert!(
        !manifest["plugins"]["disabled"]
            .as_array()
            .expect("disabled plugins")
            .iter()
            .any(|plugin| plugin.as_str() == Some("health"))
    );
}

#[test]
fn plugin_explain_exposes_the_complete_static_decision_surface() {
    let output = run(&["plugin", "explain", "feedback"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let explanation: Value =
        serde_json::from_slice(&output.stdout).expect("plugin explanation JSON");
    assert_eq!(explanation["schema_version"], 1);
    assert_eq!(explanation["plugin"]["id"], "feedback");
    assert_eq!(explanation["plugin"]["crate"], "minco-plugin-feedback");
    for section in [
        "capabilities",
        "dependencies",
        "operations",
        "migrations",
        "data_classes",
        "resources",
        "cost",
        "configuration",
        "conformance",
    ] {
        assert!(
            !explanation[section].is_null(),
            "missing explanation section {section}"
        );
    }
    assert!(
        explanation["dependencies"]
            .as_array()
            .expect("plugin dependencies")
            .iter()
            .any(|dependency| dependency == "health")
    );
    assert_eq!(explanation["conformance"]["profile"], "minco-plugin-v1");
    assert!(
        explanation["conformance"]["evidence"]
            .as_array()
            .expect("inert conformance evidence")
            .iter()
            .all(Value::is_string)
    );
}

#[test]
fn explain_traces_every_orders_operation_to_the_dynamodb_adapter() {
    for operation in [
        "placeOrder",
        "listOrders",
        "getOrder",
        "updateOrder",
        "deleteOrder",
    ] {
        let output = run(&["explain", operation]);
        assert!(
            output.status.success(),
            "{operation} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let explanation: Value =
            serde_json::from_slice(&output.stdout).expect("operation explanation JSON");
        assert!(
            explanation["adapters"]
                .as_array()
                .expect("operation adapters")
                .iter()
                .any(|adapter| adapter == "dynamodb"),
            "{operation} must expose the DynamoDB adapter"
        );
        assert!(
            explanation["tests"]
                .as_array()
                .expect("operation tests")
                .iter()
                .any(|test| {
                    test.as_str().is_some_and(|test| {
                        test.starts_with("examples/orders/adapters/tests/dynamodb.rs#")
                    })
                }),
            "{operation} must expose DynamoDB conformance evidence"
        );
    }
}

#[test]
fn plugin_doctor_proves_catalog_version_selection_and_static_registration() {
    let first = run(&["plugin", "doctor"]);
    let second = run(&["plugin", "doctor"]);

    assert!(
        first.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&first.stderr),
        String::from_utf8_lossy(&first.stdout)
    );
    assert_eq!(first.stdout, second.stdout);
    let report: Value = serde_json::from_slice(&first.stdout).expect("plugin doctor JSON");
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["status"], "passed");
    assert_eq!(report["resolved_minco_version"], CURRENT_MINCO_VERSION);
    assert_eq!(report["composition_root"], "crates/minco/src/lib.rs");
    let checks = report["checks"].as_array().expect("doctor checks");
    for code in [
        "catalog.valid",
        "distribution.compatible",
        "selection.known",
        "cargo.version_exact",
        "cargo.feature_selected",
        "composition.static",
    ] {
        assert!(
            checks
                .iter()
                .any(|check| check["code"] == code && check["status"] == "passed"),
            "missing passed doctor check {code}"
        );
    }
}

#[test]
fn plugin_init_adopts_reviewed_local_metadata_through_a_dry_run_first() {
    let (_temporary, root) = create_application();
    let package = root.join("plugins/minco-plugin-metrics");
    fs::create_dir_all(package.join("src")).expect("local plugin directory");
    fs::write(
        package.join("Cargo.toml"),
        r#"[package]
name = "minco-plugin-metrics"
version = "0.3.1"
description = "Reviewed metrics plugin."
include = ["src/**", "Cargo.toml", "minco-plugin.json"]

[package.metadata.minco]
plugin = "minco-plugin.json"
"#,
    )
    .expect("local plugin manifest");
    fs::write(package.join("src/lib.rs"), "#![forbid(unsafe_code)]\n")
        .expect("local plugin source");
    fs::write(
        package.join("minco-plugin.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema": 1,
            "id": "metrics",
            "kind": "plugin",
            "plugin_version": "0.3.1",
            "core_compatibility": "^1.0.0",
            "stability": "experimental",
            "default_enabled": false,
            "feature": "plugin-metrics",
            "runtimes": ["native"],
            "retention": "none",
            "failure_policy": {
                "mode": "fail_closed",
                "description": "Metrics failures remain explicit."
            },
            "documentation": {"reference": "https://docs.rs/minco-plugin-metrics"},
            "conformance": {
                "profile": "minco-plugin-v1",
                "evidence": ["cargo test -p minco-plugin-metrics --locked"]
            }
        }))
        .expect("distribution JSON"),
    )
    .expect("distribution record");
    let cargo_path = root.join("Cargo.toml");
    let current_cargo = fs::read_to_string(&cargo_path).expect("workspace Cargo.toml");
    fs::write(
        &cargo_path,
        current_cargo.replace(
            &format!("version = \"{CURRENT_MINCO_VERSION}\""),
            "version = \"0.5.0\"",
        ),
    )
    .expect("set older Minco dependency");
    let incompatible_before = snapshot(&root);
    let incompatible = run_at(
        &root,
        &[
            "plugin",
            "init",
            "plugins/minco-plugin-metrics",
            "--dry-run",
        ],
    );
    assert!(!incompatible.status.success());
    assert_eq!(snapshot(&root), incompatible_before);
    assert!(
        String::from_utf8(incompatible.stderr)
            .expect("UTF-8 version mismatch")
            .contains("application Minco version 0.5.0")
    );
    fs::write(&cargo_path, current_cargo).expect("restore current Minco dependency");
    let before = snapshot(&root);

    let dry_run = run_at(
        &root,
        &[
            "plugin",
            "init",
            "plugins/minco-plugin-metrics",
            "--dry-run",
        ],
    );
    assert!(
        dry_run.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    assert_eq!(snapshot(&root), before);
    let plan: Value = serde_json::from_slice(&dry_run.stdout).expect("plugin init dry-run plan");
    assert_eq!(plan["operation"], "init");
    assert_eq!(plan["plugin"]["id"], "metrics");
    assert_eq!(plan["plugin"]["resolved_version"], "0.3.1");
    assert_eq!(plan["changes"][0]["path"], "plugins/catalog.toml");

    let applied = run_at(&root, &["plugin", "init", "plugins/minco-plugin-metrics"]);
    assert!(
        applied.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let catalog: toml::Value = toml::from_str(
        &fs::read_to_string(root.join("plugins/catalog.toml")).expect("updated catalog"),
    )
    .expect("valid updated catalog");
    let metrics = catalog["plugin"]
        .as_array()
        .expect("catalog entries")
        .iter()
        .find(|plugin| plugin["id"].as_str() == Some("metrics"))
        .expect("initialized metrics plugin");
    assert_eq!(metrics["crate"].as_str(), Some("minco-plugin-metrics"));
    assert_eq!(
        metrics["path"].as_str(),
        Some("plugins/minco-plugin-metrics")
    );
}

#[test]
fn plugin_remove_reports_operations_migrations_and_data_before_refusing() {
    let cargo_before = fs::read(workspace_root().join("Cargo.toml")).expect("workspace Cargo.toml");
    let manifest_before = fs::read(workspace_root().join("minco.toml")).expect("minco.toml");

    let dry_run = run(&["plugin", "remove", "feedback", "--dry-run"]);
    assert!(
        dry_run.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    assert_eq!(
        fs::read(workspace_root().join("Cargo.toml")).expect("workspace Cargo.toml"),
        cargo_before
    );
    assert_eq!(
        fs::read(workspace_root().join("minco.toml")).expect("minco.toml"),
        manifest_before
    );
    let plan: Value = serde_json::from_slice(&dry_run.stdout).expect("plugin removal plan");
    assert_eq!(plan["operation"], "remove");
    assert_eq!(plan["safe"], false);
    assert_eq!(plan["applied"], false);
    let blockers = plan["blockers"].as_array().expect("removal blockers");
    for kind in ["application_operation", "migration", "data_class"] {
        assert!(
            blockers.iter().any(|blocker| blocker["kind"] == kind),
            "missing blocker kind {kind}"
        );
    }

    let apply = run(&["plugin", "remove", "feedback"]);
    assert!(!apply.status.success());
    assert!(
        String::from_utf8(apply.stderr)
            .expect("UTF-8 blocked removal")
            .contains("plugin feedback cannot be removed safely")
    );
    assert_eq!(
        fs::read(workspace_root().join("Cargo.toml")).expect("workspace Cargo.toml"),
        cargo_before
    );
    assert_eq!(
        fs::read(workspace_root().join("minco.toml")).expect("minco.toml"),
        manifest_before
    );
}

#[test]
fn plugin_test_can_run_one_reviewed_local_package() {
    let output = run(&["plugin", "test", "health"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reports: Value = serde_json::from_slice(&output.stdout).expect("conformance JSON");
    let reports = reports.as_array().expect("conformance reports");
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0]["plugin_id"], "health");
    assert_eq!(reports[0]["status"], "passed");
    assert_eq!(reports[0]["assurance"]["provider_live"], "not_run");
}

#[test]
fn legacy_plugin_mutations_also_offer_json_dry_runs() {
    let cases: &[(&[&str], &str, &str)] = &[
        (
            &["plugin", "enable", "health", "--dry-run"],
            "operation",
            "enable",
        ),
        (
            &["plugin", "disable", "health", "--dry-run"],
            "operation",
            "disable",
        ),
        (
            &["plugin", "new", "metrics", "--dry-run"],
            "generator",
            "plugin",
        ),
    ];

    for (arguments, key, expected) in cases {
        let (_temporary, root) = create_application();
        let before = snapshot(&root);
        let output = run_at(&root, arguments);
        assert!(
            output.status.success(),
            "{arguments:?} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            snapshot(&root),
            before,
            "{arguments:?} wrote during dry-run"
        );
        let plan: Value = serde_json::from_slice(&output.stdout).expect("mutation dry-run JSON");
        assert_eq!(plan[*key], *expected, "unexpected plan for {arguments:?}");
        assert_eq!(plan["dry_run"], true);
        assert_eq!(plan["applied"], false);
    }
}

#[test]
fn plugin_add_refuses_to_invent_a_constructor_for_an_app_owned_plugin() {
    let (_temporary, root) = create_application();
    let generated = run_at(&root, &["plugin", "new", "metrics"]);
    assert!(
        generated.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let before = snapshot(&root);

    let output = run_at(&root, &["plugin", "add", "metrics", "--dry-run"]);

    assert!(!output.status.success());
    assert_eq!(snapshot(&root), before);
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 registration error");
    assert!(stderr.contains("plugin-metrics is not a Minco facade feature"));
    assert!(stderr.contains("register its typed constructor explicitly"));

    let enable = run_at(&root, &["plugin", "enable", "metrics", "--dry-run"]);
    assert!(
        enable.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&enable.stderr)
    );
    assert_eq!(snapshot(&root), before);
    let plan: Value = serde_json::from_slice(&enable.stdout).expect("selection-only enable plan");
    assert_eq!(plan["operation"], "enable");
    assert_eq!(
        plan["registration"]["strategy"],
        "application_explicit_registration"
    );
    assert_eq!(plan["registration"]["verified"], false);
}

#[test]
fn plugin_doctor_fails_closed_for_a_selected_unverified_constructor() {
    let (_temporary, root) = create_application();
    let generated = run_at(&root, &["plugin", "new", "metrics"]);
    assert!(generated.status.success());
    let manifest_path = root.join("minco.toml");
    let mut manifest: toml::Value =
        toml::from_str(&fs::read_to_string(&manifest_path).expect("minco.toml"))
            .expect("valid minco.toml");
    manifest["plugins"]["enabled"]
        .as_array_mut()
        .expect("enabled plugins")
        .push(toml::Value::String("metrics".into()));
    fs::write(
        &manifest_path,
        toml::to_string_pretty(&manifest).expect("render minco.toml"),
    )
    .expect("select app-owned plugin");

    let output = run_at(&root, &["plugin", "doctor"]);

    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).expect("failed doctor JSON");
    assert_eq!(report["status"], "failed");
    let composition = report["checks"]
        .as_array()
        .expect("doctor checks")
        .iter()
        .find(|check| check["code"] == "composition.static")
        .expect("static composition check");
    assert_eq!(composition["status"], "failed");
    assert!(
        composition["findings"]
            .as_array()
            .expect("composition findings")
            .iter()
            .any(|finding| finding.as_str().is_some_and(
                |finding| finding.contains("plugin-metrics is not a Minco facade feature")
            ))
    );
}

#[test]
fn plugin_doctor_detects_enabled_plugins_missing_their_cargo_feature() {
    let (_temporary, root) = create_application();
    let cargo_path = root.join("Cargo.toml");
    let cargo = fs::read_to_string(&cargo_path)
        .expect("workspace Cargo.toml")
        .replace(
            &format!("minco = {{ version = \"{CURRENT_MINCO_VERSION}\", features = ["),
            &format!(
                "minco = {{ version = \"{CURRENT_MINCO_VERSION}\", default-features = false, features = ["
            ),
        );
    fs::write(&cargo_path, cargo).expect("disable facade default features");

    let output = run_at(&root, &["plugin", "doctor"]);

    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).expect("failed doctor JSON");
    let cargo = report["checks"]
        .as_array()
        .expect("doctor checks")
        .iter()
        .find(|check| check["code"] == "cargo.feature_selected")
        .expect("Cargo feature check");
    assert_eq!(cargo["status"], "failed");
    assert!(
        cargo["findings"]
            .as_array()
            .expect("Cargo feature findings")
            .iter()
            .any(|finding| finding
                .as_str()
                .is_some_and(|finding| finding.contains("plugin-health")))
    );
}

#[test]
fn plugin_doctor_rejects_contradictory_plugin_selection() {
    let (_temporary, root) = create_application();
    let manifest_path = root.join("minco.toml");
    let mut manifest: toml::Value =
        toml::from_str(&fs::read_to_string(&manifest_path).expect("minco.toml"))
            .expect("valid minco.toml");
    manifest["plugins"]["disabled"]
        .as_array_mut()
        .expect("disabled plugins")
        .push(toml::Value::String("health".into()));
    fs::write(
        &manifest_path,
        toml::to_string_pretty(&manifest).expect("render contradictory selection"),
    )
    .expect("write contradictory selection");

    let output = run_at(&root, &["plugin", "doctor"]);

    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).expect("failed doctor JSON");
    let selection = report["checks"]
        .as_array()
        .expect("doctor checks")
        .iter()
        .find(|check| check["code"] == "selection.known")
        .expect("selection check");
    assert_eq!(selection["status"], "failed");
    assert!(
        selection["findings"]
            .as_array()
            .expect("selection findings")
            .iter()
            .any(|finding| finding
                .as_str()
                .is_some_and(|finding| finding.contains("both enabled and disabled: health")))
    );
}

#[test]
fn plugin_remove_applies_only_after_empty_ownership_metadata_is_available() {
    let (_temporary, root) = create_application();
    let package = root.join("plugins/minco-plugin-health");
    fs::create_dir_all(package.join("src")).expect("local health package");
    fs::write(
        package.join("Cargo.toml"),
        format!(
            r#"[package]
name = "minco-plugin-health"
version = "{CURRENT_MINCO_VERSION}"
include = ["src/**", "Cargo.toml", "minco-plugin.json"]

[package.metadata.minco]
plugin = "minco-plugin.json"
"#
        ),
    )
    .expect("health Cargo.toml");
    fs::write(package.join("src/lib.rs"), "#![forbid(unsafe_code)]\n").expect("health source");
    fs::write(
        package.join("minco-plugin.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema": 1,
            "id": "health",
            "kind": "plugin",
            "plugin_version": "1.0.0",
            "core_compatibility": "^1.0.0",
            "stability": "stable",
            "default_enabled": true,
            "feature": "plugin-health",
            "runtimes": ["native"],
            "retention": "none",
            "failure_policy": {
                "mode": "fail_closed",
                "description": "Health failures remain explicit."
            },
            "documentation": {"reference": "https://docs.rs/minco-plugin-health"},
            "resources": [{
                "id": "health-state",
                "kind": "s3_bucket",
                "idle_cost": "storage_only"
            }],
            "conformance": {
                "profile": "minco-plugin-v1",
                "evidence": ["cargo test -p minco-plugin-health --locked"]
            }
        }))
        .expect("health distribution JSON"),
    )
    .expect("health distribution record");
    let catalog_path = root.join("plugins/catalog.toml");
    let mut catalog: toml::Value =
        toml::from_str(&fs::read_to_string(&catalog_path).expect("plugin catalog"))
            .expect("valid plugin catalog");
    let health = catalog["plugin"]
        .as_array_mut()
        .expect("catalog entries")
        .iter_mut()
        .find(|plugin| plugin["id"].as_str() == Some("health"))
        .expect("health catalog entry");
    health.as_table_mut().expect("health catalog table").insert(
        "path".into(),
        toml::Value::String("plugins/minco-plugin-health".into()),
    );
    fs::write(
        &catalog_path,
        toml::to_string_pretty(&catalog).expect("render plugin catalog"),
    )
    .expect("point health at local metadata");
    let cargo_path = root.join("Cargo.toml");
    let cargo = fs::read_to_string(&cargo_path)
        .expect("workspace Cargo.toml")
        .replace(
            &format!("minco = {{ version = \"{CURRENT_MINCO_VERSION}\", features = ["),
            &format!(
                "minco = {{ version = \"{CURRENT_MINCO_VERSION}\", default-features = false, features = [\"plugin-health\", "
            ),
    );
    fs::write(&cargo_path, cargo).expect("select explicit health feature");

    let blocked = run_at(&root, &["plugin", "remove", "health", "--dry-run"]);
    assert!(blocked.status.success());
    let blocked: Value =
        serde_json::from_slice(&blocked.stdout).expect("resource-blocked removal plan");
    assert_eq!(blocked["safe"], false);
    assert!(
        blocked["blockers"]
            .as_array()
            .expect("resource blockers")
            .iter()
            .any(|blocker| blocker["kind"] == "resource" && blocker["id"] == "health-state")
    );
    let distribution_path = package.join("minco-plugin.json");
    let mut distribution: Value = serde_json::from_slice(
        &fs::read(&distribution_path).expect("resource-bearing distribution"),
    )
    .expect("valid resource-bearing distribution");
    distribution["resources"] = serde_json::json!([]);
    fs::write(
        &distribution_path,
        serde_json::to_vec_pretty(&distribution).expect("resource-free distribution"),
    )
    .expect("prove no retained resource ownership");

    let output = run_at(&root, &["plugin", "remove", "health"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: Value = serde_json::from_slice(&output.stdout).expect("safe removal plan");
    assert_eq!(plan["safe"], true);
    assert_eq!(plan["applied"], true);
    let cargo: toml::Value =
        toml::from_str(&fs::read_to_string(&cargo_path).expect("updated Cargo.toml"))
            .expect("valid updated Cargo.toml");
    assert!(
        !cargo["workspace"]["dependencies"]["minco"]["features"]
            .as_array()
            .expect("Minco features")
            .iter()
            .any(|feature| feature.as_str() == Some("plugin-health"))
    );
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("minco.toml")).expect("updated minco.toml"))
            .expect("valid updated minco.toml");
    assert!(
        manifest["plugins"]["disabled"]
            .as_array()
            .expect("disabled plugins")
            .iter()
            .any(|plugin| plugin.as_str() == Some("health"))
    );
}

#[test]
fn plugin_list_preserves_static_distribution_coordinates() {
    let output = run(&["plugin", "list"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body: Value = serde_json::from_slice(&output.stdout).expect("plugin list JSON");
    let plugins = body["catalog"]["plugin"]
        .as_array()
        .expect("plugin catalog entries");

    let health = plugins
        .iter()
        .find(|plugin| plugin["id"] == "health")
        .expect("health plugin");
    assert_eq!(health["kind"], "plugin");
    assert_eq!(health["feature"], "plugin-health");

    let lambda = plugins
        .iter()
        .find(|plugin| plugin["id"] == "aws-lambda")
        .expect("AWS Lambda runtime");
    assert_eq!(lambda["kind"], "runtime");
    assert_eq!(lambda["feature"], "aws-lambda");
}

#[test]
fn plugin_list_reads_archive_visible_distribution_metadata_without_loading_plugin_code() {
    let output = run(&["plugin", "list"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body: Value = serde_json::from_slice(&output.stdout).expect("plugin list JSON");
    let health = body["catalog"]["plugin"]
        .as_array()
        .expect("plugin catalog entries")
        .iter()
        .find(|plugin| plugin["id"] == "health")
        .expect("health plugin");

    assert_eq!(health["distribution"]["schema"], 1);
    assert_eq!(health["distribution"]["plugin_version"], "1.0.0");
    assert_eq!(
        health["distribution"]["core_compatibility"],
        format!("^{CURRENT_MINCO_VERSION}")
    );
    assert_eq!(
        health["distribution"]["runtimes"],
        serde_json::json!(["native"])
    );
    assert_eq!(
        health["distribution"]["failure_policy"]["mode"],
        "fail_closed"
    );
    assert_eq!(
        health["distribution"]["documentation"]["reference"],
        "https://docs.rs/minco-plugin-health"
    );
    assert_eq!(
        health["distribution"]["conformance"]["profile"],
        "minco-plugin-v1"
    );
}

#[test]
fn every_official_catalog_entry_has_a_valid_distribution_record() {
    let output = run(&["plugin", "validate"]);

    assert!(
        output.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let findings: Value = serde_json::from_slice(&output.stdout).expect("validation JSON");
    assert_eq!(findings, serde_json::json!([]));
}

#[test]
fn plugin_test_all_uses_the_public_offline_conformance_boundary() {
    let output = run(&["plugin", "test", "--all"]);

    assert!(
        output.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let reports: Value = serde_json::from_slice(&output.stdout).expect("conformance JSON");
    let reports = reports.as_array().expect("conformance reports");
    assert_eq!(reports.len(), 18);
    assert!(
        reports
            .iter()
            .any(|report| report["plugin_id"] == "aws-dynamodb")
    );
    assert!(
        reports
            .iter()
            .any(|report| report["plugin_id"] == "realtime")
    );
    for report in reports {
        assert_eq!(report["status"], "passed", "{report:#}");
        assert_eq!(report["assurance"]["application_readiness"], "not_assessed");
        assert_eq!(report["assurance"]["provider_live"], "not_run");
        assert_eq!(report["assurance"]["production_readiness"], "not_assessed");
    }
}
