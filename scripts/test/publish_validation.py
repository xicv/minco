#!/usr/bin/env python3
"""Regression fixtures for publish-package integration-test inclusion."""
from __future__ import annotations

import copy
import io
import json
import os
import subprocess
import sys
import tempfile
import tomllib
import urllib.error
from pathlib import Path
from unittest import mock

import yaml

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "scripts" / "release"))

from validate_publish import PublishValidator  # noqa: E402
from publish import (  # noqa: E402
    archive_patch_arguments,
    clean_workspace,
    external_consumer_manifest,
    packaged_test_command,
    publish_command,
    verify_release_ref,
)

MANIFEST = ROOT / "crates" / "minco-contract" / "Cargo.toml"
WORKSPACE_VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"][
    "package"
]["version"]

workflow = yaml.load(
    (ROOT / ".github" / "workflows" / "publish-crates.yml").read_text(),
    Loader=yaml.BaseLoader,
)
dispatch_inputs = workflow["on"]["workflow_dispatch"]["inputs"]
assert dispatch_inputs["release_tag"]["required"] == "false"
assert dispatch_inputs["resume_packages"]["required"] == "false"
assert dispatch_inputs["resume_packages"]["default"] == ""
release_steps = workflow["jobs"]["release"]["steps"]
checkout = next(
    step for step in release_steps if step.get("uses", "").startswith("actions/checkout@")
)
assert checkout["with"]["ref"] == "${{ inputs.release_tag || github.ref }}"
verify_tag = next(step for step in release_steps if step.get("name") == "Verify release tag")
assert verify_tag["env"]["RELEASE_TAG"] == "${{ inputs.release_tag }}"
assert "${{ inputs.release_tag }}" not in verify_tag["run"]
assert 'release_tag="$RELEASE_TAG"' in verify_tag["run"]
assert 'test "$release_tag" = "v${version}"' in verify_tag["run"]
assert 'tag_commit="$(git rev-parse "refs/tags/${release_tag}^{commit}")"' in verify_tag["run"]
assert 'test "$tag_commit" = "$(git rev-parse HEAD)"' in verify_tag["run"]
assert not any(
    step.get("name")
    in {
        "Install pinned JJ for compatibility tests",
        "Install pinned ripgrep for release scripts",
    }
    for step in release_steps
)
publish_step = next(
    step
    for step in release_steps
    if step.get("name") == "Publish selected crate family"
)
static_validation = next(
    step
    for step in release_steps
    if step.get("name") == "Verify committed local release evidence"
)
trusted_preflight = next(
    step
    for step in release_steps
    if step.get("name") == "Refuse OIDC first publication"
)
resume_preflight = next(
    step
    for step in release_steps
    if step.get("name") == "Verify partial-publication registry complement"
)
assert static_validation["env"]["MINCO_RESUME_PACKAGES"] == "${{ inputs.resume_packages }}"
assert 'if [[ -n "$MINCO_RESUME_PACKAGES" ]]' in static_validation["run"]
assert "scripts/validate_static.py" in static_validation["run"]
assert "scripts/validate_publish.py --check-registry" in static_validation["run"]
assert "scripts/source_manifest.py --check" in static_validation["run"]
assert trusted_preflight["if"] == "${{ inputs.publish }}"
assert 'new_publishable_packages' in trusted_preflight["run"]
assert "manual authenticated publication" in trusted_preflight["run"]
assert release_steps.index(trusted_preflight) < release_steps.index(publish_step)
assert resume_preflight["if"] == "${{ inputs.publish && inputs.resume_packages != '' }}"
assert resume_preflight["env"]["MINCO_RESUME_PACKAGES"] == "${{ inputs.resume_packages }}"
assert "${{ inputs.resume_packages }}" not in resume_preflight["run"]
assert 'if set(selected) != set(absent):' in resume_preflight["run"]
assert 'if set(present) != expected_present:' in resume_preflight["run"]
assert release_steps.index(resume_preflight) < release_steps.index(publish_step)
assert publish_step["env"]["MINCO_RELEASE_TAG"] == "${{ steps.release-ref.outputs.tag }}"
assert publish_step["env"]["MINCO_RESUME_PACKAGES"] == "${{ inputs.resume_packages }}"
assert "${{ inputs.release_tag }}" not in publish_step["run"]
assert "${{ inputs.resume_packages }}" not in publish_step["run"]
assert '[[ ! "$package" =~ ^[a-z0-9][a-z0-9_-]*$ ]]' in publish_step["run"]
assert 'package_args+=(--package "$package")' in publish_step["run"]
assert 'done <<< "$MINCO_RESUME_PACKAGES"' in publish_step["run"]
assert 'GITHUB_REF="refs/tags/${MINCO_RELEASE_TAG}"' in publish_step["run"]
assert 'scripts/release/publish.sh --execute --skip-quality "${package_args[@]}"' in publish_step["run"]

