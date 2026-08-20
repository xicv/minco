#!/usr/bin/env python3
"""Tests for Minco's deterministic release-identity projection."""
from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
import tomllib
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "release" / "release_identity.py"


def load_module():
    spec = importlib.util.spec_from_file_location("minco_release_identity", MODULE_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {MODULE_PATH}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ReleaseIdentityTests(unittest.TestCase):
    def test_current_projection_is_deterministic_and_complete(self) -> None:
        identity = load_module()
        workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())
        truth = tomllib.loads(
            (ROOT / "verification/repository-truth.toml").read_text()
        )
        catalog = tomllib.loads((ROOT / "plugins/catalog.toml").read_text())
        release = json.loads((ROOT / "docs-site/release.json").read_text())

        first = identity.build_projection(ROOT)
        second = identity.build_projection(ROOT)

        self.assertEqual(first, second)
        self.assertEqual(
            first["workspace"]["version"], workspace["workspace"]["package"]["version"]
        )
        self.assertEqual(
            first["workspace"]["release_state"], truth["workspace_release_state"]
        )
        self.assertEqual(len(first["packages"]), truth["publishable_package_count"])
        self.assertEqual(len(first["plugins"]), len(catalog["plugin"]))
        self.assertEqual(first["documentation"]["stable"], release["stable"])
        self.assertEqual(first["documentation"]["state"], release["state"])

    def test_descriptor_drift_changes_the_projection(self) -> None:
        identity = load_module()
        with tempfile.TemporaryDirectory(prefix="minco-release-identity-") as temporary:
            fixture = Path(temporary)
            shutil.copytree(ROOT / "crates", fixture / "crates", ignore=shutil.ignore_patterns("src", "tests"))
            shutil.copytree(ROOT / "extensions", fixture / "extensions", ignore=shutil.ignore_patterns("src", "tests"))
            shutil.copytree(ROOT / "plugins", fixture / "plugins", ignore=shutil.ignore_patterns("src", "tests", "assets", "node_modules"))
            shutil.copytree(ROOT / "examples", fixture / "examples", ignore=shutil.ignore_patterns("src", "tests", "openapi", "config"))
            (fixture / "verification").mkdir()
            (fixture / "docs-site").mkdir()
            for relative in (
                "Cargo.toml",
                "CHANGELOG.md",
                "verification/repository-truth.toml",
                "docs-site/release.json",
                "docs-site/versions.md",
            ):
                target = fixture / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / relative, target)
            release = json.loads((fixture / "docs-site/release.json").read_text())
            (fixture / "docs-site" / release["stable"]).mkdir()
            (fixture / "docs-site/next").mkdir()

            before = identity.build_projection(fixture)
            descriptor_path = fixture / "plugins/minco-plugin-health/minco-plugin.json"
            descriptor = json.loads(descriptor_path.read_text())
            descriptor["core_compatibility"] = "^9.9.9"
            descriptor_path.write_text(json.dumps(descriptor, indent=2) + "\n")
            after = identity.build_projection(fixture)

            self.assertNotEqual(before["projection_sha256"], after["projection_sha256"])
            self.assertNotEqual(before["plugins"], after["plugins"])

    def test_checked_output_must_equal_current_projection(self) -> None:
        identity = load_module()
        projection = identity.build_projection(ROOT)
        projection["workspace"]["version"] = "9.9.9"

        with self.assertRaisesRegex(
            ValueError,
            "RELEASE-IDENTITY-004: checked projection is stale",
        ):
            identity.check_projection(projection, ROOT)


if __name__ == "__main__":
    unittest.main()
