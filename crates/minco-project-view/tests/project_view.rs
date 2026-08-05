use minco_project_view::{
    EvidenceLane, SemanticStatus, ViewLimits, load_project_view, load_project_view_with_limits,
};
use sha2::{Digest, Sha256};
use std::{fmt::Write as _, fs, path::Path};

fn write(root: &Path, relative: &str, source: &str) {
    let destination = root.join(relative);
    fs::create_dir_all(destination.parent().expect("fixture parent")).expect("create parent");
    fs::write(destination, source).expect("write fixture");
}

fn fixture() -> tempfile::TempDir {
    let fixture = tempfile::tempdir().expect("fixture root");
    write(
        fixture.path(),
        "minco.toml",
        r#"schema = 1
name = "fixture"
contract = "openapi.yaml"
generated = "generated.rs"
deployment_config = "deployment.toml"
roadmap = "roadmap.yaml"
tasks = "tasks"
plugin_catalog = "plugins.toml"
quality = "quality.toml"

[configuration]
root = "config"
default_file = "default.toml"
local_override = ".local.toml"
environment_prefix = "FIXTURE__"

[[configuration.fields]]
key = "application.name"
kind = "string"
required = true
secret = false
description = "Application name"
default = "fixture"

[architecture]
domain_roots = ["domain"]
application_roots = ["application"]
api_roots = ["api"]

[plugins]
enabled = ["health", "feedback"]
disabled = []

[migrations]
roots = []

[seeds]
roots = []

[operations.createFeedback]
contract = "plugins/feedback/openapi.yaml"
handler = "plugins/feedback/src/http.rs#create_feedback"
application = "plugins/feedback/src/service.rs#create"
adapters = ["memory"]
tests = ["plugins/feedback/src/http.rs"]
"#,
    );
    write(
        fixture.path(),
        "openapi.yaml",
        r"openapi: 3.1.0
info:
  title: Fixture API
  version: 1.0.0
paths:
  /health:
    get:
      operationId: healthLive
      responses:
        '200':
          description: healthy
",
    );
    write(
        fixture.path(),
        "deployment.toml",
        r#"schema_version = 1
application = "fixture"
environment = "local"
region = "local"
runtime = "local_native"
ingress = "local_tcp"
allowed_origins = ["http://127.0.0.1:5173"]
allowed_headers = ["content-type"]
log_retention_days = 1
scheduled_wakeups = []
uses_nat_gateway = false

[auth]
kind = "development_headers"

[database]
kind = "sqlite_persistent_host"
host_monthly_usd = 0.0
backup_monthly_usd = 0.0

[[functions]]
name = "api"
artifact_path = "target/debug/fixture"
memory_mb = 128
timeout_seconds = 5
reserved_concurrency = 1
provisioned_concurrency = 0
database_connections_per_instance = 1
"#,
    );
    write(fixture.path(), "generated.rs", "// generated fixture\n");
    write(
        fixture.path(),
        "roadmap.yaml",
        r"schema: 1
product: Fixture
milestones:
  - id: M1
    name: Foundation
    status: active
    depends_on: []
    outcome: A bounded fixture
    exit_criteria: []
",
    );
    write(
        fixture.path(),
        "tasks/M1/M1-T01.md",
        r#"---
id: M1-T01
title: Build the fixture
milestone: M1
status: ready
priority: high
area: test
depends_on: []
operations: [healthLive]
owned_paths: ["api/**"]
checks: ["cargo test"]
---

Fixture task.
"#,
    );
    write(
        fixture.path(),
        "plugins.toml",
        r#"schema = 1

[[plugin]]
id = "health"
crate = "fixture-health"
kind = "plugin"
feature = "plugin-health"
default_enabled = true
stability = "stable"
description = "Fixture health capability."

[[plugin]]
id = "feedback"
crate = "fixture-feedback"
path = "plugins/feedback"
kind = "plugin"
feature = "plugin-feedback"
default_enabled = false
stability = "stable"
description = "Fixture feedback capability."
"#,
    );
    write(
        fixture.path(),
        "plugins/feedback/openapi.yaml",
        r"openapi: 3.1.0
info:
  title: Fixture Feedback API
  version: 1.0.0
paths:
  /feedback:
    post:
      operationId: createFeedback
      responses:
        '201':
          description: created
",
    );
    write(fixture.path(), "quality.toml", "schema = 1\n");
    write(
        fixture.path(),
        "verification/static-validation.json",
        r#"{"schema_version":1,"status":"ok","errors":0,"warnings":0}"#,
    );
    for directory in ["config", "domain", "application", "api", "plugins/health"] {
        fs::create_dir_all(fixture.path().join(directory)).expect("fixture directory");
    }
    fixture
}

#[test]
fn source_digest_and_source_lane_cover_every_file_that_was_read() {
    let fixture = fixture();
    let canonical = fixture.path().canonicalize().expect("canonical fixture");

    let view = load_project_view(&canonical).expect("bounded project view");
    let mut canonical_provenance = String::new();
    for source in &view.provenance {
        writeln!(
            &mut canonical_provenance,
            "{}\0{}",
            source.path.display(),
            source.sha256
        )
        .expect("writing to a String is infallible");
    }
    let expected_digest = format!("{:x}", Sha256::digest(canonical_provenance));

    assert_eq!(view.project.source_digest, expected_digest);
    assert_eq!(
        view.evidence[&EvidenceLane::Source].len(),
        view.provenance.len()
    );
}