print("Publish workflow tagged-checkout and committed-evidence boundary passed.")


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
        "['tests/compatibility.rs', 'tests/contract_policy.rs', "
        "'tests/request_profile.rs', 'tests/request_validation.rs']",
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
        manifest = (
            package_root
            / "target"
            / "package"
            / f"{name}-{WORKSPACE_VERSION}"
            / "Cargo.toml"
        )
        manifest.parent.mkdir(parents=True)
        manifest.write_text("[package]\n")
    patch_arguments = archive_patch_arguments(package_root, WORKSPACE_VERSION, selected)
    assert patch_arguments == [
        "--config",
        f'patch.crates-io.minco-core.path="{package_root}/target/package/minco-core-{WORKSPACE_VERSION}"',
        "--config",
        f'patch.crates-io.minco-config.path="{package_root}/target/package/minco-config-{WORKSPACE_VERSION}"',
    ]
    packaged_test = packaged_test_command(
        package_root
        / f"target/package/minco-config-{WORKSPACE_VERSION}/Cargo.toml",
        patch_arguments,
    )
    assert "--offline" in packaged_test
    assert "--locked" not in packaged_test

print("Coordinated dry-run and unpacked-archive fixtures passed.")

with tempfile.TemporaryDirectory(prefix="minco-clean-jj-workspace-") as temporary:
    release_root = Path(temporary)
    (release_root / ".jj").mkdir()
    clean = subprocess.CompletedProcess(args=[], returncode=0, stdout="")
    with (
        mock.patch("publish.ROOT", release_root),
        mock.patch(
            "publish.shutil.which",
            side_effect=lambda command: "/usr/bin/jj" if command == "jj" else None,
        ),
        mock.patch("publish.run", return_value=clean) as run_mock,
    ):
        clean_workspace()
        assert run_mock.call_args_list[1].args[0] == [
            "jj",
            "log",
            "-r",
            "@ & conflicts()",
            "--no-graph",
            "--template",
            'change_id ++ "\n"',
        ]

print("JJ release-snapshot conflict fixture passed.")

