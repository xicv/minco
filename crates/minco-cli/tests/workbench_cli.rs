use serde_json::Value;
use std::{fs, path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    time::timeout,
};

fn repository_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repository root")
}

#[tokio::test]
async fn check_reports_a_non_serving_read_only_workbench_surface() {
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .args(["workbench", "--check", "--json"])
        .current_dir(repository_root())
        .output()
        .await
        .expect("run workbench check");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("workbench check JSON");
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["status"], "ok");
    assert_eq!(report["mode"], "check");
    assert_eq!(report["read_only"], true);
    assert_eq!(report["listening_sockets"], 0);
    assert_eq!(report["writes"], 0);
    assert_eq!(report["project"]["name"], "minco-framework");
    assert!(report["summary"]["node_count"].as_u64().is_some());
}

#[tokio::test]
async fn static_export_uses_the_explicit_new_project_relative_destination() {
    let root = repository_root();
    let target = root.join("target");
    fs::create_dir_all(&target).expect("target directory");
    let sandbox = tempfile::tempdir_in(&target).expect("export sandbox");
    let destination = sandbox
        .path()
        .join("workbench")
        .strip_prefix(&root)
        .expect("project-relative destination")
        .to_path_buf();
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .arg("--root")
        .arg(&root)
        .arg("--json")
        .args(["workbench", "export", "--format", "static", "--output"])
        .arg(&destination)
        .output()
        .await
        .expect("run workbench static export");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("workbench export JSON");
    assert_eq!(report["status"], "ok");
    assert_eq!(report["format"], "static");
    assert_eq!(
        report["destination"],
        destination.to_string_lossy().as_ref()
    );
    assert!(root.join(&destination).join("index.html").is_file());
    assert!(root.join(&destination).join("project-view.json").is_file());
}

#[tokio::test]
async fn serve_reports_its_exact_loopback_origin_before_accepting_requests() {
    let root = repository_root();
    let mut child = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .arg("--root")
        .arg(&root)
        .arg("--json")
        .args(["workbench", "serve", "--port", "0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn workbench server");
    let stdout = child.stdout.take().expect("workbench stdout");
    let mut lines = BufReader::new(stdout).lines();
    let startup = timeout(Duration::from_secs(10), lines.next_line())
        .await
        .expect("workbench startup timeout")
        .expect("read workbench startup")
        .expect("workbench startup line");
    let startup: Value = serde_json::from_str(&startup).expect("workbench startup JSON");
    assert_eq!(startup["status"], "serving");
    assert_eq!(startup["read_only"], true);
    assert_eq!(startup["loopback"], true);
    let origin = startup["origin"].as_str().expect("served origin");
    assert!(origin.starts_with("http://127.0.0.1:"));

    let response = reqwest::get(format!("{origin}/project-view.json"))
        .await
        .expect("served ProjectView response");
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    child.kill().await.expect("stop workbench server");
    let _ = child.wait().await;
}

#[tokio::test]
async fn serving_requires_an_explicit_canonical_project_root() {
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .args(["workbench", "serve", "--port", "0"])
        .current_dir(repository_root())
        .output()
        .await
        .expect("run workbench serve without root");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("requires an explicit canonical project root via --root")
    );
    assert!(output.stdout.is_empty());
}
