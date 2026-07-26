#!/usr/bin/env python3
"""Measure and compare Minco facade dependency shapes and native Lambda ZIPs."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import zipfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = re.compile(r"^([A-Za-z0-9_.-]+) v[0-9]")


def sha256(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def capture(arguments: list[str]) -> str:
    return subprocess.run(
        arguments,
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    ).stdout


def tree(arguments: list[str]) -> dict[str, int]:
    normal = capture(
        [
            "cargo",
            "tree",
            "--locked",
            "-p",
            "minco",
            "--prefix",
            "none",
            "-e",
            "normal",
            *arguments,
        ]
    )
    feature = capture(
        [
            "cargo",
            "tree",
            "--locked",
            "-p",
            "minco",
            "--prefix",
            "none",
            "-e",
            "features",
            *arguments,
        ]
    )
    packages = {
        match.group(1)
        for line in normal.splitlines()
        if (match := PACKAGE.match(line)) is not None
    }
    return {
        "normal_dependency_packages": len(packages),
        "feature_tree_lines": len([line for line in feature.splitlines() if line.strip()]),
    }


def assignments(values: list[str], value_type: type) -> dict[str, Any]:
    parsed: dict[str, Any] = {}
    for value in values:
        label, separator, raw = value.partition("=")
        if not separator or not label or label in parsed:
            raise SystemExit(f"invalid or duplicate assignment {value!r}; expected LABEL=VALUE")
        try:
            parsed[label] = value_type(raw)
        except ValueError as error:
            raise SystemExit(f"invalid value in {value!r}: {error}") from error
    return parsed


def artifact_measurements(
    values: list[str],
    timings: dict[str, float],
) -> dict[str, dict[str, Any]]:
    artifacts: dict[str, dict[str, Any]] = {}
    for label, raw_path in assignments(values, str).items():
        path = (ROOT / raw_path).resolve()
        if not path.is_file():
            raise SystemExit(f"artifact does not exist: {path}")
        try:
            display_path = str(path.relative_to(ROOT))
        except ValueError:
            display_path = str(path)
        artifact: dict[str, Any] = {
            "path": display_path,
            "compressed_bytes": path.stat().st_size,
            "sha256": sha256(path),
        }
        if zipfile.is_zipfile(path):
            with zipfile.ZipFile(path) as archive:
                artifact["uncompressed_bytes"] = sum(
                    member.file_size for member in archive.infolist()
                )
        if label in timings:
            artifact["cold_build_seconds"] = timings[label]
        artifacts[label] = artifact
    unknown_timings = sorted(set(timings) - set(artifacts))
    if unknown_timings:
        raise SystemExit(f"artifact timings lack matching artifacts: {unknown_timings}")
    return artifacts


def snapshot(args: argparse.Namespace) -> dict[str, Any]:
    artifact_timings = assignments(args.artifact_timing, float)
    return {
        "revision": args.revision,
        "facade": {
            "no_default_features": tree(["--no-default-features"]),
            "default_features": tree([]),
            "official_plugins": tree(["--features", "official-plugins"]),
            "all_features": tree(["--all-features"]),
        },
        "build_timings_seconds": assignments(args.timing, float),
        "native_arm64_artifacts": artifact_measurements(
            args.artifact,
            artifact_timings,
        ),
    }


def comparison(baseline: dict[str, Any], candidate: dict[str, Any]) -> dict[str, Any]:
    def package_delta(profile: str) -> int:
        return (
            candidate["facade"][profile]["normal_dependency_packages"]
            - baseline["facade"][profile]["normal_dependency_packages"]
        )

    result = {
        "no_default_dependency_package_delta": package_delta("no_default_features"),
        "default_dependency_package_delta": package_delta("default_features"),
        "official_plugin_dependency_package_delta": package_delta("official_plugins"),
        "all_feature_dependency_package_delta": package_delta("all_features"),
    }
    baseline_orders = baseline.get("native_arm64_artifacts", {}).get("orders_lambda")
    candidate_orders = candidate.get("native_arm64_artifacts", {}).get("orders_lambda")
    if baseline_orders and candidate_orders:
        byte_delta = (
            candidate_orders["compressed_bytes"] - baseline_orders["compressed_bytes"]
        )
        result["orders_lambda_compressed_byte_delta"] = byte_delta
        result["orders_lambda_compressed_percent_delta"] = round(
            byte_delta / baseline_orders["compressed_bytes"] * 100,
            4,
        )
    return result


def render(args: argparse.Namespace) -> dict[str, Any]:
    candidate = snapshot(args)
    if args.baseline is None:
        return {
            "schema_version": 1,
            **candidate,
            "method": method(),
            "limitations": limitations(),
        }
    baseline_path = args.baseline if args.baseline.is_absolute() else ROOT / args.baseline
    baseline = json.loads(baseline_path.read_text())
    return {
        "schema_version": 1,
        "baseline": baseline,
        "candidate": candidate,
        "comparison": comparison(baseline, candidate),
        "method": method(),
        "limitations": limitations(),
    }


def method() -> dict[str, Any]:
    return {
        "dependency_shape": "cargo tree --locked -p minco --prefix none with no-default, default, official-plugins, and all-features selections",
        "timings": "caller-supplied wall-clock observations from the documented isolated-target commands; not a CI budget",
        "artifacts": "ZIP stat plus uncompressed member-size sum; artifacts are built by cargo lambda build --release --arm64 --output-format zip",
        "toolchain": {
            "rustc": capture(["rustc", "--version"]).strip(),
            "cargo": capture(["cargo", "--version"]).strip(),
        },
        "command": "scripts/measure_adoption.py --revision REVISION --baseline verification/adoption-baseline.json [--timing LABEL=SECONDS] [--artifact LABEL=PATH --artifact-timing LABEL=SECONDS]",
    }


def limitations() -> list[str]:
    return [
        "Single local wall-clock samples are observational evidence, not CI budgets.",
        f"cargo-bloat available: {shutil.which('cargo-bloat') is not None}; cargo-llvm-lines available: {shutil.which('cargo-llvm-lines') is not None}.",
        "The SQS worker is new in the candidate, so no baseline worker artifact exists.",
    ]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--revision", required=True)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--timing", action="append", default=[], metavar="LABEL=SECONDS")
    parser.add_argument("--artifact", action="append", default=[], metavar="LABEL=PATH")
    parser.add_argument(
        "--artifact-timing",
        action="append",
        default=[],
        metavar="LABEL=SECONDS",
    )
    args = parser.parse_args()
    rendered = json.dumps(render(args), indent=2, sort_keys=True) + "\n"
    if args.output is None:
        print(rendered, end="")
    else:
        output = args.output if args.output.is_absolute() else ROOT / args.output
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(rendered)
        print(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