consumer_manifest = external_consumer_manifest(
    "minco-archive-no-default",
    WORKSPACE_VERSION,
    [f'minco = {{ version = "={WORKSPACE_VERSION}", default-features = false }}'],
)
assert 'name = "minco-archive-no-default"' in consumer_manifest
assert (
    f'minco = {{ version = "={WORKSPACE_VERSION}", default-features = false }}'
    in consumer_manifest
)

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
        mock.patch.dict(
            os.environ,
            {
                "GITHUB_REF": "refs/heads/main",
                "MINCO_RELEASE_TAG": f"v{WORKSPACE_VERSION}",
            },
            clear=True,
        ),
        mock.patch("publish.run", return_value=tagged) as run_mock,
    ):
        verify_release_ref(f"v{WORKSPACE_VERSION}")
        assert run_mock.call_args.args[0] == [
            "jj",
            "log",
            "-r",
            f'(@ | @-) & tags(exact:"v{WORKSPACE_VERSION}")',
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
        mock.patch.dict(
            os.environ,
            {
                "GITHUB_REF": "refs/heads/main",
                "MINCO_RELEASE_TAG": "v0.0.0",
            },
            clear=True,
        ),
        mock.patch("publish.run", return_value=tagged) as run_mock,
    ):
        try:
            verify_release_ref(f"v{WORKSPACE_VERSION}")
        except SystemExit as error:
            assert str(error) == (
                f"publishing requires verified release tag v{WORKSPACE_VERSION}; "
                "found v0.0.0"
            )
            run_mock.assert_not_called()
        else:
            raise AssertionError("mismatched verified release tag must fail closed")

    with (
        mock.patch("publish.ROOT", release_root),
        mock.patch(
            "publish.shutil.which",
            side_effect=lambda command: "/usr/bin/jj" if command == "jj" else None,
        ),
        mock.patch.dict(os.environ, {}, clear=True),
        mock.patch("publish.run", return_value=tagged) as run_mock,
    ):
        verify_release_ref(f"v{WORKSPACE_VERSION}")
        assert run_mock.call_args.args[0] == [
            "jj",
            "log",
            "-r",
            f'(@ | @-) & tags(exact:"v{WORKSPACE_VERSION}")',
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
            verify_release_ref(f"v{WORKSPACE_VERSION}")
        except SystemExit as error:
            assert str(error) == (
                f"release commit is not tagged v{WORKSPACE_VERSION}"
            )
        else:
            raise AssertionError("untagged JJ publication must fail closed")

print("Exact Git/JJ release-ref fixtures passed.")


def registry_response(payload: dict[str, object]) -> mock.MagicMock:
    response = mock.MagicMock()
    response.__enter__.return_value = io.BytesIO(json.dumps(payload).encode())
    return response


published_validator = PublishValidator(
    ROOT,
    check_registry=False,
    expect_unpublished=False,
    require_registry=False,
    expect_published=True,
)
published_validator.packages = {"minco-contract": (MANIFEST, package)}
with mock.patch(
    "validate_publish.urllib.request.urlopen",
    return_value=registry_response(
        {"versions": [{"num": WORKSPACE_VERSION, "yanked": False}]}
    ),
):
    published_validator.validate_registry_names()
assert published_validator.registry_checks_succeeded == 1
assert published_validator.findings == []

missing_version_validator = PublishValidator(
    ROOT,
    check_registry=False,
    expect_unpublished=False,
    require_registry=False,
    expect_published=True,
)
missing_version_validator.packages = {"minco-contract": (MANIFEST, package)}
with mock.patch(
    "validate_publish.urllib.request.urlopen",
    return_value=registry_response(
        {"versions": [{"num": "0.3.1", "yanked": False}]}
    ),
):
    missing_version_validator.validate_registry_names()
assert [
    (finding.code, finding.severity, finding.message)
    for finding in missing_version_validator.findings
] == [
    (
        "PUBLISH-074",
        "error",
        f"minco-contract {WORKSPACE_VERSION} is not published on crates.io",
    )
]

yanked_version_validator = PublishValidator(
    ROOT,
    check_registry=False,
    expect_unpublished=False,
    require_registry=False,
    expect_published=True,
)
yanked_version_validator.packages = {"minco-contract": (MANIFEST, package)}
with mock.patch(
    "validate_publish.urllib.request.urlopen",
    return_value=registry_response(
        {"versions": [{"num": WORKSPACE_VERSION, "yanked": True}]}
    ),
):
    yanked_version_validator.validate_registry_names()
assert [
    (finding.code, finding.severity, finding.message)
    for finding in yanked_version_validator.findings
] == [
    (
        "PUBLISH-074",
        "error",
        f"minco-contract {WORKSPACE_VERSION} is not published on crates.io",
    )
]

candidate_yanked_version_validator = PublishValidator(
    ROOT,
    check_registry=False,
    expect_unpublished=False,
    require_registry=False,
)
candidate_yanked_version_validator.packages = {
    "minco-contract": (MANIFEST, package)
}
with mock.patch(
    "validate_publish.urllib.request.urlopen",
    return_value=registry_response(
        {"versions": [{"num": WORKSPACE_VERSION, "yanked": True}]}
    ),
):
    candidate_yanked_version_validator.validate_registry_names()
assert [
    (finding.code, finding.severity, finding.message)
    for finding in candidate_yanked_version_validator.findings
] == [
    (
        "PUBLISH-072",
        "error",
        f"minco-contract {WORKSPACE_VERSION} already exists on crates.io; "
        "publishing this version will fail",
    )
]

unavailable_registry_validator = PublishValidator(
    ROOT,
    check_registry=False,
    expect_unpublished=False,
    require_registry=False,
    expect_published=True,
)
unavailable_registry_validator.packages = {"minco-contract": (MANIFEST, package)}
with mock.patch(
    "validate_publish.urllib.request.urlopen",
    side_effect=urllib.error.URLError("offline"),
):
    unavailable_registry_validator.validate_registry_names()
assert [
    (finding.code, finding.severity)
    for finding in unavailable_registry_validator.findings
] == [("PUBLISH-071", "error")]

print("Exact published-release registry fixtures passed.")