#[test]
fn includes_plugin_owned_operations_from_only_their_declared_contracts() {
    let fixture = fixture();
    let canonical = fixture.path().canonicalize().expect("canonical fixture");

    let view = load_project_view(&canonical).expect("bounded project view");

    assert!(view.operation("createFeedback").is_some());
    assert_eq!(view.feedback.operation_ids, ["createFeedback"]);
    assert!(
        view.provenance
            .iter()
            .any(|source| { source.path == Path::new("plugins/feedback/openapi.yaml") })
    );
}

#[test]
fn applies_the_text_limit_to_configuration_projection_with_a_diagnostic() {
    let fixture = fixture();
    let canonical = fixture.path().canonicalize().expect("canonical fixture");
    let limits = ViewLimits {
        max_text_bytes: 8,
        ..ViewLimits::default()
    };

    let view = load_project_view_with_limits(&canonical, limits).expect("bounded project view");

    assert!(
        view.configuration
            .iter()
            .all(|field| field.description.len() <= limits.max_text_bytes)
    );
    assert!(
        view.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "PROJECT_VIEW_TEXT_TRUNCATED")
    );
}

#[test]
fn projects_raw_repository_state_without_merging_evidence_lanes() {
    let fixture = fixture();
    let canonical = fixture.path().canonicalize().expect("canonical fixture");

    let view = load_project_view(&canonical).expect("bounded project view");

    assert_eq!(view.schema_version, 1);
    assert_eq!(view.project.name, "fixture");
    let registry_plugin = view
        .nodes
        .iter()
        .find(|node| node.id == "feature:health")
        .expect("registry plugin node");
    assert_eq!(registry_plugin.properties["path"], serde_json::Value::Null);
    let workspace_plugin = view
        .nodes
        .iter()
        .find(|node| node.id == "feature:feedback")
        .expect("workspace plugin node");
    assert_eq!(
        workspace_plugin.properties["path"],
        serde_json::json!("plugins/feedback")
    );
    let task = view
        .nodes
        .iter()
        .find(|node| node.id == "task:M1-T01")
        .expect("task node");
    assert_eq!(task.raw_status.as_deref(), Some("ready"));
    assert_eq!(task.semantic_status, Some(SemanticStatus::NotStarted));
    assert!(view.evidence.contains_key(&EvidenceLane::Source));
    assert!(view.evidence.contains_key(&EvidenceLane::LocalVerification));
    assert!(
        view.evidence
            .contains_key(&EvidenceLane::HostedVerification)
    );
    assert!(view.evidence.contains_key(&EvidenceLane::Deployment));
    assert!(view.evidence.contains_key(&EvidenceLane::Runtime));
    assert!(view.evidence.contains_key(&EvidenceLane::Review));
    assert!(view.summary.derived);
    assert_eq!(view.summary.ready_task_ids, ["M1-T01"]);
    assert!(
        view.provenance
            .iter()
            .all(|source| !source.path.is_absolute())
    );
}

#[cfg(unix)]
#[test]
fn rejects_a_dangling_symlink_at_an_optional_evidence_boundary() {
    let fixture = fixture();
    let evidence = fixture.path().join("verification/static-validation.json");
    fs::remove_file(&evidence).expect("remove evidence fixture");
    std::os::unix::fs::symlink(fixture.path().join("missing-outside-file"), &evidence)
        .expect("dangling evidence symlink");
    let canonical = fixture.path().canonicalize().expect("canonical fixture");

    let error = load_project_view(&canonical).expect_err("dangling symlink must fail closed");

    assert!(matches!(
        error,
        minco_project_view::ProjectViewError::SymbolicLink(_)
    ));
}

#[test]
fn directory_scans_are_bounded_even_when_entries_do_not_match_the_requested_extension() {
    let fixture = fixture();
    for index in 0..32 {
        write(
            fixture.path(),
            &format!("tasks/M1/ignored-{index:02}.txt"),
            "ignored",
        );
    }
    let canonical = fixture.path().canonicalize().expect("canonical fixture");
    let limits = ViewLimits {
        max_files: 20,
        ..ViewLimits::default()
    };

    let error = load_project_view_with_limits(&canonical, limits)
        .expect_err("directory entry budget must be enforced");

    assert!(matches!(
        error,
        minco_project_view::ProjectViewError::LimitExceeded {
            limit_name: "max_files",
            ..
        }
    ));
}

#[test]
fn secret_defaults_are_redacted_from_the_serialized_view() {
    let fixture = fixture();
    let manifest_path = fixture.path().join("minco.toml");
    let manifest = fs::read_to_string(&manifest_path).expect("read fixture manifest");
    fs::write(
        &manifest_path,
        manifest.replace(
            "[architecture]",
            r#"[[configuration.fields]]
key = "provider.token"
kind = "string"
required = true
secret = true
description = "Provider token"
default = "must-never-appear"

[architecture]"#,
        ),
    )
    .expect("write secret fixture");
    let canonical = fixture.path().canonicalize().expect("canonical fixture");

    let view = load_project_view(&canonical).expect("bounded project view");
    let serialized = serde_json::to_string(&view).expect("serialize ProjectView");

    assert!(!serialized.contains("must-never-appear"));
    assert!(serialized.contains("provider.token"));
    assert!(serialized.contains("redacted"));
}
