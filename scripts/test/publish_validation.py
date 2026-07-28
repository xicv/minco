#!/usr/bin/env python3
"""Regression fixtures for publish-package integration-test inclusion."""
from __future__ import annotations

import copy
import os
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "scripts" / "release"))

from validate_publish import PublishValidator  # noqa: E402
from publish import (  # noqa: E402
    archive_patch_arguments,
    external_consumer_manifest,
    packaged_test_command,
    publish_command,
    verify_release_ref,
)

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
        "['tests/compatibility.rs', 'tests/contract_policy.rs']",
    )
]

print("Publish integration-test inclusion fixtures passed.")

selected = ["minco-core", "minco-config"]
assert publish_command(selected, "crates-io", execute=False) == [
    "cargo",
    "publish",
    "--registry",
    "crates-io",
    "--locked",
    "--dry-run",
    "--package",
    "minco-core",
    "--package",
    "minco-config",
]
assert "--dry-run" not in publish_command(selected, "crates-io", execute=True)

with tempfile.TemporaryDirectory(prefix="minco-package-patches-") as temporary:
    package_root = Path(temporary)
    for name in selected:
        manifest = package_root / "target" / "package" / f"{name}-0.4.0" / "Cargo.toml"
        manifest.parent.mkdir(parents=True)
        manifest.write_text("[package]\n")
    patch_arguments = archive_patch_arguments(package_root, "0.4.0", selected)
    assert patch_arguments == [
        "--config",
        f'patch.crates-io.minco-core.path="{package_root}/target/package/minco-core-0.4.0"',
        "--config",
        f'patch.crates-io.minco-config.path="{package_root}/target/package/minco-config-0.4.0"',
    ]
    packaged_test = packaged_test_command(
        package_root / "target/package/minco-config-0.4.0/Cargo.toml",
        patch_arguments,
    )
    assert "--offline" in packaged_test
    assert "--locked" not in packaged_test

print("Coordinated dry-run and unpacked-archive fixtures passed.")

consumer_manifest = external_consumer_manifest(
    "minco-archive-no-default",
    "0.4.0",
    ['minco = { version = "=0.4.0", default-features = false }'],
)
assert 'name = "minco-archive-no-default"' in consumer_manifest
assert 'minco = { version = "=0.4.0", default-features = false }' in consumer_manifest

print("External archive-consumer manifest fixtures passed.")

with tempfile.TemporaryDirectory(prefix="minco-release-ref-") as temporary:
    release_root = Path(temporary)
    (release_root / ".jj").mkdir()
    tagged = subprocess.CompletedProcess(
        args=[],
        returncode=0,
        stdout="0123456789abcdef\n",
    )
    untagged = subprocess.CompletedProcess(args=[], returncode=0, stdout="")
    with (
        mock.patch("publish.ROOT", release_root),
        mock.patch(
            "publish.shutil.which",
            side_effect=lambda command: "/usr/bin/jj" if command == "jj" else None,
        ),
        mock.patch.dict(os.environ, {}, clear=True),
        mock.patch("publish.run", return_value=tagged) as run_mock,
    ):
        verify_release_ref("v0.4.0")
        assert run_mock.call_args.args[0] == [
            "jj",
            "log",
            "-r",
            '(@ | @-) & tags(exact:"v0.4.0")',
            "--no-graph",
            "--template",
            'commit_id ++ "\n"',
        ]

    with (
        mock.patch("publish.ROOT", release_root),
        mock.patch(
            "publish.shutil.which",
            side_effect=lambda command: "/usr/bin/jj" if command == "jj" else None,
        ),
        mock.patch.dict(os.environ, {}, clear=True),
        mock.patch("publish.run", return_value=untagged),
    ):
        try:
            verify_release_ref("v0.4.0")
        except SystemExit as error:
            assert str(error) == "release commit is not tagged v0.4.0"
        else:
            raise AssertionError("untagged JJ publication must fail closed")

print("Exact Git/JJ release-ref fixtures passed.")
