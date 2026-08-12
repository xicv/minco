#!/usr/bin/env python3
"""Generate or verify Minco's reviewed golden-topology cost baseline."""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any, Callable, Iterable


ROOT = Path(__file__).resolve().parents[1]
OUTPUT_RELATIVE = Path("verification/cost-regression-baseline.json")
PROFILES: tuple[tuple[str, Path], ...] = (
    ("orders-local-sqlite", Path("examples/orders/config/minco.local-sqlite.toml")),
    ("orders-neon-free", Path("examples/orders/config/minco.dev.toml")),
    ("orders-neon-launch", Path("examples/orders/config/minco.neon-launch.toml")),
    (
        "orders-aurora-serverless-v2",
        Path("examples/orders/config/minco.aurora-serverless-v2.toml"),
    ),
    ("orders-rds-postgres", Path("examples/orders/config/minco.rds-postgres.toml")),
    (
        "orders-self-hosted-postgres",
        Path("examples/orders/config/minco.self-hosted-postgres.toml"),
    ),
    ("orders-dynamodb-on-demand", Path("examples/orders/config/minco.dynamodb.toml")),
)
SHA256 = re.compile(r"[0-9a-f]{64}")
REPORT_KEYS = {
    "schema_version",
    "kind",
    "provider_contact",
    "production_budget",
    "profiles",
    "limitations",
}
PROFILE_KEYS = {
    "id",
    "config",
    "config_sha256",
    "projection_sha256",
    "projection",
}
PROJECTION_KEYS = {
    "database",
    "runtime",
    "database_profile",
    "structural_diagnostics",
    "overall_estimate_complete",
}
Runner = Callable[[Path, Path], dict[str, Any]]


def canonical_bytes(value: Any) -> bytes:
    """Return the one canonical JSON representation accepted by this gate."""

    require_json_safe(value)
    return (
        json.dumps(value, allow_nan=False, indent=2, sort_keys=True) + "\n"
    ).encode()


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def require_json_safe(value: Any) -> None:
    if isinstance(value, float) and not math.isfinite(value):
        raise ValueError("COST-REGRESSION-002: projection contains a non-finite number")
    if isinstance(value, dict):
        if not all(isinstance(key, str) for key in value):
            raise ValueError("COST-REGRESSION-002: projection keys must be strings")
        for child in value.values():
            require_json_safe(child)
    elif isinstance(value, list):
        for child in value:
            require_json_safe(child)


def canonical_projection(raw: dict[str, Any]) -> dict[str, Any]:
    """Retain cost authority while excluding only the explanatory CLI note."""

    if not isinstance(raw, dict) or set(raw) != PROJECTION_KEYS | {"note"}:
        raise ValueError("COST-REGRESSION-002: CLI cost projection has an unknown schema")
    projection = {key: raw[key] for key in sorted(PROJECTION_KEYS)}
    require_json_safe(projection)
    if (
        not isinstance(projection["database"], dict)
        or not isinstance(projection["runtime"], dict)
        or not isinstance(projection["database_profile"], str)
        or not projection["database_profile"]
        or not isinstance(projection["structural_diagnostics"], list)
        or not isinstance(projection["overall_estimate_complete"], bool)
    ):
        raise ValueError("COST-REGRESSION-002: CLI cost projection is malformed")
    return projection


def read_stable_config(root: Path, relative: Path) -> tuple[bytes, os.stat_result]:
    """Read one repository-confined regular file and reject symlink aliases."""

    if relative.is_absolute() or any(part in {"", ".", ".."} for part in relative.parts):
        raise ValueError("COST-REGRESSION-005: configuration path is not confined")
    try:
        root = root.resolve(strict=True)
        current = root
        for part in relative.parts:
            current /= part
            if current.is_symlink():
                raise ValueError(
                    "COST-REGRESSION-005: configuration path follows a symlink"
                )
        resolved = current.resolve(strict=True)
        if not resolved.is_relative_to(root) or not resolved.is_file():
            raise ValueError(
                "COST-REGRESSION-005: configuration path is not a regular file"
            )
        before = resolved.stat()
        data = resolved.read_bytes()
        after = resolved.stat()
    except OSError as error:
        raise ValueError(
            "COST-REGRESSION-005: configuration path is missing or unreadable"
        ) from error
    stable = ("st_dev", "st_ino", "st_mode", "st_size", "st_mtime_ns", "st_ctime_ns")
    if any(getattr(before, field) != getattr(after, field) for field in stable):
        raise ValueError("COST-REGRESSION-005: configuration changed while it was read")
    return data, after


