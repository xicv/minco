use minco_test::{ConformanceStatus, PluginConformance};
use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use minco_core::{
    ConfigurationField, ConfigurationValueKind, Plugin, PluginContext, PluginDescriptor,
    PluginError, PluginId,
};

#[derive(Debug)]
struct ExampleService;

#[derive(Debug)]
struct AlternateService;

#[derive(Debug, Clone, Default)]
struct ThirdPartyPlugin;

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct FixtureConfiguration {
    name: String,
}

#[derive(Debug, Clone, Default)]
struct ConfiguredThirdPartyPlugin;

#[derive(Debug, Default)]
struct NondeterministicPlugin {
    alternate: AtomicBool,
}

impl Plugin for ThirdPartyPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("third-party-minimal").expect("static plugin ID"),
            "0.1.0".parse().expect("static plugin version"),
            "Minimal external conformance fixture",
        );
        descriptor.core_compatibility = "^0.5.0".parse().expect("static core compatibility");
        descriptor
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        context.services().insert(Arc::new(ExampleService))?;
        Ok(())
    }
}

impl Plugin for ConfiguredThirdPartyPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = ThirdPartyPlugin.descriptor();
        descriptor.configuration.push(ConfigurationField {
            key: "name".into(),
            kind: ConfigurationValueKind::String,
            required: true,
            secret: false,
            description: "Stable example name.".into(),
            default: None,
        });
        descriptor
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        let configuration = context.configuration::<FixtureConfiguration>()?;
        if configuration.name.is_empty() {
            return Err(PluginError::Installation(
                "fixture name must not be empty".into(),
            ));
        }
        Ok(())
    }
}

impl Plugin for NondeterministicPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        ThirdPartyPlugin.descriptor()
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        if self.alternate.fetch_xor(true, Ordering::SeqCst) {
            context.services().insert(Arc::new(AlternateService))?;
        } else {
            context.services().insert(Arc::new(ExampleService))?;
        }
        Ok(())
    }
}

fn write_minimal_package(root: &std::path::Path) {
    fs::create_dir_all(root.join("src")).expect("plugin source directory");
    fs::write(root.join("src/lib.rs"), "#![forbid(unsafe_code)]\n").expect("plugin source");
    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "third-party-minimal"
version = "0.1.0"
edition = "2024"
include = ["src/**", "Cargo.toml", "minco-plugin.json"]

[package.metadata.minco]
plugin = "minco-plugin.json"
"#,
    )
    .expect("plugin Cargo manifest");
    fs::write(
        root.join("minco-plugin.json"),
        r#"{
  "schema": 1,
  "id": "third-party-minimal",
  "kind": "plugin",
  "plugin_version": "0.1.0",
  "core_compatibility": "^0.5.0",
  "stability": "experimental",
  "default_enabled": false,
  "feature": "plugin-third-party-minimal",
  "runtimes": ["native"],
  "retention": "none",
  "failure_policy": {
    "mode": "fail_closed",
    "description": "Failures remain explicit."
  },
  "documentation": {
    "reference": "https://docs.rs/third-party-minimal"
  },
  "conformance": {
    "profile": "minco-plugin-v1",
    "evidence": ["cargo test --all-features --locked"]
  }
}"#,
    )
    .expect("plugin distribution record");
}

fn mutate_manifest(package: &std::path::Path, mutate: impl FnOnce(&mut serde_json::Value)) {
    let path = package.join("minco-plugin.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("distribution record"))
            .expect("distribution JSON");
    mutate(&mut manifest);
    fs::write(
        path,
        serde_json::to_vec_pretty(&manifest).expect("distribution JSON"),
    )
    .expect("distribution record");
}

#[test]
fn standalone_plugin_package_passes_the_public_offline_contract() {
    let package = tempfile::tempdir().expect("temporary plugin package");
    write_minimal_package(package.path());

    let report = PluginConformance::for_package(package.path()).run();

    assert_eq!(report.status, ConformanceStatus::Passed);
    assert_eq!(report.plugin_id, "third-party-minimal");
    assert!(report.diagnostics.is_empty());
    assert_eq!(
        report.assurance.application_readiness,
        ConformanceStatus::NotAssessed
    );
    assert_eq!(report.assurance.provider_live, ConformanceStatus::NotRun);
    assert_eq!(
        report.assurance.production_readiness,
        ConformanceStatus::NotAssessed
    );
}

#[test]
fn secret_defaults_emit_a_stable_fail_closed_diagnostic() {
    let package = tempfile::tempdir().expect("temporary plugin package");
    write_minimal_package(package.path());
    mutate_manifest(package.path(), |manifest| {
        manifest["configuration"] = serde_json::json!([{
            "key": "api_key",
            "kind": "string",
            "required": true,
            "secret": true,
            "description": "Opaque provider credential.",
            "default": "must-not-ship"
        }]);
    });

    let report = PluginConformance::for_package(package.path()).run();

    assert_eq!(report.status, ConformanceStatus::Failed);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code, "secret_configuration_default");
    assert_eq!(report.diagnostics[0].path, "configuration.api_key.default");
}

