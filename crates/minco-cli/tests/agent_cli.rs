use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::TempDir;

const fn cargo_minco() -> &'static str {
    env!("CARGO_BIN_EXE_cargo-minco")
}

fn project() -> TempDir {
    let root = tempfile::tempdir().expect("temporary Minco project");
    fs::write(
        root.path().join("minco.toml"),
        r#"schema = 1
name = "agent-fixture"
contract = "openapi.yaml"
generated = "generated"
deployment_config = "deploy.toml"
roadmap = "roadmap.yaml"
tasks = "tasks"
plugin_catalog = "plugins.toml"
quality = "quality.toml"
"#,
    )
    .expect("write minimal Minco manifest");
    root
}

fn run(root: &Path, arguments: &[&str]) -> Output {
    let mut command = Command::new(cargo_minco());
    command.args([
        "--root",
        root.to_str().expect("UTF-8 fixture root"),
        "--json",
        "agent",
    ]);
    command.args(arguments);
    command.output().expect("run cargo minco agent command")
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "invalid JSON output: {error}; stdout: {}; stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn successful_json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    json(output)
}

fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn visit(root: &Path, current: &Path, output: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in fs::read_dir(current).expect("read fixture directory") {
            let entry = entry.expect("fixture entry");
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).expect("fixture metadata");
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                visit(root, &path, output);
            } else if metadata.is_file() {
                output.push((
                    path.strip_prefix(root)
                        .expect("fixture-relative path")
                        .to_path_buf(),
                    fs::read(path).expect("read fixture file"),
                ));
            }
        }
    }

    let mut output = Vec::new();
    visit(root, root, &mut output);
    output.sort_by(|left, right| left.0.cmp(&right.0));
    output
}

fn plan(root: &Path, target: &str) -> Value {
    successful_json(&run(root, &["plan", "--target", target]))
}

fn plan_digest(plan: &Value) -> &str {
    plan["plan_digest"].as_str().expect("plan digest")
}

fn digest(contents: &[u8]) -> String {
    format!("{:x}", Sha256::digest(contents))
}

