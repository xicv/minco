use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
};

fn repository_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("cargo-minco lives under the repository crates directory")
}

fn run_dev(arguments: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-minco"));
    command.args([
        "--root",
        repository_root().to_str().expect("UTF-8 repository path"),
        "--json",
        "dev",
        "--dry-run",
    ]);
    command.args(arguments);
    command.output().expect("run cargo minco dev")
}

#[test]
fn default_dry_run_is_a_complete_local_only_plan() {
    let output = run_dev(&[]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: Value = serde_json::from_slice(&output.stdout).expect("JSON development plan");
    assert_eq!(plan["schema_version"], 1);
    assert_eq!(plan["application"], "minco-orders");
    assert_eq!(plan["environment"], "local");
    assert_eq!(plan["profile"], "postgres");
    assert_eq!(plan["external_aws_contact"], false);
    assert_eq!(plan["services"][0]["kind"], "postgres");
    assert_eq!(plan["services"][0]["port"], 55_432);
    assert_eq!(plan["services"][1]["kind"], "rustack");
    assert_eq!(plan["services"][1]["port"], 4_566);
    assert_eq!(
        plan["services"][1]["aws_services"],
        serde_json::json!(["ssm", "sts"])
    );
    assert_eq!(plan["lifecycle"][0]["kind"], "migrate");
    assert_eq!(plan["processes"][0]["id"], "api");
    assert_eq!(plan["processes"][0]["role"], "api");
    assert_eq!(plan["omitted_schedule_ids"], serde_json::json!([]));

    let serialized = String::from_utf8(output.stdout).expect("UTF-8 development plan");
    for sensitive in [
        "AWS_SECRET_ACCESS_KEY",
        "DATABASE_URL",
        "MIGRATION_DATABASE_URL",
        "postgres://",
    ] {
        assert!(
            !serialized.contains(sensitive),
            "dry-run leaked sensitive field {sensitive}"
        );
    }
}