#[test]
fn linked_plugin_lifecycle_and_registration_provenance_are_exercised() {
    let package = tempfile::tempdir().expect("temporary plugin package");
    write_minimal_package(package.path());

    let report = PluginConformance::for_package(package.path())
        .with_plugin(ThirdPartyPlugin)
        .run();

    report.assert_passed();
    assert_eq!(report.assurance.plugin_lifecycle, ConformanceStatus::Passed);
}

#[test]
fn lifecycle_registration_provenance_must_be_deterministic() {
    let package = tempfile::tempdir().expect("temporary plugin package");
    write_minimal_package(package.path());

    let report = PluginConformance::for_package(package.path())
        .with_plugin(NondeterministicPlugin::default())
        .run();

    assert_eq!(report.status, ConformanceStatus::Failed);
    assert_eq!(report.assurance.plugin_lifecycle, ConformanceStatus::Failed);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(
        report.diagnostics[0].code,
        "registration_provenance_nondeterministic"
    );
    assert_eq!(report.diagnostics[0].path, "lifecycle.provenance");
}

#[test]
fn descriptor_only_checks_do_not_claim_lifecycle_readiness() {
    let package = tempfile::tempdir().expect("temporary plugin package");
    write_minimal_package(package.path());

    let report = PluginConformance::for_package(package.path())
        .with_descriptor(ThirdPartyPlugin.descriptor())
        .run();

    report.assert_passed();
    assert_eq!(
        report.assurance.plugin_lifecycle,
        ConformanceStatus::NotAssessed
    );
}

#[test]
fn invalid_http_header_names_have_a_stable_operation_diagnostic() {
    let package = tempfile::tempdir().expect("temporary plugin package");
    write_minimal_package(package.path());
    mutate_manifest(package.path(), |manifest| {
        manifest["operations"] = serde_json::json!([{
            "operation_id": "createExample",
            "method": "POST",
            "path": "/examples",
            "public": false,
            "idempotent": false,
            "headers": ["Authorization: injected"]
        }]);
    });

    let report = PluginConformance::for_package(package.path()).run();

    assert_eq!(report.status, ConformanceStatus::Failed);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code, "operation_header_invalid");
    assert_eq!(
        report.diagnostics[0].path,
        "operations.createExample.headers"
    );
}

#[test]
fn provider_neutral_plugins_reject_aws_runtime_dependencies() {
    let package = tempfile::tempdir().expect("temporary plugin package");
    write_minimal_package(package.path());
    let cargo_path = package.path().join("Cargo.toml");
    let cargo = fs::read_to_string(&cargo_path).expect("plugin Cargo manifest");
    fs::write(
        cargo_path,
        format!("{cargo}\n[dependencies]\naws-sdk-s3 = \"1\"\n"),
    )
    .expect("plugin Cargo manifest");

    let report = PluginConformance::for_package(package.path()).run();

    assert_eq!(report.status, ConformanceStatus::Failed);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code, "provider_dependency_leakage");
    assert_eq!(report.diagnostics[0].path, "dependencies.aws-sdk-s3");
}

#[test]
fn provider_leakage_cannot_hide_in_target_specific_dependencies() {
    let package = tempfile::tempdir().expect("temporary plugin package");
    write_minimal_package(package.path());
    let cargo_path = package.path().join("Cargo.toml");
    let cargo = fs::read_to_string(&cargo_path).expect("plugin Cargo manifest");
    fs::write(
        cargo_path,
        format!("{cargo}\n[target.'cfg(unix)'.dependencies]\naws-sdk-s3 = \"1\"\n"),
    )
    .expect("plugin Cargo manifest");

    let report = PluginConformance::for_package(package.path()).run();

    assert_eq!(report.status, ConformanceStatus::Failed);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code, "provider_dependency_leakage");
    assert_eq!(
        report.diagnostics[0].path,
        "target.cfg(unix).dependencies.aws-sdk-s3"
    );
}