#[test]
fn agent_plan_is_deterministic_complete_and_read_only() {
    let root = project();
    let before = snapshot(root.path());

    let first = run(root.path(), &["plan", "--target", "all"]);
    let second = run(root.path(), &["plan", "--target", "all"]);

    assert!(first.status.success());
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(snapshot(root.path()), before);
    let plan = json(&first);
    assert_eq!(plan["schema_version"], 1);
    assert_eq!(plan["operation"], "plan");
    assert_eq!(plan["target"], "all");
    assert_eq!(plan["minco_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(plan["safe"], true);
    assert_eq!(plan["conflicts"], serde_json::json!([]));
    assert_eq!(
        plan["actions"].as_array().expect("planned actions").len(),
        49
    );
    assert!(
        plan["actions"]
            .as_array()
            .expect("planned actions")
            .iter()
            .all(|action| action["action"] == "create")
    );
    assert_eq!(
        plan["manual_actions"]
            .as_array()
            .expect("manual actions")
            .iter()
            .map(|action| action["client"].as_str().expect("manual client"))
            .collect::<Vec<_>>(),
        ["claude", "codex"]
    );
    assert_eq!(plan_digest(&plan).len(), 64);
}

#[test]
fn sync_requires_the_exact_current_plan_digest_and_is_repeatable() {
    let root = project();
    let before = snapshot(root.path());

    let missing = run(root.path(), &["sync", "--target", "all"]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("--expect-plan-digest"));
    assert_eq!(snapshot(root.path()), before);

    let initial = plan(root.path(), "all");
    let stale = run(
        root.path(),
        &[
            "sync",
            "--target",
            "all",
            "--expect-plan-digest",
            &"0".repeat(64),
        ],
    );
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("stale agent plan digest"));
    assert_eq!(snapshot(root.path()), before);

    let applied = successful_json(&run(
        root.path(),
        &[
            "sync",
            "--target",
            "all",
            "--expect-plan-digest",
            plan_digest(&initial),
        ],
    ));
    assert_eq!(applied["operation"], "sync");
    assert_eq!(applied["applied"], true);
    assert_eq!(applied["writes"], 49);

    let manifest: Value = serde_json::from_slice(
        &fs::read(root.path().join(".minco/agent-manifest.json"))
            .expect("generated ownership manifest"),
    )
    .expect("valid ownership manifest");
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(
        manifest["files"].as_array().expect("managed files").len(),
        48
    );

    let unchanged = plan(root.path(), "all");
    assert!(
        unchanged["actions"]
            .as_array()
            .expect("planned actions")
            .iter()
            .all(|action| action["action"] == "unchanged")
    );
    let before_repeat = snapshot(root.path());
    let repeated = successful_json(&run(
        root.path(),
        &[
            "sync",
            "--target",
            "all",
            "--expect-plan-digest",
            plan_digest(&unchanged),
        ],
    ));
    assert_eq!(repeated["writes"], 0);
    assert_eq!(snapshot(root.path()), before_repeat);
}

#[test]
fn user_owned_and_edited_managed_files_are_never_overwritten() {
    let user_owned = project();
    let destination = user_owned
        .path()
        .join(".agents/skills/minco-review/SKILL.md");
    fs::create_dir_all(destination.parent().expect("destination parent"))
        .expect("create user-owned parent");
    fs::write(&destination, "user owned\n").expect("write user-owned file");
    let before = snapshot(user_owned.path());
    let conflict = plan(user_owned.path(), "codex");
    assert_eq!(conflict["safe"], false);
    assert!(
        conflict["conflicts"]
            .as_array()
            .expect("conflicts")
            .iter()
            .any(|item| item["code"] == "user_owned_destination")
    );
    let refused = run(
        user_owned.path(),
        &[
            "sync",
            "--target",
            "codex",
            "--expect-plan-digest",
            plan_digest(&conflict),
        ],
    );
    assert!(!refused.status.success());
    assert_eq!(snapshot(user_owned.path()), before);

    let managed = project();
    let initial = plan(managed.path(), "codex");
    successful_json(&run(
        managed.path(),
        &[
            "sync",
            "--target",
            "codex",
            "--expect-plan-digest",
            plan_digest(&initial),
        ],
    ));
    let destination = managed.path().join(".agents/skills/minco-review/SKILL.md");
    fs::write(&destination, "locally edited managed file\n").expect("edit managed file");
    let before = snapshot(managed.path());
    let conflict = plan(managed.path(), "codex");
    assert_eq!(conflict["safe"], false);
    assert!(
        conflict["conflicts"]
            .as_array()
            .expect("conflicts")
            .iter()
            .any(|item| item["code"] == "managed_file_modified")
    );
    let refused = run(
        managed.path(),
        &[
            "sync",
            "--target",
            "codex",
            "--expect-plan-digest",
            plan_digest(&conflict),
        ],
    );
    assert!(!refused.status.success());
    assert_eq!(snapshot(managed.path()), before);
}

#[test]
fn a_missing_managed_file_is_reported_as_drift_and_not_recreated() {
    let root = project();
    let initial = plan(root.path(), "codex");
    successful_json(&run(
        root.path(),
        &[
            "sync",
            "--target",
            "codex",
            "--expect-plan-digest",
            plan_digest(&initial),
        ],
    ));
    let destination = root.path().join(".agents/skills/minco-review/SKILL.md");
    fs::remove_file(&destination).expect("remove managed projection");

    let drift = plan(root.path(), "codex");
    assert_eq!(drift["safe"], false);
    assert!(
        drift["conflicts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|conflict| conflict["code"] == "managed_file_missing")
    );
    let refused = run(
        root.path(),
        &[
            "sync",
            "--target",
            "codex",
            "--expect-plan-digest",
            plan_digest(&drift),
        ],
    );
    assert!(!refused.status.success());
    assert!(!destination.exists());
}

#[test]
fn owned_outdated_files_are_replaced_without_deleting_unmanaged_neighbors() {
    let root = project();
    let initial = plan(root.path(), "codex");
    successful_json(&run(
        root.path(),
        &[
            "sync",
            "--target",
            "codex",
            "--expect-plan-digest",
            plan_digest(&initial),
        ],
    ));
    let destination = root.path().join(".agents/skills/minco-review/SKILL.md");
    let expected = fs::read(&destination).expect("current bundled skill");
    let old = b"old Minco-managed skill\n";
    fs::write(&destination, old).expect("install an older managed projection");
    let neighbor = destination.parent().unwrap().join("NOTES.md");
    fs::write(&neighbor, "user notes\n").expect("unmanaged neighbor");

    let manifest_path = root.path().join(".minco/agent-manifest.json");
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("generated ownership manifest"))
            .expect("valid ownership manifest");
    let record = manifest["files"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["path"] == ".agents/skills/minco-review/SKILL.md")
        .expect("review skill ownership record");
    record["sha256"] = Value::String(digest(old));
    let mut rendered = serde_json::to_vec_pretty(&manifest).expect("render older manifest");
    rendered.push(b'\n');
    fs::write(&manifest_path, rendered).expect("record older managed digest");

    let update = plan(root.path(), "codex");
    assert_eq!(update["safe"], true);
    assert!(update["actions"].as_array().unwrap().iter().any(|action| {
        action["path"] == ".agents/skills/minco-review/SKILL.md" && action["action"] == "update"
    }));
    successful_json(&run(
        root.path(),
        &[
            "sync",
            "--target",
            "codex",
            "--expect-plan-digest",
            plan_digest(&update),
        ],
    ));
    assert_eq!(fs::read(destination).expect("updated skill"), expected);
    assert_eq!(
        fs::read_to_string(neighbor).expect("unmanaged neighbor remains"),
        "user notes\n"
    );
}

#[test]
fn selecting_the_second_client_preserves_the_first_projection_and_ownership() {
    let root = project();
    let codex = plan(root.path(), "codex");
    successful_json(&run(
        root.path(),
        &[
            "sync",
            "--target",
            "codex",
            "--expect-plan-digest",
            plan_digest(&codex),
        ],
    ));
    let codex_skill = root.path().join(".agents/skills/minco-operation/SKILL.md");
    let before = fs::read(&codex_skill).expect("Codex projection");

    let claude = plan(root.path(), "claude");
    successful_json(&run(
        root.path(),
        &[
            "sync",
            "--target",
            "claude",
            "--expect-plan-digest",
            plan_digest(&claude),
        ],
    ));

    assert_eq!(
        fs::read(codex_skill).expect("preserved Codex skill"),
        before
    );
    assert!(
        root.path()
            .join(".claude/skills/minco-operation/SKILL.md")
            .is_file()
    );
    let canonical = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/agent/skills/minco-operation/SKILL.md"),
    )
    .expect("canonical operation skill");
    assert_eq!(
        fs::read(root.path().join(".agents/skills/minco-operation/SKILL.md"))
            .expect("Codex operation skill"),
        canonical
    );
    assert_eq!(
        fs::read(root.path().join(".claude/skills/minco-operation/SKILL.md"))
            .expect("Claude operation skill"),
        canonical
    );
    let manifest: Value = serde_json::from_slice(
        &fs::read(root.path().join(".minco/agent-manifest.json"))
            .expect("combined ownership manifest"),
    )
    .expect("valid ownership manifest");
    assert_eq!(manifest["files"].as_array().unwrap().len(), 48);
}

