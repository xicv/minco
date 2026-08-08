#!/usr/bin/env python3
"""Regression tests for current-release truth markers."""
from __future__ import annotations

import shutil
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from validate_static import Validator  # noqa: E402

TRUTH = tomllib.loads((ROOT / "verification/repository-truth.toml").read_text())
BASELINE = TRUTH["published_baseline"]
RELEASE_COMMIT = TRUTH["published_release_commit"]


class CurrentProductTruthTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="minco-current-truth-")
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

    def replace(self, relative: str, old: str, new: str) -> None:
        path = self.root / relative
        source = path.read_text()
        self.assertIn(old, source)
        path.write_text(source.replace(old, new, 1))

    def test_current_repository_truth_passes(self) -> None:
        self.assertEqual(self.truth_codes(), set())

    def test_framework_maturity_header_cannot_regress(self) -> None:
        self.replace(
            "docs/vision/minco-framework-definition.md",
            f"| Area | Current published `{BASELINE}` state | Remaining boundary |",
            "| Area | Current published `0.6.0` state | Remaining boundary |",
        )
        self.assertIn("STATIC-TRUTH-DOCS-001", self.truth_codes())

    def test_review_status_requires_current_release_marker(self) -> None:
        self.replace(
            "REVIEW_STATUS.md",
            f"Minco `{BASELINE}` is the current published baseline",
            "Minco `0.6.0` is the current published baseline",
        )
        self.assertIn("STATIC-TRUTH-DOCS-001", self.truth_codes())

    def test_framework_definition_requires_exact_release_commit(self) -> None:
        self.replace(
            "docs/vision/minco-framework-definition.md",
            RELEASE_COMMIT,
            "0" * 40,
        )
        self.assertIn("STATIC-TRUTH-DOCS-001", self.truth_codes())

    def test_repository_truth_requires_exact_release_commit(self) -> None:
        self.replace(
            "verification/repository-truth.toml",
            f'published_release_commit = "{RELEASE_COMMIT}"',
            'published_release_commit = "not-a-commit"',
        )
        self.assertIn("STATIC-TRUTH-RELEASE-005", self.truth_codes())


if __name__ == "__main__":
    unittest.main()
