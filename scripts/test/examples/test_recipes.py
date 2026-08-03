#!/usr/bin/env python3
"""Behavioral tests for the exercised Minco recipe matrix."""
from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import textwrap
import tomllib
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
VALIDATOR = ROOT / "scripts" / "test" / "examples" / "validate.py"
RUNNER = ROOT / "scripts" / "test" / "examples" / "all.sh"


class RecipeMatrixTests(unittest.TestCase):
    def run_validator(self, root: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(VALIDATOR), "--check", "--root", str(root)],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )

    def write_fixture(
        self,
        root: Path,
        *,
        documentation: str,
        checks: str = 'checks = ["sample"]',
        cost_classes: str = 'cost_classes = ["zero_compute"]',
        kind: str = "application",
        runtime: str = "local_native",
        database: str = "memory",
        wake_sources: str = "wake_sources = []",
    ) -> None:
        (root / "examples" / "sample").mkdir(parents=True)
        (root / "docs" / "how-to").mkdir(parents=True)
        (root / "docs" / "how-to" / "sample.md").write_text(documentation)
        (root / "examples" / "recipes.toml").write_text(
            textwrap.dedent(
                f"""
                schema_version = 1

                [[recipe]]
                id = "sample"
                title = "Sample"
                kind = "{kind}"
                example = "examples/sample"
                documentation = "docs/how-to/sample.md"
                features = ["contract"]
                runtime = "{runtime}"
                database = "{database}"
                provider_assumptions = ["No provider contact."]
                {cost_classes}
                {wake_sources}
                {checks}
                unsupported_gates = ["No production evidence."]
                """
            ).lstrip()
        )

    def test_repository_recipe_matrix_is_valid(self) -> None:
        result = self.run_validator(ROOT)

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_recipe_documentation_requires_operational_disclosures(self) -> None:
        with tempfile.TemporaryDirectory(prefix="minco-recipe-test-") as temporary:
            root = Path(temporary)
            self.write_fixture(root, documentation="# Sample\n")

            result = self.run_validator(root)

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing required section: ## Features", result.stderr)

    def test_recipe_checks_must_use_the_bounded_runner_registry(self) -> None:
        documentation = "\n".join(
            (
                "# Sample",
                "## Features",
                "## Provider assumptions",
                "## Cost and wake behavior",
                "## Verification",
                "## Unsupported gates",
                "",
            )
        )
        with tempfile.TemporaryDirectory(prefix="minco-recipe-test-") as temporary:
            root = Path(temporary)
            self.write_fixture(
                root,
                documentation=documentation,
                checks='checks = ["shell-anything"]',
            )

            result = self.run_validator(root)

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unknown check id: shell-anything", result.stderr)

    def test_recipe_documentation_names_every_bound_check(self) -> None:
        documentation = "\n".join(
            (
                "# Sample",
                "## Features",
                "## Provider assumptions",
                "## Cost and wake behavior",
                "## Verification",
                "No executable proof is named here.",
                "## Unsupported gates",
                "",
            )
        )
        with tempfile.TemporaryDirectory(prefix="minco-recipe-test-") as temporary:
            root = Path(temporary)
            self.write_fixture(
                root,
                documentation=documentation,
                checks='checks = ["third-party-plugin"]',
            )

            result = self.run_validator(root)

        self.assertNotEqual(result.returncode, 0)
        self.assertIn(
            "documentation does not name check: third-party-plugin", result.stderr
        )

    def test_recipe_cost_classes_are_closed_vocabulary(self) -> None:
        documentation = "\n".join(
            (
                "# Sample",
                "## Features",
                "## Provider assumptions",
                "## Cost and wake behavior",
                "## Verification",
                "Run `third-party-plugin`.",
                "## Unsupported gates",
                "",
            )
        )
        with tempfile.TemporaryDirectory(prefix="minco-recipe-test-") as temporary:
            root = Path(temporary)
            self.write_fixture(
                root,
                documentation=documentation,
                checks='checks = ["third-party-plugin"]',
                cost_classes='cost_classes = ["cheap-ish"]',
            )

            result = self.run_validator(root)

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unknown cost class: cheap-ish", result.stderr)

    def test_recipe_classification_fields_are_closed_vocabularies(self) -> None:
        documentation = "\n".join(
            (
                "# Sample",
                "## Features",
                "## Provider assumptions",
                "## Cost and wake behavior",
                "## Verification",
                "Run `third-party-plugin`.",
                "## Unsupported gates",
                "",
            )
        )
        cases = (
            ({"kind": "misc"}, "unknown kind: misc"),
            ({"runtime": "somewhere"}, "unknown runtime: somewhere"),
            ({"database": "everything"}, "unknown database: everything"),
            (
                {"wake_sources": 'wake_sources = ["sqs_message"]'},
                "unknown wake source: sqs_message",
            ),
        )
        for overrides, expected in cases:
            with self.subTest(expected=expected):
                with tempfile.TemporaryDirectory(
                    prefix="minco-recipe-test-"
                ) as temporary:
                    root = Path(temporary)
                    self.write_fixture(
                        root,
                        documentation=documentation,
                        checks='checks = ["third-party-plugin"]',
                        **overrides,
                    )

                    result = self.run_validator(root)

                self.assertNotEqual(result.returncode, 0)
                self.assertIn(expected, result.stderr)

    def test_recipe_documentation_bash_fences_must_parse(self) -> None:
        documentation = "\n".join(
            (
                "# Sample",
                "## Features",
                "## Provider assumptions",
                "## Cost and wake behavior",
                "## Verification",
                "Run `third-party-plugin`.",
                "```bash",
                "if then",
                "```",
                "## Unsupported gates",
                "",
            )
        )
        with tempfile.TemporaryDirectory(prefix="minco-recipe-test-") as temporary:
            root = Path(temporary)
            self.write_fixture(
                root,
                documentation=documentation,
                checks='checks = ["third-party-plugin"]',
            )

            result = self.run_validator(root)

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("invalid bash fence", result.stderr)

    def test_bounded_runner_executes_a_named_public_recipe_check(self) -> None:
        result = subprocess.run(
            [
                sys.executable,
                str(VALIDATOR),
                "--run",
                "--only",
                "third-party-plugin",
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("recipe check passed: third-party-plugin", result.stdout)

    def test_bounded_runner_cannot_inherit_aws_credentials_or_endpoints(self) -> None:
        with tempfile.TemporaryDirectory(prefix="minco-recipe-env-") as temporary:
            temporary_path = Path(temporary)
            binary_dir = temporary_path / "bin"
            binary_dir.mkdir()
            environment_log = temporary_path / "environment.log"
            cargo = binary_dir / "cargo"
            cargo.write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                "printf '%s\\n' \"${AWS_ACCESS_KEY_ID-unset}\" "
                "\"${AWS_CONFIG_FILE-unset}\" "
                "\"${AWS_SHARED_CREDENTIALS_FILE-unset}\" "
                "\"${AWS_EC2_METADATA_DISABLED-unset}\" "
                "\"${AWS_ENDPOINT_URL-unset}\" > \"$MINCO_ENV_LOG\"\n"
            )
            cargo.chmod(0o755)
            environment = os.environ.copy()
            environment.update(
                {
                    "PATH": f"{binary_dir}:{environment['PATH']}",
                    "MINCO_ENV_LOG": str(environment_log),
                    "AWS_ACCESS_KEY_ID": "test-access-key",
                    "AWS_CONFIG_FILE": "/tmp/test-aws-config",
                    "AWS_SHARED_CREDENTIALS_FILE": "/tmp/test-aws-credentials",
                    "AWS_ENDPOINT_URL_S3": "https://provider.example.invalid",
                }
            )

            result = subprocess.run(
                [
                    sys.executable,
                    str(VALIDATOR),
                    "--run",
                    "--only",
                    "third-party-plugin",
                ],
                cwd=ROOT,
                env=environment,
                capture_output=True,
                text=True,
                check=False,
            )
            environment_values = environment_log.read_text().splitlines()

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            environment_values,
            [
                "unset",
                os.devnull,
                os.devnull,
                "true",
                "http://127.0.0.1:9",
            ],
        )

    def test_task_runner_exposes_the_bounded_recipe_checks(self) -> None:
        result = subprocess.run(
            [str(RUNNER), "--only", "third-party-plugin"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("recipe check passed: third-party-plugin", result.stdout)

    def test_matrix_covers_the_essential_web_application_recipes(self) -> None:
        matrix = tomllib.loads((ROOT / "examples" / "recipes.toml").read_text())
        recipes = {recipe["id"]: recipe for recipe in matrix["recipe"]}

        self.assertTrue(
            {
                "first-application",
                "resource-crud",
                "local-sqlite",
                "postgres-profiles",
                "sqs-worker",
                "zero-idle-aws",
                "static-site",
                "third-party-plugin",
                "generated-application",
                "verified-review-loop",
            }.issubset(recipes)
        )
        self.assertTrue(
            {"memory", "sqlite", "postgres"}.issubset(
                {recipe["database"] for recipe in recipes.values()}
            )
        )
        self.assertTrue(
            {"local_native", "lambda_zip_arm64"}.issubset(
                {recipe["runtime"] for recipe in recipes.values()}
            )
        )
        cost_classes = {
            cost_class
            for recipe in recipes.values()
            for cost_class in recipe["cost_classes"]
        }
        self.assertTrue(
            {"zero_compute", "request_only", "storage_only"}.issubset(cost_classes)
        )


if __name__ == "__main__":
    unittest.main()
