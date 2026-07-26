#!/usr/bin/env python3
"""Regression fixtures for publish-package integration-test inclusion."""
from __future__ import annotations

import copy
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from validate_publish import PublishValidator  # noqa: E402

MANIFEST = ROOT / "crates" / "minco-contract" / "Cargo.toml"


def validate(package: dict[str, object]) -> list[tuple[str, str]]:
    validator = PublishValidator(
        ROOT,
        check_registry=False,
        expect_unpublished=False,
        require_registry=False,
    )
    validator.packages = {"minco-contract": (MANIFEST, package)}
    validator.validate_package_metadata()
    return [(finding.code, finding.message) for finding in validator.findings]


package = tomllib.loads(MANIFEST.read_text())["package"]
assert not [finding for finding in validate(package) if finding[0] == "PUBLISH-021"]

drifted = copy.deepcopy(package)
drifted["include"] = [
    entry for entry in drifted["include"] if entry != "tests/**"
]
assert [finding for finding in validate(drifted) if finding[0] == "PUBLISH-021"] == [
    (
        "PUBLISH-021",
        "minco-contract package.include omits integration test sources: "
        "['tests/contract_policy.rs']",
    )
]

print("Publish integration-test inclusion fixtures passed.")
