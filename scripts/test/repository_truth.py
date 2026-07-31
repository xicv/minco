#!/usr/bin/env python3
"""Regression fixtures for cross-source repository truth diagnostics."""
from __future__ import annotations

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

from validate_static import (  # noqa: E402
    Validator,
    security_allows_anonymous,
    walk_openapi_schema_objects,
)


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
                "__pycache__",
            ),
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def truth_codes(self) -> set[str]:
        validator = Validator(self.root)
        validator.validate_repository_truth()
        return {finding.code for finding in validator.findings}

    def test_current_repository_truth_is_consistent(self) -> None:
        self.assertEqual(self.truth_codes(), set())

    def test_workspace_version_drift_has_a_stable_code(self) -> None:
        truth = self.root / "verification/repository-truth.toml"
        truth.write_text(
            truth.read_text().replace(
                f'workspace_version = "{WORKSPACE_VERSION}"',
                'workspace_version = "9.9.9"',
            )
        )
        self.assertIn("STATIC-TRUTH-VERSION-001", self.truth_codes())

    def test_unpublished_candidate_requires_an_explicit_release_state(self) -> None:
        truth = self.root / "verification/repository-truth.toml"
        truth.write_text(
            truth.read_text().replace(
                'workspace_release_state = "candidate"',
                'workspace_release_state = "published"',
            )
        )
        self.assertIn("STATIC-TRUTH-RELEASE-001", self.truth_codes())

    def test_unpublished_candidate_requires_an_upgrade_guide(self) -> None:
        guide = (
            self.root
            / "docs/adoption"
            / f"{PUBLISHED_BASELINE}-to-{WORKSPACE_VERSION}.md"
        )
        guide.unlink()
        self.assertIn("STATIC-TRUTH-RELEASE-002", self.truth_codes())

    def test_unpublished_candidate_requires_substantive_changelog_notes(self) -> None:
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
                "Current publishable package count: `28`",
                "Current publishable package count: `25`",
            )
        )
        self.assertIn("STATIC-TRUTH-DOCS-001", self.truth_codes())

    def test_current_adoption_baseline_drift_has_a_stable_code(self) -> None:
        guide = self.root / "docs/adoption/incremental-adoption.md"
        guide.write_text(
            guide.read_text().replace(
                "Published baseline: `0.4.0`",
                "Published baseline: `0.3.1`",
            )
        )
        self.assertIn("STATIC-TRUTH-DOCS-001", self.truth_codes())

    def test_cli_deployment_surface_drift_has_a_stable_code(self) -> None:
        cli = self.root / "docs/reference/cli.md"
        cli.write_text(cli.read_text().replace("cargo minco promote", "cargo minco alias"))
        self.assertIn("STATIC-TRUTH-DOCS-001", self.truth_codes())

    def test_new_package_archive_test_drift_has_a_stable_code(self) -> None:
        truth = self.root / "verification/repository-truth.toml"
        truth.write_text(
            truth.read_text()
            .replace(
                'published_baseline = "0.4.0"',
                'published_baseline = "0.3.1"',
            )
            .replace(
                "published_package_count = 28",
                "published_package_count = 24",
            )
            .replace(
                "new_publishable_packages = []",
                'new_publishable_packages = ["minco-config"]',
            )
        )
        cargo = self.root / "Cargo.toml"
        cargo.write_text(
            cargo.read_text().replace(
                'package_tests = [\n  "minco-config",\n',
                "package_tests = [\n",
            )
        )
        self.assertIn("STATIC-TRUTH-PACKAGES-004", self.truth_codes())

    def test_current_published_baseline_requires_the_full_package_count(self) -> None:
        cargo = self.root / "Cargo.toml"
        cargo.write_text(
            cargo.read_text().replace(
                f'version = "{WORKSPACE_VERSION}"',
                f'version = "{PUBLISHED_BASELINE}"',
            )
        )
        truth = self.root / "verification/repository-truth.toml"
        truth.write_text(
            truth.read_text()
            .replace(
                f'workspace_version = "{WORKSPACE_VERSION}"',
                f'workspace_version = "{PUBLISHED_BASELINE}"',
            )
            .replace(
                'workspace_release_state = "candidate"',
                'workspace_release_state = "published"',
            )
            .replace(
                "published_package_count = 28",
                "published_package_count = 27",
            )
        )
        self.assertIn("STATIC-TRUTH-PUBLISHED-002", self.truth_codes())

    def test_current_published_baseline_has_no_candidate_packages(self) -> None:
        cargo = self.root / "Cargo.toml"
        cargo.write_text(
            cargo.read_text().replace(
                f'version = "{WORKSPACE_VERSION}"',
                f'version = "{PUBLISHED_BASELINE}"',
            )
        )
        truth = self.root / "verification/repository-truth.toml"
        truth.write_text(
            truth.read_text()
            .replace(
                f'workspace_version = "{WORKSPACE_VERSION}"',
                f'workspace_version = "{PUBLISHED_BASELINE}"',
            )
            .replace(
                'workspace_release_state = "candidate"',
                'workspace_release_state = "published"',
            )
            .replace(
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