def run_cost_cli(root: Path, config: Path) -> dict[str, Any]:
    binary = root / "target/debug/cargo-minco"
    if (
        binary.is_symlink()
        or (root / "target").is_symlink()
        or (root / "target/debug").is_symlink()
        or not binary.is_file()
    ):
        raise ValueError(
            "COST-REGRESSION-006: build target/debug/cargo-minco before cost validation"
        )
    try:
        completed = subprocess.run(
            [str(binary), "minco", "cost", "--config", str(config), "--json"],
            cwd=root,
            text=True,
            capture_output=True,
            check=False,
            env={
                "LANG": "C",
                "LC_ALL": "C",
                "NO_COLOR": "1",
                "PATH": os.defpath,
            },
            timeout=30,
        )
    except subprocess.TimeoutExpired as error:
        raise ValueError(
            "COST-REGRESSION-007: cost projection command exceeded 30 seconds"
        ) from error
    except OSError as error:
        raise ValueError(
            "COST-REGRESSION-007: cost projection command could not be executed"
        ) from error
    if completed.returncode != 0:
        raise ValueError(
            "COST-REGRESSION-007: cost projection command failed with exit "
            f"{completed.returncode}"
        )
    try:
        value = json.loads(
            completed.stdout,
            parse_constant=lambda item: (_ for _ in ()).throw(
                ValueError(f"non-finite value {item}")
            ),
        )
    except (json.JSONDecodeError, ValueError) as error:
        raise ValueError(
            "COST-REGRESSION-007: cost projection command returned invalid JSON"
        ) from error
    if not isinstance(value, dict):
        raise ValueError("COST-REGRESSION-007: cost projection command returned a non-object")
    return value


def build_report(
    *,
    root: Path = ROOT,
    profiles: Iterable[tuple[str, Path]] = PROFILES,
    runner: Runner = run_cost_cli,
) -> dict[str, Any]:
    records: list[dict[str, Any]] = []
    for identifier, config in profiles:
        before, before_stat = read_stable_config(root, config)
        projection = canonical_projection(runner(root, config))
        after, after_stat = read_stable_config(root, config)
        stable = ("st_dev", "st_ino", "st_mode", "st_size", "st_mtime_ns", "st_ctime_ns")
        if before != after or any(
            getattr(before_stat, field) != getattr(after_stat, field) for field in stable
        ):
            raise ValueError(
                "COST-REGRESSION-005: configuration changed while cost was projected"
            )
        records.append(
            {
                "id": identifier,
                "config": config.as_posix(),
                "config_sha256": sha256_bytes(before),
                "projection_sha256": sha256_bytes(canonical_bytes(projection)),
                "projection": projection,
            }
        )
    report = {
        "schema_version": 1,
        "kind": "minco.topology-cost-regression.v1",
        "provider_contact": False,
        "production_budget": False,
        "profiles": records,
        "limitations": [
            "The baseline detects repository cost-model drift; it does not fetch current provider prices.",
            "Incomplete regional rates, account eligibility and live-provider behavior remain explicit operational gates.",
            "Local structural evidence is not a production budget, invoice forecast or AWS qualification.",
        ],
    }
    validate_report(report)
    return report


def validate_report(report: Any) -> None:
    if (
        not isinstance(report, dict)
        or set(report) != REPORT_KEYS
        or type(report.get("schema_version")) is not int
        or report["schema_version"] != 1
        or report.get("kind") != "minco.topology-cost-regression.v1"
        or report.get("provider_contact") is not False
        or report.get("production_budget") is not False
    ):
        raise ValueError("COST-REGRESSION-001: baseline envelope is malformed")
    profiles = report.get("profiles")
    if not isinstance(profiles, list) or not profiles:
        raise ValueError("COST-REGRESSION-003: baseline requires reviewed profiles")
    ids: set[str] = set()
    paths: set[str] = set()
    for profile in profiles:
        if not isinstance(profile, dict) or set(profile) != PROFILE_KEYS:
            raise ValueError("COST-REGRESSION-003: baseline profile is malformed")
        identifier = profile.get("id")
        path = profile.get("config")
        if (
            not isinstance(identifier, str)
            or re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", identifier) is None
            or identifier in ids
            or not isinstance(path, str)
            or not path
            or "\\" in path
            or Path(path).is_absolute()
            or Path(path).as_posix() != path
            or any(part in {"", ".", ".."} for part in Path(path).parts)
            or path in paths
            or not isinstance(profile.get("config_sha256"), str)
            or SHA256.fullmatch(profile["config_sha256"]) is None
            or not isinstance(profile.get("projection_sha256"), str)
            or SHA256.fullmatch(profile["projection_sha256"]) is None
        ):
            raise ValueError("COST-REGRESSION-003: baseline profile identity is ambiguous")
        if (
            not isinstance(profile.get("projection"), dict)
            or set(profile["projection"]) != PROJECTION_KEYS
        ):
            raise ValueError("COST-REGRESSION-003: baseline projection is malformed")
        projection = canonical_projection({**profile["projection"], "note": "excluded"})
        if sha256_bytes(canonical_bytes(projection)) != profile["projection_sha256"]:
            raise ValueError("COST-REGRESSION-003: baseline projection digest is invalid")
        ids.add(identifier)
        paths.add(path)
    limitations = report.get("limitations")
    if (
        not isinstance(limitations, list)
        or not limitations
        or not all(isinstance(item, str) and item.strip() for item in limitations)
    ):
        raise ValueError("COST-REGRESSION-001: baseline limitations are missing")