#[test]
fn provider_leakage_cannot_hide_behind_a_dependency_alias() {
    let package = tempfile::tempdir().expect("temporary plugin package");
    write_minimal_package(package.path());
    let cargo_path = package.path().join("Cargo.toml");
    let cargo = fs::read_to_string(&cargo_path).expect("plugin Cargo manifest");
    fs::write(
        cargo_path,
        format!(
            "{cargo}\n[dependencies]\nobject-store-client = {{ package = \"aws-sdk-s3\", version = \"1\" }}\n"
        ),
    )
    .expect("plugin Cargo manifest");

    let report = PluginConformance::for_package(package.path()).run();

    assert_eq!(report.status, ConformanceStatus::Failed);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code, "provider_dependency_leakage");
    assert_eq!(
        report.diagnostics[0].path,
        "dependencies.object-store-client"
    );
}

#[cfg(unix)]
#[test]
fn distribution_record_symlinks_fail_closed() {
    use std::os::unix::fs::symlink;

    let package = tempfile::tempdir().expect("temporary plugin package");
    write_minimal_package(package.path());
    let outside = tempfile::NamedTempFile::new().expect("outside distribution record");
    fs::write(
        outside.path(),
        fs::read(package.path().join("minco-plugin.json")).expect("distribution record"),
    )
    .expect("outside distribution record");
    fs::remove_file(package.path().join("minco-plugin.json"))
        .expect("remove package distribution record");
    symlink(outside.path(), package.path().join("minco-plugin.json"))
        .expect("symlink distribution record");

    let report = PluginConformance::for_package(package.path()).run();

    assert_eq!(report.status, ConformanceStatus::Failed);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code, "distribution_not_regular_file");
    assert_eq!(report.diagnostics[0].path, "minco-plugin.json");
}

#[test]
fn configuration_defaults_must_match_the_declared_type() {
    let package = tempfile::tempdir().expect("temporary plugin package");
    write_minimal_package(package.path());
    mutate_manifest(package.path(), |manifest| {
        manifest["configuration"] = serde_json::json!([{
            "key": "enabled",
            "kind": "boolean",
            "required": false,
            "secret": false,
            "description": "Enable the optional behavior.",
            "default": "yes"
        }]);
    });

    let report = PluginConformance::for_package(package.path()).run();

    assert_eq!(report.status, ConformanceStatus::Failed);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(
        report.diagnostics[0].code,
        "configuration_default_type_mismatch"
    );
    assert_eq!(report.diagnostics[0].path, "configuration.enabled.default");
}

#[test]
fn distributions_must_accept_the_current_minco_core_api() {
    let package = tempfile::tempdir().expect("temporary plugin package");
    write_minimal_package(package.path());
    mutate_manifest(package.path(), |manifest| {
        manifest["core_compatibility"] = serde_json::json!(">=99.0.0");
    });

    let report = PluginConformance::for_package(package.path()).run();

    assert_eq!(report.status, ConformanceStatus::Failed);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(
        report.diagnostics[0].code,
        "core_compatibility_excludes_current"
    );
    assert_eq!(report.diagnostics[0].path, "core_compatibility");
}

#[test]
fn linked_http_ownership_must_match_the_distribution_record() {
    let package = tempfile::tempdir().expect("temporary plugin package");
    write_minimal_package(package.path());
    mutate_manifest(package.path(), |manifest| {
        manifest["operations"] = serde_json::json!([{
            "operation_id": "listExamples",
            "method": "GET",
            "path": "/examples",
            "public": true,
            "idempotent": true
        }]);
    });

    let report = PluginConformance::for_package(package.path())
        .with_plugin(ThirdPartyPlugin)
        .run();

    assert_eq!(report.status, ConformanceStatus::Failed);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code, "descriptor_operations_mismatch");
    assert_eq!(report.diagnostics[0].path, "descriptor.operations");
    assert_eq!(
        report.assurance.plugin_lifecycle,
        ConformanceStatus::NotAssessed
    );
}

#[test]
fn resource_dependencies_and_iam_actions_emit_stable_diagnostics() {
    let package = tempfile::tempdir().expect("temporary plugin package");
    write_minimal_package(package.path());
    mutate_manifest(package.path(), |manifest| {
        manifest["resources"] = serde_json::json!([{
            "id": "example-bucket",
            "kind": "s3_bucket",
            "idle_cost": "storage_only",
            "dependencies": ["missing-resource"],
            "iam_actions": ["s3 GetObject"]
        }]);
    });

    let report = PluginConformance::for_package(package.path()).run();

    assert_eq!(report.status, ConformanceStatus::Failed);
    assert_eq!(
        report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        ["resource_dependency_unknown", "resource_iam_action_invalid"]
    );
}

