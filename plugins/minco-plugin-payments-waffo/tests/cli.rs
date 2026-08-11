use serde_json::Value;
use std::{fs, process::Command};

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_minco-waffo"))
}

fn stderr_json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stderr).expect("CLI stderr must be one JSON failure envelope")
}

#[test]
fn help_and_version_are_successful_human_output() {
    for flag in ["--help", "--version"] {
        let output = binary().arg(flag).output().unwrap();
        assert!(output.status.success());
        assert!(!output.stdout.is_empty());
        assert!(serde_json::from_slice::<Value>(&output.stdout).is_err());
    }
}

#[test]
fn clap_failures_are_stable_schema_one_json() {
    for arguments in [
        vec!["--unknown"],
        vec!["checkout", "--product-id", "PROD_0123456789ABCDEFGHIJKL"],
        vec![
            "checkout",
            "--product-id",
            "PROD_0123456789ABCDEFGHIJKL",
            "--idempotency-key",
            "key",
            "--metadata",
            "missing-separator",
        ],
    ] {
        let output = binary().args(arguments).output().unwrap();
        assert!(!output.status.success());
        let failure = stderr_json(&output);
        assert_eq!(failure["schema"], 1);
        assert_eq!(failure["ok"], false);
        assert_eq!(failure["error"]["code"], "minco_waffo.arguments");
        assert_eq!(
            failure["error"]["message"],
            "invalid command-line arguments"
        );
    }

    let compact = binary().args(["--compact", "--unknown"]).output().unwrap();
    assert!(!compact.status.success());
    assert_eq!(compact.stderr.last(), Some(&b'\n'));
    assert!(!compact.stderr[..compact.stderr.len() - 1].contains(&b'\n'));
    assert_eq!(stderr_json(&compact)["schema"], 1);
}

fn test_config(
    directory: &tempfile::TempDir,
    production: bool,
    allow_writes: bool,
) -> std::path::PathBuf {
    let config = directory.path().join("minco.toml");
    let environment = if production { "production" } else { "test" };
    fs::write(
        &config,
        format!(
            "schema = 1\nenvironment_class = \"{environment}\"\n[values.plugins.payments-waffo]\nenvironment = \"{environment}\"\nmerchant_id = \"MER_0123456789ABCDEFGHIJKL\"\nprivate_key = \"env:MINCO_WAFFO_TEST_MISSING_PRIVATE_KEY\"\nallow_production_writes = {allow_writes}\n"
        ),
    )
    .unwrap();
    config
}

#[test]
fn request_validation_and_production_guard_precede_secret_resolution() {
    let directory = tempfile::tempdir().unwrap();
    let config = test_config(&directory, false, false);
    for (product, currency) in [
        ("PROD_short", "AUD"),
        ("PROD_0123456789ABCDEFGHIJKL", "aud"),
    ] {
        let output = binary()
            .arg("--config")
            .arg(&config)
            .args([
                "checkout",
                "--product-id",
                product,
                "--currency",
                currency,
                "--idempotency-key",
                "key",
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let failure = stderr_json(&output);
        assert_eq!(failure["error"]["code"], "waffo.invalid_configuration");
        assert!(
            !String::from_utf8_lossy(&output.stderr)
                .contains("MINCO_WAFFO_TEST_MISSING_PRIVATE_KEY")
        );
    }

    let production = test_config(&directory, true, false);
    let output = binary()
        .arg("--config")
        .arg(production)
        .args([
            "checkout",
            "--product-id",
            "PROD_0123456789ABCDEFGHIJKL",
            "--idempotency-key",
            "key",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        stderr_json(&output)["error"]["code"],
        "waffo.production_writes_disabled"
    );
}

#[test]
fn secret_resolution_failure_is_redacted_and_has_a_stable_exit() {
    let directory = tempfile::tempdir().unwrap();
    let config = test_config(&directory, false, false);
    let output = binary()
        .arg("--config")
        .arg(config)
        .args([
            "checkout",
            "--product-id",
            "PROD_0123456789ABCDEFGHIJKL",
            "--idempotency-key",
            "key",
        ])
        .env_remove("MINCO_WAFFO_TEST_MISSING_PRIVATE_KEY")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        stderr_json(&output)["error"]["code"],
        "waffo.secret_resolution"
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("MINCO_WAFFO_TEST_MISSING_PRIVATE_KEY")
    );
}

#[test]
fn colliding_stdin_sources_fail_without_reading_stdin() {
    let output = binary()
        .args([
            "--config",
            "-",
            "action",
            "--path",
            "/v1/actions/store/add-webhook",
            "--idempotency-key",
            "key",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        stderr_json(&output)["error"]["message"],
        "stdin may be selected by at most one input"
    );
}

#[test]
fn production_raw_action_is_rejected_before_secret_resolution() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("minco.toml");
    let body = directory.path().join("body.json");
    fs::write(
        &config,
        r#"schema = 1
environment_class = "production"
[values.plugins.payments-waffo]
environment = "production"
merchant_id = "MER_0123456789ABCDEFGHIJKL"
private_key = "env:MINCO_WAFFO_TEST_MISSING_PRIVATE_KEY"
allow_production_writes = true
"#,
    )
    .unwrap();
    fs::write(&body, "{}").unwrap();
    let output = binary()
        .arg("--config")
        .arg(&config)
        .args([
            "action",
            "--path",
            "/v1/actions/store/add-webhook",
            "--body",
        ])
        .arg(&body)
        .args(["--idempotency-key", "key"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        stderr_json(&output)["error"]["code"],
        "waffo.generic_production_action_disabled"
    );
}

#[test]
fn mutating_graphql_is_rejected_before_secret_resolution() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("minco.toml");
    let query = directory.path().join("query.graphql");
    fs::write(
        &config,
        r#"schema = 1
environment_class = "test"
[values.plugins.payments-waffo]
merchant_id = "MER_0123456789ABCDEFGHIJKL"
private_key = "env:MINCO_WAFFO_TEST_MISSING_PRIVATE_KEY"
"#,
    )
    .unwrap();
    fs::write(&query, "mutation Unsafe { createStore { id } }").unwrap();
    let output = binary()
        .arg("--config")
        .arg(&config)
        .args(["graphql", "--query"])
        .arg(&query)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        stderr_json(&output)["error"]["code"],
        "waffo.invalid_configuration"
    );
}
