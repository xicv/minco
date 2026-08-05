use minco_project_view::load_project_view;
use minco_workbench::{ExportFormat, ExportRequest, export_project_view};
use serde_json::Value;
use std::{fs, path::Path};

fn packaged_project_fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/project")
        .canonicalize()
        .expect("canonical packaged project fixture")
}

#[test]
fn json_export_publishes_a_complete_view_to_a_new_relative_destination() {
    let root = packaged_project_fixture();
    let view = load_project_view(&root).expect("packaged fixture ProjectView");
    let target = root.join("target");
    fs::create_dir_all(&target).expect("target directory");
    let sandbox = tempfile::tempdir_in(&target).expect("export sandbox");
    let destination = sandbox
        .path()
        .join("workbench-json")
        .strip_prefix(&root)
        .expect("project-relative destination")
        .to_path_buf();
    let canonical_inputs = view
        .provenance
        .iter()
        .map(|source| source.path.clone())
        .collect::<Vec<_>>();

    let report = export_project_view(
        &view,
        ExportRequest {
            root: &root,
            destination: &destination,
            canonical_inputs: &canonical_inputs,
            format: ExportFormat::Json,
        },
    )
    .expect("safe JSON export");

    assert_eq!(report.destination, destination);
    assert_eq!(report.files, vec!["project-view.json"]);
    let exported: Value = serde_json::from_slice(
        &fs::read(root.join(&destination).join("project-view.json"))
            .expect("exported project view"),
    )
    .expect("valid exported JSON");
    assert_eq!(exported["schema_version"], 1);
    assert_eq!(
        exported["project"]["source_digest"],
        view.project.source_digest
    );
}

#[test]
fn mermaid_export_is_deterministic_and_uses_opaque_node_identifiers() {
    let root = packaged_project_fixture();
    let view = load_project_view(&root).expect("packaged fixture ProjectView");
    let target = root.join("target");
    fs::create_dir_all(&target).expect("target directory");
    let sandbox = tempfile::tempdir_in(&target).expect("export sandbox");
    let canonical_inputs = view
        .provenance
        .iter()
        .map(|source| source.path.clone())
        .collect::<Vec<_>>();
    let export = |name: &str| {
        let destination = sandbox
            .path()
            .join(name)
            .strip_prefix(&root)
            .expect("project-relative destination")
            .to_path_buf();
        export_project_view(
            &view,
            ExportRequest {
                root: &root,
                destination: &destination,
                canonical_inputs: &canonical_inputs,
                format: ExportFormat::Mermaid,
            },
        )
        .expect("safe Mermaid export");
        fs::read_to_string(root.join(destination).join("project-view.mmd"))
            .expect("exported Mermaid")
    };

    let first = export("workbench-mermaid-a");
    let second = export("workbench-mermaid-b");
    assert_eq!(first, second);
    assert!(first.starts_with("flowchart TD\n"));
    assert!(first.contains("n0["));
    assert!(first.contains("-->|contains|"));
    assert!(!first.contains("project:root["));
}

#[test]
fn static_export_contains_accessible_local_only_assets_and_the_same_project_view() {
    let root = packaged_project_fixture();
    let view = load_project_view(&root).expect("packaged fixture ProjectView");
    let target = root.join("target");
    fs::create_dir_all(&target).expect("target directory");
    let sandbox = tempfile::tempdir_in(&target).expect("export sandbox");
    let destination = sandbox
        .path()
        .join("workbench-static")
        .strip_prefix(&root)
        .expect("project-relative destination")
        .to_path_buf();
    let canonical_inputs = view
        .provenance
        .iter()
        .map(|source| source.path.clone())
        .collect::<Vec<_>>();

    let report = export_project_view(
        &view,
        ExportRequest {
            root: &root,
            destination: &destination,
            canonical_inputs: &canonical_inputs,
            format: ExportFormat::Static,
        },
    )
    .expect("safe static export");

    assert_eq!(
        report.files,
        vec![
            "index.html",
            "project-view.json",
            "project-view.mmd",
            "workbench.css",
            "workbench.js",
        ]
    );
    let output = root.join(destination);
    let html = fs::read_to_string(output.join("index.html")).expect("workbench HTML");
    let css = fs::read_to_string(output.join("workbench.css")).expect("workbench CSS");
    let javascript = fs::read_to_string(output.join("workbench.js")).expect("workbench JavaScript");
    let exported: Value = serde_json::from_slice(
        &fs::read(output.join("project-view.json")).expect("exported ProjectView"),
    )
    .expect("valid ProjectView JSON");

    assert!(html.contains("<main id=\"main-content\""));
    assert!(html.contains("aria-label=\"Workbench views\""));
    assert!(html.contains("Read aloud"));
    assert!(css.contains("@media (max-width: 720px)"));
    assert!(javascript.contains("speechSynthesis"));
    assert!(javascript.contains("textContent"));
    assert!(!javascript.contains("innerHTML"));
    assert_eq!(
        exported["project"]["source_digest"],
        view.project.source_digest
    );
}

#[cfg(unix)]
#[test]
fn export_rejects_a_symlinked_destination_ancestor_without_writing_through_it() {
    use std::os::unix::fs::symlink;

    let root = packaged_project_fixture();
    let view = load_project_view(&root).expect("packaged fixture ProjectView");
    let target = root.join("target");
    fs::create_dir_all(&target).expect("target directory");
    let sandbox = tempfile::tempdir_in(&target).expect("export sandbox");
    let real_parent = sandbox.path().join("real-parent");
    fs::create_dir(&real_parent).expect("real destination parent");
    let linked_parent = sandbox.path().join("linked-parent");
    symlink(&real_parent, &linked_parent).expect("symlinked destination ancestor");
    let destination = linked_parent
        .join("workbench")
        .strip_prefix(&root)
        .expect("project-relative destination")
        .to_path_buf();
    let canonical_inputs = view
        .provenance
        .iter()
        .map(|source| source.path.clone())
        .collect::<Vec<_>>();

    let error = export_project_view(
        &view,
        ExportRequest {
            root: &root,
            destination: &destination,
            canonical_inputs: &canonical_inputs,
            format: ExportFormat::Json,
        },
    )
    .expect_err("symlinked ancestor must fail closed");

    assert!(error.to_string().contains("without symlinks"));
    assert!(!real_parent.join("workbench").exists());
    assert!(
        fs::read_dir(&real_parent)
            .expect("real parent entries")
            .next()
            .is_none()
    );
}

#[test]
fn export_rejects_a_destination_inside_a_canonical_input_root() {
    let root = packaged_project_fixture();
    let view = load_project_view(&root).expect("packaged fixture ProjectView");
    let destination = Path::new("tasks/workbench-export-test");
    let canonical_inputs = vec![Path::new("tasks").to_path_buf()];

    let error = export_project_view(
        &view,
        ExportRequest {
            root: &root,
            destination,
            canonical_inputs: &canonical_inputs,
            format: ExportFormat::Json,
        },
    )
    .expect_err("canonical input overlap must fail closed");

    assert!(error.to_string().contains("overlaps canonical input tasks"));
    assert!(!root.join(destination).exists());
}