def validate_current_report(
    report: dict[str, Any],
    *,
    root: Path = ROOT,
    profiles: Iterable[tuple[str, Path]] = PROFILES,
    runner: Runner = run_cost_cli,
) -> None:
    validate_report(report)
    current = build_report(root=root, profiles=profiles, runner=runner)
    if {profile["id"] for profile in current["profiles"]} == {
        identifier for identifier, _ in PROFILES
    }:
        validate_golden_invariants(current)
    if report != current:
        raise ValueError(
            "COST-REGRESSION-004: golden topology cost projection changed; review and regenerate the baseline"
        )


def validate_golden_invariants(report: dict[str, Any]) -> None:
    """Fail independently of snapshots on Minco's critical zero-idle policy."""

    records = {profile["id"]: profile for profile in report["profiles"]}
    if set(records) != {identifier for identifier, _ in PROFILES}:
        raise ValueError("COST-REGRESSION-009: golden topology inventory is incomplete")
    local = records["orders-local-sqlite"]["projection"]["runtime"]
    if (
        local.get("request_based_resources") != []
        or local.get("missing_rates") != []
        or local.get("evidence") != []
        or local.get("schedules") != []
        or local.get("queues") != []
        or local.get("workers") != []
        or local.get("realtime") is not None
    ):
        raise ValueError(
            "COST-REGRESSION-009: local topology acquired an AWS cost or wake dimension"
        )
    expected_fixed = {
        "orders-local-sqlite": ["database:sqlite_persistent_host"],
        "orders-neon-free": [],
        "orders-neon-launch": [],
        "orders-aurora-serverless-v2": [],
        "orders-rds-postgres": ["database:rds_postgres"],
        "orders-self-hosted-postgres": ["database:self_hosted_postgres"],
        "orders-dynamodb-on-demand": [],
    }
    expected_rates = [
        "regional_api_gateway_request_rate",
        "regional_lambda_request_and_duration_rates",
    ]
    for identifier, profile in records.items():
        runtime = profile["projection"]["runtime"]
        if runtime.get("fixed_cost_resources") != expected_fixed[identifier]:
            raise ValueError(
                "COST-REGRESSION-009: golden topology fixed-cost policy changed"
            )
        if identifier == "orders-local-sqlite":
            continue
        if (
            profile["projection"].get("overall_estimate_complete") is not False
            or runtime.get("request_based_resources") != ["http_api", "lambda:api"]
            or runtime.get("missing_rates") != expected_rates
            or runtime.get("schedules") != []
            or runtime.get("queues") != []
            or runtime.get("workers") != []
            or runtime.get("realtime") is not None
        ):
            raise ValueError(
                "COST-REGRESSION-009: golden AWS topology cost or wake policy changed"
            )


def load_canonical_report(path: Path) -> dict[str, Any]:
    def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result:
                raise ValueError("COST-REGRESSION-001: baseline contains duplicate keys")
            result[key] = value
        return result

    try:
        rendered = path.read_bytes()
        report = json.loads(
            rendered,
            object_pairs_hook=unique_object,
            parse_constant=lambda item: (_ for _ in ()).throw(
                ValueError(f"non-finite value {item}")
            ),
        )
    except (json.JSONDecodeError, UnicodeDecodeError, OSError, ValueError) as error:
        raise ValueError("COST-REGRESSION-001: baseline is not canonical JSON") from error
    if not isinstance(report, dict) or rendered != canonical_bytes(report):
        raise ValueError("COST-REGRESSION-001: baseline is not canonical JSON")
    validate_report(report)
    return report


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--output", type=Path, default=OUTPUT_RELATIVE)
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    output = arguments.output if arguments.output.is_absolute() else ROOT / arguments.output
    if output.resolve(strict=False) != (ROOT / OUTPUT_RELATIVE).resolve(strict=False):
        raise ValueError("COST-REGRESSION-008: baseline output must use the canonical path")
    if arguments.check:
        validate_current_report(load_canonical_report(output))
        print(f"Verified {OUTPUT_RELATIVE}.")
        return 0
    if (
        output.is_symlink()
        or output.parent.is_symlink()
        or (output.exists() and not output.is_file())
    ):
        raise ValueError("COST-REGRESSION-008: baseline output path is unsafe")
    report = build_report()
    validate_golden_invariants(report)
    temporary = output.with_name(f".{output.name}.tmp")
    if temporary.is_symlink() or temporary.exists():
        raise ValueError("COST-REGRESSION-008: baseline temporary path is unsafe")
    try:
        with temporary.open("xb") as handle:
            handle.write(canonical_bytes(report))
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, output)
    finally:
        if temporary.exists() and not temporary.is_symlink():
            temporary.unlink()
    print(f"Generated {OUTPUT_RELATIVE}.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, OSError, TypeError, ValueError, subprocess.SubprocessError) as error:
        print(error, file=sys.stderr)
        raise SystemExit(1) from None
