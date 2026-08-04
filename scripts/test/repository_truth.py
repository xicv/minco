#!/usr/bin/env python3
"""Regression fixtures for cross-source repository truth diagnostics."""
from __future__ import annotations

import importlib.util
import json
import shutil
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
TRUTH = tomllib.loads((ROOT / "verification/repository-truth.toml").read_text())
WORKSPACE_VERSION = TRUTH["workspace_version"]
PUBLISHED_BASELINE = TRUTH["published_baseline"]
RELEASE_STATE = TRUTH["workspace_release_state"]
PUBLISHED_PACKAGE_COUNT = TRUTH["published_package_count"]
PUBLISHABLE_PACKAGE_COUNT = TRUTH["publishable_package_count"]
NEW_PUBLISHABLE_PACKAGES = TRUTH["new_publishable_packages"]
PREVIOUS_PUBLISHED_BASELINE = "0.5.0"
DRIFTED_PUBLISHED_BASELINE = "9.9.8"
CANDIDATE_BASELINE = (
    PUBLISHED_BASELINE if RELEASE_STATE == "candidate" else PREVIOUS_PUBLISHED_BASELINE
)

from validate_static import (  # noqa: E402
    Validator,
    security_allows_anonymous,
    walk_openapi_schema_objects,
)

REFERENCE_GENERATOR_PATH = ROOT / "scripts" / "docs" / "generate_reference.py"
REFERENCE_GENERATOR_SPEC = importlib.util.spec_from_file_location(
    "minco_generate_reference", REFERENCE_GENERATOR_PATH
)
if REFERENCE_GENERATOR_SPEC is None or REFERENCE_GENERATOR_SPEC.loader is None:
    raise RuntimeError(f"cannot load {REFERENCE_GENERATOR_PATH}")
REFERENCE_GENERATOR = importlib.util.module_from_spec(REFERENCE_GENERATOR_SPEC)
REFERENCE_GENERATOR_SPEC.loader.exec_module(REFERENCE_GENERATOR)

SOURCE_MANIFEST_PATH = ROOT / "scripts" / "source_manifest.py"
SOURCE_MANIFEST_SPEC = importlib.util.spec_from_file_location(
    "minco_source_manifest", SOURCE_MANIFEST_PATH
)
if SOURCE_MANIFEST_SPEC is None or SOURCE_MANIFEST_SPEC.loader is None:
    raise RuntimeError(f"cannot load {SOURCE_MANIFEST_PATH}")
SOURCE_MANIFEST = importlib.util.module_from_spec(SOURCE_MANIFEST_SPEC)
SOURCE_MANIFEST_SPEC.loader.exec_module(SOURCE_MANIFEST)


class RepositoryTruthTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="minco-truth-")
        self.root = Path(self.temporary.name) / "repository"
        shutil.copytree(
            ROOT,
            self.root,
            ignore=shutil.ignore_patterns(
                ".git",
                ".jj",
                ".venv",
                "target",
                "node_modules",
                "dist",
                "cache",
                "__pycache__",
            ),
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def truth_codes(self) -> set[str]:
        validator = Validator(self.root)
        validator.validate_repository_truth()
        return {finding.code for finding in validator.findings}

    def make_unpublished_candidate(self) -> None:
        if RELEASE_STATE == "candidate":
            return
        truth = self.root / "verification/repository-truth.toml"
        truth.write_text(
            truth.read_text()
            .replace(
                f'published_baseline = "{PUBLISHED_BASELINE}"',
                f'published_baseline = "{PREVIOUS_PUBLISHED_BASELINE}"',
            )
            .replace(
                'workspace_release_state = "published"',
                'workspace_release_state = "candidate"',
            )
        )
        guide = (
            self.root
            / "docs/adoption"
            / f"{CANDIDATE_BASELINE}-to-{WORKSPACE_VERSION}.md"
        )
        guide.write_text(
            guide.read_text()
            .replace(
                f"Target version: `{WORKSPACE_VERSION}`",
                f"Candidate workspace version: `{WORKSPACE_VERSION}`",
            )
            .replace(
                "Publication status: `published`",
                "Candidate publication status: `unpublished`",
            )
        )

    def make_workspace_published(self) -> None:
        if RELEASE_STATE == "published":
            return
        truth = self.root / "verification/repository-truth.toml"
        truth.write_text(
            truth.read_text()
            .replace(
                f'published_baseline = "{PUBLISHED_BASELINE}"',
                f'published_baseline = "{WORKSPACE_VERSION}"',
            )
            .replace(
                'workspace_release_state = "candidate"',
                'workspace_release_state = "published"',
            )
            .replace(
                f"published_package_count = {PUBLISHED_PACKAGE_COUNT}",
                f"published_package_count = {PUBLISHABLE_PACKAGE_COUNT}",
            )
            .replace(
                f"new_publishable_packages = {json.dumps(NEW_PUBLISHABLE_PACKAGES)}",
                "new_publishable_packages = []",
            )
        )

    def test_current_repository_truth_is_consistent(self) -> None:
        self.assertEqual(self.truth_codes(), set())

    def test_generated_reference_is_current(self) -> None:
        self.assertEqual(REFERENCE_GENERATOR.stale_outputs(ROOT), [])

    def test_generated_reference_detects_facade_feature_drift(self) -> None:
        facade = self.root / "crates" / "minco" / "Cargo.toml"
        facade.write_text(
            facade.read_text().replace(
                'test = ["dep:minco-test", "http"]',
                'test = ["dep:minco-test", "http", "contract"]',
            )
        )
        self.assertIn(
            "docs/reference/generated/features.md",
            REFERENCE_GENERATOR.stale_outputs(self.root),
        )

    def test_generated_reference_detects_plugin_catalog_drift(self) -> None:
        catalog = self.root / "plugins" / "catalog.toml"
        catalog.write_text(
            catalog.read_text().replace(
                "Append-only audit history independent of operational logs.",
                "Append-only audited history independent of operational logs.",
            )
        )
        self.assertIn(
            "docs/reference/generated/plugins.md",
            REFERENCE_GENERATOR.stale_outputs(self.root),
        )

    def test_generated_reference_detects_distribution_value_drift(self) -> None:
        distribution = self.root / "plugins" / "minco-plugin-audit" / "minco-plugin.json"
        distribution.write_text(
            distribution.read_text().replace('"plugin_version": "1.0.0"', '"plugin_version": "1.0.1"')
        )
        self.assertIn(
            "docs/reference/generated/plugins.md",
            REFERENCE_GENERATOR.stale_outputs(self.root),
        )

    def test_generated_reference_rejects_distribution_symlinks(self) -> None:
        distribution = self.root / "plugins" / "minco-plugin-audit" / "minco-plugin.json"
        outside = Path(self.temporary.name) / "outside-plugin.json"
        outside.write_text(distribution.read_text())
        distribution.unlink()
        distribution.symlink_to(outside)
        with self.assertRaisesRegex(ValueError, "cannot be a symlink"):
            REFERENCE_GENERATOR.stale_outputs(self.root)

    def test_generated_reference_detects_configuration_schema_drift(self) -> None:
        manifest = self.root / "minco.toml"
        manifest.write_text(
            manifest.read_text().replace(
                "Stable application service name",
                "Stable application and service name",
            )
        )
        self.assertIn(
            "docs/reference/generated/schemas.md",
            REFERENCE_GENERATOR.stale_outputs(self.root),
        )

    def test_generated_reference_detects_diagnostic_drift(self) -> None:
        model = self.root / "crates" / "minco-plan" / "src" / "model.rs"
        old_code = "MINCO-PLAN-" + "003"
        new_code = "MINCO-PLAN-" + "099"
        model.write_text(model.read_text().replace(old_code, new_code))
        self.assertIn(
            "docs/reference/generated/diagnostics.md",
            REFERENCE_GENERATOR.stale_outputs(self.root),
        )

    def test_generated_reference_detects_cli_page_drift(self) -> None:
        cli = self.root / "docs" / "reference" / "generated" / "cli.md"
        cli.write_text(cli.read_text() + "\nmanual drift\n")
        self.assertIn(
            "docs/reference/generated/cli.md",
            REFERENCE_GENERATOR.stale_outputs(self.root),
        )

    def test_generated_reference_rejects_output_symlinks(self) -> None:
        features = self.root / "docs" / "reference" / "generated" / "features.md"
        outside = Path(self.temporary.name) / "outside-features.md"
        outside.write_text(features.read_text())
        features.unlink()
        features.symlink_to(outside)
        with self.assertRaisesRegex(ValueError, "cannot be a symlink"):
            REFERENCE_GENERATOR.stale_outputs(self.root)

    def test_generated_schema_reference_redacts_secret_defaults(self) -> None:
        schemas = REFERENCE_GENERATOR.render_outputs(ROOT)[
            "docs/reference/generated/schemas.md"
        ]
        database_row = next(
            line for line in schemas.splitlines() if line.startswith("| `database.url`")
        )
        self.assertIn("| yes | — |", database_row)
        self.assertNotIn("env:", database_row)
        self.assertNotIn("ssm:", database_row)

    def test_generated_reference_is_byte_stable(self) -> None:
        first = REFERENCE_GENERATOR.render_outputs(ROOT)
        second = REFERENCE_GENERATOR.render_outputs(ROOT)
        self.assertEqual(first, second)

    def test_generated_reference_has_no_trailing_whitespace(self) -> None:
        for relative, content in REFERENCE_GENERATOR.render_outputs(ROOT).items():
            with self.subTest(relative=relative):
                self.assertFalse(
                    any(line.endswith((" ", "\t")) for line in content.splitlines())
                )

    def test_generated_reference_rejects_cli_binary_symlinks(self) -> None:
        binary = self.root / "target" / "debug" / "cargo-minco"
        binary.parent.mkdir(parents=True)
        outside = Path(self.temporary.name) / "outside-cargo-minco"
        outside.write_text("#!/usr/bin/env bash\nexit 0\n")
        outside.chmod(0o755)
        binary.symlink_to(outside)
        with self.assertRaisesRegex(ValueError, "cannot be a symlink"):
            REFERENCE_GENERATOR.cli_binary(self.root)

    def test_workspace_version_drift_has_a_stable_code(self) -> None:
        truth = self.root / "verification/repository-truth.toml"
        truth.write_text(
            truth.read_text().replace(
                f'workspace_version = "{WORKSPACE_VERSION}"',
                'workspace_version = "9.9.9"',
            )
        )
        self.assertIn("STATIC-TRUTH-VERSION-001", self.truth_codes())

    def test_release_requires_an_explicit_matching_state(self) -> None:
        truth = self.root / "verification/repository-truth.toml"
        mismatched = "published" if RELEASE_STATE == "candidate" else "candidate"
        truth.write_text(
            truth.read_text().replace(
                f'workspace_release_state = "{RELEASE_STATE}"',
                f'workspace_release_state = "{mismatched}"',
            )
        )
        self.assertIn("STATIC-TRUTH-RELEASE-001", self.truth_codes())

    def test_unpublished_candidate_requires_an_upgrade_guide(self) -> None:
        self.make_unpublished_candidate()
        guide = (
            self.root
            / "docs/adoption"
            / f"{CANDIDATE_BASELINE}-to-{WORKSPACE_VERSION}.md"
        )
        guide.unlink()
        self.assertIn("STATIC-TRUTH-RELEASE-002", self.truth_codes())

    def test_unpublished_candidate_requires_substantive_changelog_notes(self) -> None:
        self.make_unpublished_candidate()
        changelog = self.root / "CHANGELOG.md"
        changelog.write_text(
            "# Changelog\n\n"
            "## [Unreleased]\n\n"
            "No changes yet.\n"
        )
        self.assertIn("STATIC-TRUTH-RELEASE-003", self.truth_codes())

    def test_readme_inventory_drift_has_a_stable_code(self) -> None:
        readme = self.root / "README.md"
        readme.write_text(
            readme.read_text().replace(
                f"Current publishable package count: `{PUBLISHABLE_PACKAGE_COUNT}`",
                "Current publishable package count: `25`",
            )
        )
        self.assertIn("STATIC-TRUTH-DOCS-001", self.truth_codes())

    def test_current_adoption_baseline_drift_has_a_stable_code(self) -> None:
        guide = self.root / "docs/adoption/incremental-adoption.md"
        guide.write_text(
            guide.read_text().replace(
                f"Published baseline: `{PUBLISHED_BASELINE}`",
                f"Published baseline: `{DRIFTED_PUBLISHED_BASELINE}`",
            )
        )
        self.assertIn("STATIC-TRUTH-DOCS-001", self.truth_codes())

    def test_cli_deployment_surface_drift_has_a_stable_code(self) -> None:
        cli = self.root / "docs/reference/cli.md"
        cli.write_text(cli.read_text().replace("cargo minco promote", "cargo minco alias"))
        self.assertIn("STATIC-TRUTH-DOCS-001", self.truth_codes())

    def test_new_package_archive_test_drift_has_a_stable_code(self) -> None:
        self.make_unpublished_candidate()
        new_package = (
            NEW_PUBLISHABLE_PACKAGES[0]
            if NEW_PUBLISHABLE_PACKAGES
            else "minco-config"
        )
        if not NEW_PUBLISHABLE_PACKAGES:
            truth = self.root / "verification/repository-truth.toml"
            truth.write_text(
                truth.read_text()
                .replace(
                    f"published_package_count = {PUBLISHED_PACKAGE_COUNT}",
                    f"published_package_count = {PUBLISHED_PACKAGE_COUNT - 1}",
                )
                .replace(
                    "new_publishable_packages = []",
                    f'new_publishable_packages = ["{new_package}"]',
                )
            )
        cargo = self.root / "Cargo.toml"
        cargo.write_text(
            cargo.read_text().replace(
                f'  "{new_package}",\n',
                "",
                1,
            )
        )
        self.assertIn("STATIC-TRUTH-PACKAGES-004", self.truth_codes())

    def test_current_published_baseline_requires_the_full_package_count(self) -> None:
        self.make_workspace_published()
        truth = self.root / "verification/repository-truth.toml"
        truth.write_text(
            truth.read_text().replace(
                f"published_package_count = {PUBLISHABLE_PACKAGE_COUNT}",
                f"published_package_count = {PUBLISHABLE_PACKAGE_COUNT - 1}",
            )
        )
        self.assertIn("STATIC-TRUTH-PUBLISHED-002", self.truth_codes())

    def test_current_published_baseline_has_no_candidate_packages(self) -> None:
        self.make_workspace_published()
        truth = self.root / "verification/repository-truth.toml"
        truth.write_text(
            truth.read_text().replace(
                "new_publishable_packages = []",
                'new_publishable_packages = ["minco-config"]',
            )
        )
        self.assertIn("STATIC-TRUTH-PUBLISHED-003", self.truth_codes())

    def test_catalog_workspace_drift_has_a_stable_code(self) -> None:
        catalog = self.root / "plugins/catalog.toml"
        catalog.write_text(
            catalog.read_text().replace(
                'path = "extensions/minco-aws-worker"',
                'path = "extensions/not-the-worker"',
            )
        )
        self.assertIn("STATIC-TRUTH-CATALOG-002", self.truth_codes())

    def test_roadmap_task_drift_has_a_stable_code(self) -> None:
        roadmap = self.root / "roadmap/roadmap.yaml"
        roadmap.write_text(
            roadmap.read_text().replace(
                "- id: M6\n  name: Essential official extensions\n  status: active",
                "- id: M6\n  name: Essential official extensions\n  status: complete",
            )
        )
        self.assertIn("STATIC-TRUTH-ROADMAP-001", self.truth_codes())

    def test_active_milestone_with_complete_tasks_has_a_stable_code(self) -> None:
        roadmap = self.root / "roadmap/roadmap.yaml"
        roadmap.write_text(
            roadmap.read_text().replace(
                "- id: M9\n  name: Application lifecycle and developer experience\n  status: complete",
                "- id: M9\n  name: Application lifecycle and developer experience\n  status: active",
            )
        )
        self.assertIn("STATIC-TRUTH-ROADMAP-003", self.truth_codes())

    def test_planned_milestone_rejects_ready_task_evidence(self) -> None:
        task = self.root / "tasks/M12/M12-T01-local-read-only-mcp.md"
        task.write_text(task.read_text().replace("status: planned", "status: ready"))
        self.assertIn("STATIC-TRUTH-ROADMAP-002", self.truth_codes())

    def test_default_dependency_growth_has_a_stable_code(self) -> None:
        measurements = self.root / "verification/adoption-measurements.json"
        value = json.loads(measurements.read_text())
        value["candidate"]["facade"]["default_features"]["normal_dependency_packages"] += 1
        measurements.write_text(json.dumps(value))
        self.assertIn("STATIC-BUDGET-004", self.truth_codes())

    def test_mutable_candidate_revision_has_a_stable_code(self) -> None:
        measurements = self.root / "verification/adoption-measurements.json"
        value = json.loads(measurements.read_text())
        value["candidate"]["revision"] = "mutable-change-id"
        measurements.write_text(json.dumps(value))
        self.assertIn("STATIC-MEASURE-002", self.truth_codes())

    def test_source_manifest_revision_drift_has_a_stable_code(self) -> None:
        measurements = self.root / "verification/adoption-measurements.json"
        value = json.loads(measurements.read_text())
        value["candidate"]["revision"] = f"source-tree-sha256:{'0' * 64}"
        measurements.write_text(json.dumps(value))
        self.assertIn("STATIC-MEASURE-004", self.truth_codes())

    def test_source_manifest_excludes_generated_appsync_plan_output(self) -> None:
        generated_template = (
            self.root
            / "proofs/realtime-pusher/appsync-plan/generated/template.yaml"
        )
        self.assertFalse(SOURCE_MANIFEST.included(self.root, generated_template))

    def test_native_artifact_budget_has_a_stable_code(self) -> None:
        measurements = self.root / "verification/adoption-measurements.json"
        value = json.loads(measurements.read_text())
        value["candidate"]["native_arm64_artifacts"]["orders_lambda"][
            "compressed_bytes"
        ] = 10_485_761
        measurements.write_text(json.dumps(value))
        self.assertIn("STATIC-BUDGET-006", self.truth_codes())

    def test_missing_native_artifact_digest_has_a_stable_code(self) -> None:
        measurements = self.root / "verification/adoption-measurements.json"
        value = json.loads(measurements.read_text())
        del value["candidate"]["native_arm64_artifacts"]["orders_lambda"]["sha256"]
        measurements.write_text(json.dumps(value))
        self.assertIn("STATIC-MEASURE-005", self.truth_codes())

    def test_missing_native_artifact_budget_has_a_stable_code(self) -> None:
        truth = self.root / "verification/repository-truth.toml"
        truth.write_text(
            truth.read_text().replace(
                "native_lambda_zip_max_bytes = 10485760\n",
                "",
            )
        )
        self.assertIn("STATIC-BUDGET-007", self.truth_codes())

    def test_manual_workflow_qualifies_plan_sam_and_both_lambda_artifacts(self) -> None:
        workflow = yaml.safe_load(
            (self.root / ".github/workflows/minco-manual.yml").read_text()
        )
        steps = workflow["jobs"]["quality"]["steps"]
        steps_by_name = {
            step["name"]: step
            for step in steps
            if "name" in step
        }
        commands = {
            name: step.get("run", "")
            for name, step in steps_by_name.items()
        }
        release_tools = commands["Install release qualification tools"]
        self.assertIn(
            "cargo install --locked --bin jj jj-cli --version 0.43.0",
            release_tools,
        )
        self.assertIn("jj --version", release_tools)
        self.assertIn(
            "cargo install --locked ripgrep --version 15.2.0",
            release_tools,
        )
        self.assertIn("rg --version", release_tools)
        self.assertEqual(
            steps_by_name["Install release qualification tools"]["if"],
            "${{ inputs.profile == 'release' }}",
        )
        self.assertEqual(
            commands["Install pinned Cargo Lambda"],
            "cargo install --locked cargo-lambda --version 1.9.1",
        )
        self.assertEqual(
            steps_by_name["Install pinned Cargo Lambda"]["if"],
            "${{ inputs.profile == 'release' }}",
        )
        zig_steps = [
            step
            for step in steps
            if step.get("uses", "").startswith("mlugg/setup-zig@")
        ]
        self.assertEqual(
            zig_steps,
            [
                {
                    "name": "Install pinned Zig",
                    "if": "${{ inputs.profile == 'release' }}",
                    "uses": "mlugg/setup-zig@d1434d08867e3ee9daa34448df10607b98908d29",
                    "with": {"version": "0.14.0"},
                }
            ],
        )
        qualification = commands[
            "Plan, SAM, and native ARM64 Lambda qualification"
        ]
        self.assertEqual(
            steps_by_name["Plan, SAM, and native ARM64 Lambda qualification"]["if"],
            "${{ inputs.profile == 'release' }}",
        )
        for required in [
            "cargo lambda --version",
            "sam --version",
            "scripts/aws/plan.sh",
            "scripts/aws/validate.sh",
            "scripts/aws/build-lambda.sh",
            "scripts/aws/build-worker-lambda.sh",
        ]:
            self.assertIn(required, qualification)

    def test_security_requirement_shape_matches_rust_policy_fixtures(self) -> None:
        fixture = yaml.safe_load(
            (
                ROOT
                / "crates/minco-contract/tests/fixtures/invalid-security-requirements.yaml"
            ).read_text()
        )
        operations = [
            operation
            for path_item in fixture["paths"].values()
            for method, operation in path_item.items()
            if method in {"get", "put", "post", "delete", "options", "head", "patch", "trace"}
        ]
        self.assertEqual(len(operations), 4)
        for operation in operations:
            self.assertEqual(
                security_allows_anonymous(operation["security"]),
                (False, False),
            )

    def test_example_and_extension_schema_keys_are_not_schema_objects(self) -> None:
        fixture = yaml.safe_load(
            (
                ROOT / "crates/minco-contract/tests/fixtures/valid-policy.yaml"
            ).read_text()
        )
        locations = {
            location for location, _schema in walk_openapi_schema_objects(fixture)
        }
        self.assertNotIn(
            "$.components.schemas.Metadata.example.schema",
            locations,
        )
        self.assertNotIn(
            "$.components.schemas.Metadata.x-example-metadata.schema",
            locations,
        )
        self.assertIn(
            "$.components.pathItems.CallbackPayload.post.requestBody.content.application/json.schema",
            locations,
        )
        self.assertIn(
            "$.components.responses.Problem.content.application/problem+json.encoding.payload.headers.X-Encoded-Metadata.schema",
            locations,
        )


if __name__ == "__main__":
    unittest.main()
