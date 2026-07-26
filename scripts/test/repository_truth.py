#!/usr/bin/env python3
"""Regression fixtures for cross-source repository truth diagnostics."""
from __future__ import annotations

import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

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
                'workspace_version = "0.3.0"',
                'workspace_version = "9.9.9"',
            )
        )
        self.assertIn("STATIC-TRUTH-VERSION-001", self.truth_codes())

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
