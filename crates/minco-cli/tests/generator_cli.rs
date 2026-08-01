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

fn run(root: &Path, arguments: &[&str]) -> Output {
    let mut command = Command::new(cargo_minco());
    command.args([
        "--root",
        root.to_str().expect("UTF-8 application root"),
        "--json",
    ]);
    command.args(arguments);
    command.output().expect("run cargo minco generator")
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
fn operation_dry_run_is_deterministic_and_does_not_write() {
    let (_temporary, root) = create_application();
    let before = snapshot(&root);

    let first = run(&root, &["make", "operation", "getPlatform", "--dry-run"]);
    let second = run(&root, &["make", "operation", "getPlatform", "--dry-run"]);

    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(snapshot(&root), before);

    let plan: Value = serde_json::from_slice(&first.stdout).expect("JSON generation plan");
    let serialized = String::from_utf8(first.stdout).expect("UTF-8 JSON plan");
    for content in ["panic!", "TODO(getPlatform)", "DATABASE_URL", "source ="] {
        assert!(
            !serialized.contains(content),
            "dry-run plan leaked generated or sensitive content: {content}"
        );
    }
    assert_eq!(plan["schema_version"], 1);
    assert_eq!(plan["generator"], "operation");
    assert_eq!(plan["name"], "getPlatform");
    assert_eq!(plan["dry_run"], true);
    assert_eq!(plan["contract"]["operation_id"], "getPlatform");
    assert_eq!(plan["contract"]["method"], "get");
    assert_eq!(plan["contract"]["path"], "/platform");
    assert_eq!(
        plan["changes"]
            .as_array()
            .expect("ordered change list")
            .iter()
            .map(|change| change["path"].as_str().expect("change path"))
            .collect::<Vec<_>>(),
        vec![
            "crates/api/tests/get_platform.rs",
            "crates/application/tests/get_platform.rs",
            "docs/generated/operations/get_platform.md",
            "minco.toml",
        ]
    );
}

#[test]
fn plugin_generator_keeps_the_generated_application_inspectable() {
    let (_temporary, root) = create_application();

    let generated = run(&root, &["make", "plugin", "metrics"]);
    assert!(
        generated.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let package = root.join("plugins/minco-plugin-metrics");
    let cargo = fs::read_to_string(package.join("Cargo.toml")).expect("plugin Cargo manifest");
    assert!(cargo.contains("[package.metadata.minco]"));
    assert!(cargo.contains("plugin = \"minco-plugin.json\""));
    let distribution: Value = serde_json::from_slice(
        &fs::read(package.join("minco-plugin.json")).expect("distribution record"),
    )
    .expect("valid distribution JSON");
    assert_eq!(distribution["id"], "metrics");
    assert_eq!(distribution["feature"], "plugin-metrics");

    let inspect = run(&root, &["inspect"]);
    assert!(
        inspect.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    let inspected: Value = serde_json::from_slice(&inspect.stdout).expect("inspect JSON");
    let metrics = inspected["plugins"]
        .as_array()
        .expect("plugins")
        .iter()
        .find(|plugin| plugin["id"] == "metrics")
        .expect("generated plugin");
    assert_eq!(metrics["kind"], "plugin");
    assert_eq!(metrics["distribution"]["schema"], 1);

    let validate = run(&root, &["plugin", "validate"]);
    assert!(
        validate.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&validate.stderr),
        String::from_utf8_lossy(&validate.stdout)
    );
}

#[test]
fn operation_generation_rejects_unknown_contract_ids_before_writing() {
    let (_temporary, root) = create_application();
    let before = snapshot(&root);

    let output = run(
        &root,
        &["make", "operation", "missingOperation", "--dry-run"],
    );

    assert!(!output.status.success());
    assert_eq!(snapshot(&root), before);
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 generator error");
    assert!(stderr.contains("operationId missingOperation is not present"));
    assert!(stderr.contains("add and review the OpenAPI operation first"));
}

#[test]
fn resource_dry_run_selects_the_complete_reviewed_contract_family() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let output = run(root, &["make", "resource", "order", "--dry-run"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: Value = serde_json::from_slice(&output.stdout).expect("JSON generation plan");
    assert_eq!(plan["generator"], "resource");
    assert_eq!(plan["name"], "order");
    assert_eq!(plan["resource"]["name"], "order");
    assert_eq!(
        plan["resource"]["operations"]
            .as_array()
            .expect("resource operations")
            .iter()
            .map(|operation| operation["action"].as_str().expect("resource action"))
            .collect::<Vec<_>>(),
        vec!["create", "list", "read", "update", "delete"]
    );
    assert_eq!(
        plan["changes"]
            .as_array()
            .expect("resource changes")
            .iter()
            .filter(|change| change["format"] == "rust")
            .count(),
        10
    );
}

#[test]
fn resource_generation_rejects_an_incomplete_family_before_writing() {
    let (_temporary, root) = create_application();
    let before = snapshot(&root);
    let output = run(&root, &["make", "resource", "order", "--dry-run"]);

    assert!(!output.status.success());
    assert_eq!(snapshot(&root), before);
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 generator error");
    assert!(stderr.contains("not a complete reviewed contract family"));
    for action in ["create", "list", "read", "update", "delete"] {
        assert!(stderr.contains(action), "missing {action} evidence");
    }
}

#[test]
fn operation_apply_creates_failing_specs_and_refuses_a_second_write() {
    let (_temporary, root) = create_application();

    let output = run(&root, &["make", "operation", "getPlatform"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: Value = serde_json::from_slice(&output.stdout).expect("JSON applied plan");
    assert_eq!(plan["dry_run"], false);
    assert_eq!(plan["applied"], true);
    let pre_write_plan = String::from_utf8(output.stderr).expect("UTF-8 pre-write generation plan");
    assert!(pre_write_plan.contains("Generation plan (before write):"));
    assert!(pre_write_plan.contains("\"applied\": false"));
    assert!(pre_write_plan.contains("\"path\": \"minco.toml\""));

    for relative in [
        "crates/application/tests/get_platform.rs",
        "crates/api/tests/get_platform.rs",
    ] {
        let source = fs::read_to_string(root.join(relative)).expect("generated failing test");
        assert!(source.contains("panic!(\"TODO(getPlatform):"));
    }
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("minco.toml")).expect("minco.toml"))
            .expect("valid TOML");
    assert_eq!(
        manifest["operations"]["getPlatform"]["tests"]
            .as_array()
            .expect("test traces")
            .iter()
            .filter_map(toml::Value::as_str)
            .filter(|path| path.ends_with("tests/get_platform.rs"))
            .count(),
        2
    );

    let after = snapshot(&root);
    let repeated = run(&root, &["make", "operation", "getPlatform"]);
    assert!(!repeated.status.success());
    assert_eq!(snapshot(&root), after);
    assert!(
        String::from_utf8(repeated.stderr)
            .expect("UTF-8 overwrite error")
            .contains("refuses to overwrite existing path")
    );
}

#[test]
fn every_generator_exposes_a_json_dry_run_without_writing() {
    let cases: &[(&[&str], &str, &str)] = &[
        (
            &["make", "module", "billing", "--dry-run"],
            "module",
            "billing",
        ),
        (
            &["make", "migration", "add-widgets", "--dry-run"],
            "migration",
            "add-widgets",
        ),
        (
            &["make", "seeder", "sample-widgets", "--dry-run"],
            "seeder",
            "sample-widgets",
        ),
        (
            &["make", "worker", "email-dispatch", "--dry-run"],
            "worker",
            "email-dispatch",
        ),
        (
            &["make", "adapter", "widget-store", "--dry-run"],
            "adapter",
            "widget-store",
        ),
        (
            &["make", "test", "getPlatform", "--dry-run"],
            "test",
            "getPlatform",
        ),
        (
            &["make", "plugin", "metrics", "--dry-run"],
            "plugin",
            "metrics",
        ),
        (&["stubs", "publish", "--dry-run"], "stubs", "defaults"),
    ];

    for (arguments, generator, name) in cases {
        let (_temporary, root) = create_application();
        let before = snapshot(&root);
        let output = run(&root, arguments);
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
        let plan: Value = serde_json::from_slice(&output.stdout).expect("JSON generation plan");
        assert_eq!(plan["generator"], *generator);
        assert_eq!(plan["name"], *name);
        assert_eq!(plan["dry_run"], true);
        assert_eq!(plan["applied"], false);
        assert!(
            !plan["changes"].as_array().expect("change list").is_empty(),
            "{arguments:?} planned no work"
        );
    }
}

#[test]
fn published_app_owned_stub_is_used_without_changing_framework_defaults() {
    let (_temporary, root) = create_application();
    let publish = run(&root, &["stubs", "publish"]);
    assert!(
        publish.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&publish.stderr)
    );

    let custom = root.join("stubs/minco/application-test.rs.tmpl");
    fs::write(
        &custom,
        "//! app-owned marker\n#[test]\nfn {{SNAKE_NAME}}_custom_spec() {\n    panic!(\"TODO({{OPERATION_ID}}): app-owned\");\n}\n",
    )
    .expect("customize published stub");

    let output = run(&root, &["make", "operation", "getPlatform"]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rendered = fs::read_to_string(root.join("crates/application/tests/get_platform.rs"))
        .expect("rendered app-owned stub");
    assert!(rendered.contains("//! app-owned marker"));
    assert!(rendered.contains("fn get_platform_custom_spec()"));
    assert!(rendered.contains("TODO(getPlatform): app-owned"));
}

#[test]
fn names_and_symlinked_output_ancestors_are_rejected_before_writing() {
    let (_temporary, root) = create_application();
    let before = snapshot(&root);

    let invalid = run(&root, &["make", "module", "../escape", "--dry-run"]);
    assert!(!invalid.status.success());
    assert_eq!(snapshot(&root), before);
    assert!(
        String::from_utf8(invalid.stderr)
            .expect("UTF-8 invalid-name error")
            .contains("lower-kebab-case ASCII")
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let outside = tempfile::tempdir().expect("outside directory");
        let modules = root.join("crates/domain/src/modules");
        symlink(outside.path(), &modules).expect("symlink output ancestor");
        let linked_before = snapshot(&root);
        let linked = run(&root, &["make", "module", "billing", "--dry-run"]);
        assert!(!linked.status.success());
        assert_eq!(snapshot(&root), linked_before);
        assert!(
            String::from_utf8(linked.stderr)
                .expect("UTF-8 symlink error")
                .contains("refuses symlinked path component")
        );
        assert!(
            fs::read_dir(outside.path())
                .expect("read outside directory")
                .next()
                .is_none()
        );
    }
}
