use serde_json::Value;
use std::{path::Path, process::Command};

const fn cargo_minco() -> &'static str {
    env!("CARGO_BIN_EXE_cargo-minco")
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}

fn run(arguments: &[&str]) -> std::process::Output {
    let mut command = Command::new(cargo_minco());
    command.args([
        "--root",
        workspace_root().to_str().expect("UTF-8 workspace root"),
        "--json",
    ]);
    command.args(arguments);
    command.output().expect("run cargo minco plugin command")
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
    assert_eq!(health["distribution"]["core_compatibility"], "^0.5.0");
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