#[cfg(unix)]
#[test]
fn symlinked_projection_paths_fail_closed_without_touching_the_target() {
    use std::os::unix::fs::symlink;

    let root = project();
    let outside = tempfile::tempdir().expect("outside directory");
    fs::write(outside.path().join("sentinel"), "unchanged\n").expect("outside sentinel");
    symlink(outside.path(), root.path().join(".agents")).expect("projection symlink");

    let plan = plan(root.path(), "codex");
    assert_eq!(plan["safe"], false);
    assert!(
        plan["conflicts"]
            .as_array()
            .expect("conflicts")
            .iter()
            .any(|item| item["code"] == "unsafe_path_entry")
    );
    let sync = run(
        root.path(),
        &[
            "sync",
            "--target",
            "codex",
            "--expect-plan-digest",
            plan_digest(&plan),
        ],
    );
    assert!(!sync.status.success());
    assert_eq!(
        fs::read_to_string(outside.path().join("sentinel")).expect("outside sentinel"),
        "unchanged\n"
    );
    assert!(!outside.path().join("skills").exists());
}

#[test]
fn codex_and_claude_targets_are_symmetric_and_doctor_is_read_only() {
    let codex = project();
    let claude = project();
    let codex_plan = plan(codex.path(), "codex");
    let claude_plan = plan(claude.path(), "claude");
    assert_eq!(codex_plan["actions"].as_array().unwrap().len(), 25);
    assert_eq!(claude_plan["actions"].as_array().unwrap().len(), 25);
    assert!(
        codex_plan["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action["path"] == ".agents/skills/minco-operation/SKILL.md")
    );
    assert!(
        claude_plan["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action["path"] == ".claude/skills/minco-operation/SKILL.md")
    );

    let before = snapshot(codex.path());
    let first = run(codex.path(), &["doctor"]);
    let second = run(codex.path(), &["doctor"]);
    assert!(first.status.success());
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(snapshot(codex.path()), before);
    let report = json(&first);
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["operation"], "doctor");
    assert_eq!(report["writes"], 0);
    assert_eq!(report["status"], "not_installed");
    assert_eq!(report["discovery"]["manifest"], "absent");
    assert_eq!(report["discovery"]["codex"], "absent");
    assert_eq!(report["discovery"]["claude"], "absent");
    assert_eq!(report["projection"]["target"], "all");
    assert_eq!(report["mcp"]["configured"], Value::Null);
    assert_eq!(report["mcp"]["status"], "unknown");
}
