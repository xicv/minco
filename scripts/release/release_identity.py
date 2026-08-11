#!/usr/bin/env python3
"""Generate Minco's deterministic, non-authoritative release identity projection."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import tomllib
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
OUTPUT_RELATIVE = Path("verification/release-identity.json")
SEMVER_DIRECTORY = re.compile(r"\d+\.\d+\.\d+")


def file_digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def canonical_digest(value: Any) -> str:
    encoded = json.dumps(value, allow_nan=False, separators=(",", ":"), sort_keys=True).encode()
    return hashlib.sha256(encoded).hexdigest()


def require_regular(root: Path, relative: Path) -> Path:
    path = root / relative
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"RELEASE-IDENTITY-001: required regular file is missing: {relative}")
    return path


def publishable_packages(root: Path, workspace: dict[str, Any]) -> list[dict[str, Any]]:
    packages = []
    for member in workspace["members"]:
        relative = Path(member) / "Cargo.toml"
        path = require_regular(root, relative)
        package = tomllib.loads(path.read_text())["package"]
        if package.get("publish", True) is False or package.get("publish") == []:
            continue
        packages.append(
            {
                "name": package["name"],
                "version": package["version"],
                "manifest_path": str(relative),
                "manifest_sha256": file_digest(path),
            }
        )
    return sorted(packages, key=lambda item: item["name"])


def plugin_identities(root: Path) -> list[dict[str, Any]]:
    catalog_path = require_regular(root, Path("plugins/catalog.toml"))
    catalog = tomllib.loads(catalog_path.read_text())
    plugins = []
    for entry in catalog.get("plugin", []):
        relative = Path(entry["path"]) / "minco-plugin.json"
        descriptor_path = require_regular(root, relative)
        descriptor = json.loads(descriptor_path.read_text())
        if descriptor.get("id") != entry.get("id") or descriptor.get("kind") != entry.get("kind"):
            raise ValueError(
                f"RELEASE-IDENTITY-002: catalog and descriptor disagree: {relative}"
            )
        plugins.append(
            {
                "id": descriptor["id"],
                "crate": entry["crate"],
                "kind": descriptor["kind"],
                "stability": descriptor["stability"],
                "plugin_version": descriptor["plugin_version"],
                "core_compatibility": descriptor["core_compatibility"],
                "descriptor_path": str(relative),
                "descriptor_sha256": file_digest(descriptor_path),
            }
        )
    return sorted(plugins, key=lambda item: item["id"])


def changelog_section(changelog: str, version: str) -> str:
    start = re.search(rf"^## \[{re.escape(version)}\](?:\s.*)?$", changelog, re.MULTILINE)
    if start is None:
        raise ValueError(f"RELEASE-IDENTITY-003: changelog omits release {version}")
    following = re.search(r"^## \[", changelog[start.end() :], re.MULTILINE)
    end = start.end() + following.start() if following is not None else len(changelog)
    return changelog[start.start() : end].rstrip() + "\n"


def build_projection(root: Path = ROOT) -> dict[str, Any]:
    cargo_path = require_regular(root, Path("Cargo.toml"))
    truth_path = require_regular(root, Path("verification/repository-truth.toml"))
    release_path = require_regular(root, Path("docs-site/release.json"))
    versions_path = require_regular(root, Path("docs-site/versions.md"))
    changelog_path = require_regular(root, Path("CHANGELOG.md"))
    catalog_path = require_regular(root, Path("plugins/catalog.toml"))

    cargo = tomllib.loads(cargo_path.read_text())
    workspace = cargo["workspace"]
    version = workspace["package"]["version"]
    truth = tomllib.loads(truth_path.read_text())
    release = json.loads(release_path.read_text())
    if truth.get("workspace_version") != version or release.get("workspace") != version:
        raise ValueError("RELEASE-IDENTITY-002: workspace release authorities disagree")
    if truth.get("workspace_release_state") != release.get("state"):
        raise ValueError("RELEASE-IDENTITY-002: release-state authorities disagree")

    packages = publishable_packages(root, workspace)
    plugins = plugin_identities(root)
    if truth.get("publishable_package_count") != len(packages):
        raise ValueError("RELEASE-IDENTITY-002: publishable package count disagrees")
    docs_root = root / "docs-site"
    documentation_versions = sorted(
        path.name
        for path in docs_root.iterdir()
        if path.is_dir() and not path.is_symlink() and SEMVER_DIRECTORY.fullmatch(path.name)
    )
    if release["stable"] not in documentation_versions or not (docs_root / "next").is_dir():
        raise ValueError("RELEASE-IDENTITY-002: documentation routes disagree with release truth")

    release_section = changelog_section(changelog_path.read_text(), release["stable"])
    projection: dict[str, Any] = {
        "schema_version": 1,
        "kind": "minco.release-identity.v1",
        "authority": "projection_only",
        "workspace": {
            "version": version,
            "rust_version": workspace["package"]["rust-version"],
            "release_state": truth["workspace_release_state"],
            "published_baseline": truth["published_baseline"],
            "published_release_commit": truth["published_release_commit"],
        },
        "packages": packages,
        "plugins": plugins,
        "documentation": {
            "stable": release["stable"],
            "workspace": release["workspace"],
            "state": release["state"],
            "versioned_routes": documentation_versions,
            "next_route": True,
            "release_file_sha256": file_digest(release_path),
            "versions_page_sha256": file_digest(versions_path),
        },
        "changelog": {
            "version": release["stable"],
            "section_sha256": hashlib.sha256(release_section.encode()).hexdigest(),
        },
        "repository_truth": {
            "path": "verification/repository-truth.toml",
            "sha256": file_digest(truth_path),
        },
        "inputs": [
            {"path": "Cargo.toml", "sha256": file_digest(cargo_path)},
            {"path": "CHANGELOG.md", "sha256": file_digest(changelog_path)},
            {"path": "docs-site/release.json", "sha256": file_digest(release_path)},
            {"path": "docs-site/versions.md", "sha256": file_digest(versions_path)},
            {"path": "plugins/catalog.toml", "sha256": file_digest(catalog_path)},
            {"path": "verification/repository-truth.toml", "sha256": file_digest(truth_path)},
        ],
        "limitations": [
            "This deterministic projection is an index, not release, publication, deployment or provider authority.",
            "Each referenced manifest, descriptor, documentation route and repository-truth input remains independently authoritative.",
        ],
    }
    projection["projection_sha256"] = canonical_digest(projection)
    return projection


def render(projection: dict[str, Any]) -> str:
    return json.dumps(projection, allow_nan=False, indent=2, sort_keys=True) + "\n"


def check_projection(observed: dict[str, Any], root: Path = ROOT) -> None:
    if render(observed) != render(build_projection(root)):
        raise ValueError("RELEASE-IDENTITY-004: checked projection is stale")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    output = ROOT / OUTPUT_RELATIVE
    if output.is_symlink():
        raise ValueError("RELEASE-IDENTITY-005: output cannot be a symlink")
    expected = build_projection(ROOT)
    if args.check:
        if not output.is_file():
            raise ValueError("RELEASE-IDENTITY-004: checked projection is stale")
        check_projection(json.loads(output.read_text()), ROOT)
        print(f"Verified {OUTPUT_RELATIVE}.")
        return 0
    temporary = output.with_name(f".{output.name}.tmp")
    temporary.write_text(render(expected))
    os.replace(temporary, output)
    print(f"Wrote {OUTPUT_RELATIVE} ({expected['projection_sha256']}).")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, TypeError, ValueError, OSError, json.JSONDecodeError) as error:
        print(error, file=sys.stderr)
        raise SystemExit(1) from None