#[test]
fn migration_assets_must_be_in_the_published_package() {
    let package = tempfile::tempdir().expect("temporary plugin package");
    write_minimal_package(package.path());
    fs::create_dir_all(package.path().join("migrations/postgres")).expect("migration directory");
    fs::write(
        package.path().join("migrations/postgres/0001_example.sql"),
        "select 1;\n",
    )
    .expect("migration");
    mutate_manifest(package.path(), |manifest| {
        manifest["databases"] = serde_json::json!(["postgres"]);
        manifest["migrations"] = serde_json::json!([{
            "id": "example-postgres-v1",
            "database": "postgres",
            "path": "migrations/postgres"
        }]);
    });

    let report = PluginConformance::for_package(package.path()).run();

    assert_eq!(report.status, ConformanceStatus::Failed);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code, "migration_not_packaged");
    assert_eq!(
        report.diagnostics[0].path,
        "migrations.example-postgres-v1.path"
    );
}

#[test]
fn lifecycle_checks_accept_required_configuration_and_probe_unknown_fields() {
    let package = tempfile::tempdir().expect("temporary plugin package");
    write_minimal_package(package.path());
    mutate_manifest(package.path(), |manifest| {
        manifest["configuration"] = serde_json::json!([{
            "key": "name",
            "kind": "string",
            "required": true,
            "secret": false,
            "description": "Stable example name."
        }]);
    });

    let report = PluginConformance::for_package(package.path())
        .with_plugin(ConfiguredThirdPartyPlugin)
        .with_configuration(serde_json::json!({"name": "example"}))
        .run();

    report.assert_passed();
    assert_eq!(report.assurance.plugin_lifecycle, ConformanceStatus::Passed);
}

#[test]
fn malformed_field_families_return_sorted_stable_diagnostic_codes() {
    let package = tempfile::tempdir().expect("temporary plugin package");
    write_minimal_package(package.path());
    fs::create_dir_all(package.path().join("migrations/example")).expect("migration directory");
    fs::create_dir_all(package.path().join("seeds/example")).expect("seed directory");
    let cargo_path = package.path().join("Cargo.toml");
    let cargo = fs::read_to_string(&cargo_path).expect("plugin Cargo manifest");
    fs::write(
        &cargo_path,
        cargo.replace(
            "\"minco-plugin.json\"]",
            "\"minco-plugin.json\", \"migrations/**\", \"seeds/**\"]",
        ),
    )
    .expect("plugin Cargo manifest");
    mutate_manifest(package.path(), |manifest| {
        manifest["runtimes"] = serde_json::json!(["native", "native"]);
        manifest["databases"] = serde_json::json!(["postgres"]);
        manifest["provides"] = serde_json::json!([
            {"name": "example.capability", "version": "1.0.0"},
            {"name": "example.capability", "version": "1.1.0"}
        ]);
        manifest["configuration"] = serde_json::json!([
            {
                "key": "mode",
                "kind": "string",
                "required": false,
                "secret": false,
                "description": "Example mode."
            },
            {
                "key": "mode",
                "kind": "string",
                "required": false,
                "secret": false,
                "description": "Duplicate mode."
            }
        ]);
        manifest["operations"] = serde_json::json!([
            {
                "operation_id": "listExamples",
                "method": "GET",
                "path": "/examples",
                "public": true,
                "idempotent": true
            },
            {
                "operation_id": "listExamples",
                "method": "GET",
                "path": "/examples",
                "public": true,
                "idempotent": true
            }
        ]);
        manifest["migrations"] = serde_json::json!([{
            "id": "example-migration",
            "database": "mysql",
            "path": "migrations/example"
        }]);
        manifest["seeds"] = serde_json::json!([{
            "id": "example-seed",
            "database": "sqlite",
            "class": "test",
            "path": "seeds/example"
        }]);
        manifest["resources"] = serde_json::json!([
            {
                "id": "example-worker",
                "feature": "missing-feature",
                "kind": "lambda",
                "idle_cost": "zero_compute",
                "wake_sources": [{"schedule": {"expression": ""}}]
            },
            {
                "id": "example-worker",
                "kind": "lambda",
                "idle_cost": "zero_compute"
            }
        ]);
        manifest["health_checks"] = serde_json::json!([
            {"id": "example-ready", "critical": true},
            {"id": "example-ready", "critical": false}
        ]);
        manifest["data_classes"] = serde_json::json!(["internal", "internal"]);
    });

    let report = PluginConformance::for_package(package.path()).run();

    assert_eq!(report.status, ConformanceStatus::Failed);
    assert_eq!(
        report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        [
            "capability_provision_duplicate",
            "configuration_key_duplicate",
            "data_class_duplicate",
            "health_check_id_duplicate",
            "migration_database_undeclared",
            "operation_id_duplicate",
            "operation_route_duplicate",
            "resource_feature_unknown",
            "resource_id_duplicate",
            "runtime_duplicate",
            "schedule_expression_empty",
            "seed_database_undeclared",
        ]
    );
}
