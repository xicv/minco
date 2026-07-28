#!/usr/bin/env python3
"""Guarded multi-package crates.io dry-run and publish driver for Minco."""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MIN_CARGO = (1, 90, 0)


def run(
    command: list[str],
    *,
    check: bool = True,
    capture: bool = False,
    environment: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    print("+", " ".join(command), flush=True)
    return subprocess.run(
        command,
        cwd=ROOT,
        check=check,
        text=True,
        env=environment,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.STDOUT if capture else None,
    )


def cargo_version() -> tuple[int, int, int]:
    if shutil.which("cargo") is None:
        raise SystemExit("cargo is unavailable; install the pinned Rust toolchain first")
    output = run(["cargo", "--version"], capture=True).stdout or ""
    match = re.search(r"cargo (\d+)\.(\d+)\.(\d+)", output)
    if not match:
        raise SystemExit(f"could not parse Cargo version from: {output.strip()}")
    return tuple(int(value) for value in match.groups())


def workspace_data() -> dict:
    return tomllib.loads((ROOT / "Cargo.toml").read_text())


def release_packages(data: dict) -> list[str]:
    try:
        values = data["workspace"]["metadata"]["minco"]["release"]["publish"]
    except KeyError as error:
        raise SystemExit(f"release package list is missing: {error}") from error
    if not isinstance(values, list) or not values or not all(isinstance(value, str) for value in values):
        raise SystemExit("workspace.metadata.minco.release.publish must be a non-empty string array")
    return values


def packaged_test_packages(data: dict) -> list[str]:
    values = data["workspace"]["metadata"]["minco"]["release"].get("package_tests", [])
    if not isinstance(values, list) or not all(isinstance(value, str) for value in values):
        raise SystemExit(
            "workspace.metadata.minco.release.package_tests must be a string array"
        )
    return values


def archive_patch_arguments(
    root: Path,
    version: str,
    selected: list[str],
) -> list[str]:
    arguments: list[str] = []
    for package in selected:
        package_root = root / "target" / "package" / f"{package}-{version}"
        if not (package_root / "Cargo.toml").is_file():
            raise SystemExit(f"packaged manifest is missing: {package_root / 'Cargo.toml'}")
        arguments.extend(
            [
                "--config",
                f"patch.crates-io.{package}.path={json.dumps(str(package_root))}",
            ]
        )
    return arguments


def verify_packaged_tests(data: dict, selected: list[str]) -> None:
    version = data["workspace"]["package"]["version"]
    patch_arguments = archive_patch_arguments(ROOT, version, selected)
    for package in packaged_test_packages(data):
        if package not in selected:
            continue
        manifest = ROOT / "target" / "package" / f"{package}-{version}" / "Cargo.toml"
        run(packaged_test_command(manifest, patch_arguments))


def packaged_test_command(
    manifest: Path,
    patch_arguments: list[str],
) -> list[str]:
    return [
        "cargo",
        "test",
        "--manifest-path",
        str(manifest),
        "--offline",
        *patch_arguments,
    ]


def external_consumer_manifest(
    name: str,
    version: str,
    dependency_lines: list[str],
) -> str:
    dependencies = "\n".join(dependency_lines)
    return f"""[package]
name = {json.dumps(name)}
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
{dependencies}
"""


def verify_external_consumers(data: dict, selected: list[str]) -> None:
    version = data["workspace"]["package"]["version"]
    patch_arguments = archive_patch_arguments(ROOT, version, selected)
    target = ROOT / "target" / "package" / "external-consumer-target"
    environment = dict(os.environ)
    environment["CARGO_TARGET_DIR"] = str(target)
    consumers = [
        (
            "minco-archive-no-default",
            [f'minco = {{ version = "={version}", default-features = false }}'],
        ),
        ("minco-archive-default", [f'minco = "={version}"']),
        (
            "minco-archive-all-features",
            [f'minco = {{ version = "={version}", features = ["full"] }}'],
        ),
        (
            "minco-archive-new-packages",
            [
                f'minco-config = "={version}"',
                f'minco-db = "={version}"',
                f'minco-dev = "={version}"',
                f'minco-deploy-aws = "={version}"',
            ],
        ),
    ]
    with tempfile.TemporaryDirectory(prefix="minco-release-consumers-") as temporary:
        consumer_root = Path(temporary)
        for name, dependencies in consumers:
            project = consumer_root / name
            (project / "src").mkdir(parents=True)
            (project / "Cargo.toml").write_text(
                external_consumer_manifest(name, version, dependencies)
            )
            (project / "src" / "lib.rs").write_text(
                "#![forbid(unsafe_code)]\n"
                "pub fn archive_consumer_compiled() -> bool { true }\n"
            )
            run(
                [
                    "cargo",
                    "check",
                    "--manifest-path",
                    str(project / "Cargo.toml"),
                    "--offline",
                    *patch_arguments,
                ],
                environment=environment,
            )

        install_root = consumer_root / "cargo-minco-install"
        run(
            [
                "cargo",
                "install",
                "--path",
                str(ROOT / "target" / "package" / f"cargo-minco-{version}"),
                "--root",
                str(install_root),
                "--offline",
                "--locked",
                "--debug",
                *patch_arguments,
            ],
            environment=environment,
        )
        run(
            [str(install_root / "bin" / "cargo-minco"), "minco", "--version"],
            environment=environment,
        )


def publish_command(
    selected: list[str],
    registry: str,
    *,
    execute: bool,
) -> list[str]:
    command = ["cargo", "publish", "--registry", registry, "--locked"]
    if not execute:
        command.append("--dry-run")
    for package in selected:
        command.extend(["--package", package])
    return command


def clean_workspace() -> None:
    if (ROOT / ".jj").exists() and shutil.which("jj"):
        result = run(["jj", "diff", "--summary"], capture=True)
        if (result.stdout or "").strip():
            raise SystemExit("JJ working copy is not clean; publish from a dedicated release change")
        conflicts = run(
            ["jj", "log", "-r", "conflicts()", "--no-graph", "--template", 'change_id ++ "\n"'],
            capture=True,
        )
        if (conflicts.stdout or "").strip():
            raise SystemExit("JJ working copy contains unresolved conflicts")
        return
    if (ROOT / ".git").exists() and shutil.which("git"):
        run(["git", "diff", "--quiet"])
        run(["git", "diff", "--cached", "--quiet"])
        untracked = run(["git", "ls-files", "--others", "--exclude-standard"], capture=True)
        if (untracked.stdout or "").strip():
            raise SystemExit("Git working copy contains untracked files")
        return
    raise SystemExit("a clean JJ or Git workspace is required for publication")


def expected_tag(data: dict) -> str:
    return f"v{data['workspace']['package']['version']}"


def verify_release_ref(tag: str) -> None:
    github_ref = os.environ.get("GITHUB_REF")
    if github_ref is not None and github_ref != f"refs/tags/{tag}":
        raise SystemExit(f"publishing requires refs/tags/{tag}; found {github_ref}")

    if shutil.which("git") and (ROOT / ".git").exists():
        tags = run(["git", "tag", "--points-at", "HEAD"], capture=True).stdout or ""
        if tag not in tags.splitlines():
            raise SystemExit(f"release commit is not tagged {tag}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--execute", action="store_true", help="upload instead of performing a dry run")
    parser.add_argument("--package", action="append", default=[], help="publish only one named release package; repeatable")
    parser.add_argument("--skip-quality", action="store_true", help="skip local quality commands; intended only after the same CI job ran them")
    parser.add_argument("--registry", default="crates-io")
    args = parser.parse_args()

    version = cargo_version()
    if version < MIN_CARGO:
        raise SystemExit(f"Cargo {MIN_CARGO[0]}.{MIN_CARGO[1]} or newer is required for multi-package publishing; found {version}")

    data = workspace_data()
    declared = release_packages(data)
    selected = args.package or declared
    unknown = sorted(set(selected) - set(declared))
    if unknown:
        raise SystemExit(f"packages are not in the Minco release set: {', '.join(unknown)}")
    selected = [package for package in declared if package in set(selected)]

    if not (ROOT / "Cargo.lock").is_file():
        raise SystemExit("Cargo.lock is missing; run `cargo generate-lockfile`, review it, and commit it")

    clean_workspace()
    if args.execute:
        verify_release_ref(expected_tag(data))

    if not args.skip_quality:
        run([sys.executable, "scripts/validate_static.py"])
        run([sys.executable, "scripts/validate_publish.py", "--check-registry", "--require-registry"])
        run([sys.executable, "scripts/deep_review.py"])
        run(["cargo", "fmt", "--all", "--", "--check"])
        run(["cargo", "check", "-p", "minco", "--no-default-features", "--locked"])
        run(["cargo", "check", "-p", "minco", "--locked"])
        run(["cargo", "check", "-p", "minco", "--all-features", "--locked"])
        run(["cargo", "check", "-p", "cargo-minco", "--locked"])
        run(["cargo", "test", "-p", "minco", "--no-default-features", "--locked"])
        run(["cargo", "test", "-p", "minco", "--locked"])
        run(["cargo", "clippy", "--workspace", "--all-targets", "--all-features", "--locked", "--", "-D", "warnings"])
        run(["cargo", "test", "--workspace", "--all-targets", "--all-features", "--locked"])
        run(["scripts/test/generated_apps.sh"])
        run(["cargo", "rustdoc", "-p", "cargo-minco", "--lib", "--all-features", "--locked"])
        run(["cargo", "doc", "--workspace", "--all-features", "--no-deps", "--locked"])

    print("Minco release packages (dry-run):")
    for package in selected:
        print(f"  - {package}")
    run(publish_command(selected, args.registry, execute=False))

    verify_packaged_tests(data, selected)
    if selected == declared:
        verify_external_consumers(data, selected)
    else:
        print(
            "Skipping external consumer verification for a partial package selection; "
            "the full family gate remains required before release.",
            flush=True,
        )

    if args.execute:
        print("Minco release packages (upload):")
        for package in selected:
            print(f"  - {package}")
        run(publish_command(selected, args.registry, execute=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
