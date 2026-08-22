#!/usr/bin/env python3
"""Validate and execute Minco's measured quality-assurance policy."""
from __future__ import annotations

import argparse
import datetime
import hashlib
import importlib.util
import json
import math
import os
import platform
import re
import shutil
import stat
import subprocess
import sys
import time
import tomllib
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
POLICY_RELATIVE = Path("verification/quality-assurance-policy.toml")
OUTPUT_RELATIVE = Path("verification/quality-assurance.json")
SOURCE_MANIFEST_RELATIVE = Path("verification/source-manifest.json")
TARGET_RELATIVE = Path("target/minco/quality-assurance")
TOOL_IDS = (
    "cargo-nextest",
    "cargo-llvm-cov",
    "cargo-mutants",
    "cargo-semver-checks",
)
GATE_IDS = (
    "nextest_parity",
    "coverage",
    "mutation",
    "semver",
    "local_performance",
)
EXACT_VERSION = re.compile(r"\d+\.\d+(?:\.\d+)?")
SHA256 = re.compile(r"[0-9a-f]{64}")
RECEIPT_KEYS = {
    "schema_version",
    "kind",
    "status",
    "production_slo",
    "provider_contact",
    "effective_date",
    "source",
    "policy",
    "runner",
    "tools",
    "gates",
    "commands",
    "limitations",
}


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def validate_artifact_identity(
    identity: Any,
    *,
    allowed_prefixes: tuple[Path, ...],
    root: Path = ROOT,
) -> None:
    """Authenticate one confined regular file without following symlinks."""

    diagnostic = "ASSURANCE-RECEIPT-013: assurance artifact identity is stale or unsafe"
    if (
        not isinstance(identity, dict)
        or set(identity) != {"path", "bytes", "sha256"}
        or not isinstance(identity.get("path"), str)
        or "\\" in identity["path"]
        or not isinstance(identity.get("bytes"), int)
        or isinstance(identity["bytes"], bool)
        or identity["bytes"] < 0
        or not isinstance(identity.get("sha256"), str)
        or SHA256.fullmatch(identity["sha256"]) is None
    ):
        raise ValueError(diagnostic)
    relative = Path(identity["path"])
    if (
        relative.is_absolute()
        or relative.as_posix() != identity["path"]
        or not relative.parts
        or any(part in {"", ".", ".."} for part in relative.parts)
        or not any(
            relative == prefix or relative.is_relative_to(prefix)
            for prefix in allowed_prefixes
        )
    ):
        raise ValueError(diagnostic)

    if not hasattr(os, "O_NOFOLLOW") or not hasattr(os, "O_DIRECTORY"):
        raise RuntimeError(
            "ASSURANCE-PATH-004: no-follow descriptor verification is unavailable"
        )
    flags = os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW
    descriptors: list[int] = []
    try:
        root_fd = os.open(root.resolve(strict=True), flags | os.O_DIRECTORY)
        descriptors.append(root_fd)
        for part in relative.parts[:-1]:
            directory_fd = os.open(
                part,
                flags | os.O_DIRECTORY,
                dir_fd=descriptors[-1],
            )
            descriptors.append(directory_fd)
        file_fd = os.open(relative.parts[-1], flags, dir_fd=descriptors[-1])
        descriptors.append(file_fd)
        before = os.fstat(file_fd)
        if not stat.S_ISREG(before.st_mode):
            raise ValueError(diagnostic)
        observed = hashlib.sha256()
        observed_bytes = 0
        while True:
            chunk = os.read(file_fd, 1024 * 1024)
            if not chunk:
                break
            observed.update(chunk)
            observed_bytes += len(chunk)
        after = os.fstat(file_fd)
        stable_fields = ("st_dev", "st_ino", "st_mode", "st_size", "st_mtime_ns", "st_ctime_ns")
        if (
            any(getattr(before, field) != getattr(after, field) for field in stable_fields)
            or observed_bytes != identity["bytes"]
            or before.st_size != identity["bytes"]
            or observed.hexdigest() != identity["sha256"]
        ):
            raise ValueError(diagnostic)
    except OSError as error:
        raise ValueError(diagnostic) from error
    finally:
        for descriptor in reversed(descriptors):
            os.close(descriptor)


def load_source_manifest(root: Path = ROOT) -> dict[str, Any]:
    """Require the checked source manifest to equal direct current-tree authority."""

    module_path = root / "scripts" / "source_manifest.py"
    spec = importlib.util.spec_from_file_location("minco_assurance_source_manifest", module_path)
    if spec is None or spec.loader is None:
        raise ValueError("ASSURANCE-SOURCE-001: source manifest implementation is unavailable")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    expected = module.build_report(root)
    path = root / SOURCE_MANIFEST_RELATIVE
    if not path.is_file() or path.read_text() != module.render(expected):
        raise ValueError("ASSURANCE-SOURCE-002: checked source manifest is stale")
    return expected


def safe_output_path(requested: Path, root: Path = ROOT) -> Path:
    """Confine generated assurance evidence below reviewed repository locations."""

    candidate = requested if requested.is_absolute() else root / requested
    if candidate.is_symlink():
        raise ValueError("ASSURANCE-PATH-001: assurance output cannot be a symlink")
    resolved_root = root.resolve()
    resolved = candidate.resolve(strict=False)
    try:
        relative = resolved.relative_to(resolved_root)
    except ValueError as error:
        raise ValueError("ASSURANCE-PATH-001: assurance output escapes the repository") from error
    allowed = relative == OUTPUT_RELATIVE or relative.is_relative_to(TARGET_RELATIVE)
    if not allowed:
        raise ValueError(
            "ASSURANCE-PATH-001: assurance output must be canonical verification evidence or private target evidence"
        )
    current = resolved_root
    for part in relative.parent.parts:
        current /= part
        if current.exists() and current.is_symlink():
            raise ValueError("ASSURANCE-PATH-001: assurance output parent cannot be a symlink")
    resolved.parent.mkdir(parents=True, exist_ok=True)
    if not resolved.parent.resolve().is_relative_to(resolved_root):
        raise ValueError("ASSURANCE-PATH-001: assurance output parent escapes the repository")
    if resolved.exists() and not resolved.is_file():
        raise ValueError("ASSURANCE-PATH-001: assurance output must be a regular file")
    return resolved


def load_policy(path: Path = ROOT / POLICY_RELATIVE) -> dict[str, Any]:
    """Load the reviewed policy and reject ambiguous quality authority."""

    policy = tomllib.loads(path.read_text())
    if policy.get("schema") != 1 or policy.get("kind") != "minco.quality-assurance-policy.v1":
        raise ValueError("ASSURANCE-POLICY-001: quality policy requires schema 1")
    if policy.get("production_slo") is not False or policy.get("provider_contact") is not False:
        raise ValueError(
            "ASSURANCE-POLICY-002: local quality evidence cannot claim a production SLO or provider contact"
        )
    tools = policy.get("tools")
    if not isinstance(tools, dict) or set(tools) != set(TOOL_IDS):
        raise ValueError("ASSURANCE-POLICY-003: quality policy must pin every supported tool exactly")
    for identifier, version in tools.items():
        if not isinstance(version, str) or EXACT_VERSION.fullmatch(version) is None:
            raise ValueError(f"ASSURANCE-POLICY-003: {identifier} lacks an exact version")
    semver = policy.get("semver", {})
    baseline_tag = semver.get("baseline_tag")
    if not isinstance(baseline_tag, str) or re.fullmatch(r"v\d+\.\d+\.\d+", baseline_tag) is None:
        raise ValueError("ASSURANCE-POLICY-004: SemVer baseline must be an exact release tag")
    baseline_commit = semver.get("baseline_commit")
    package_count = semver.get("package_count")
    baseline_package_count = semver.get("baseline_package_count")
    new_packages = semver.get("new_packages")
    if (
        not isinstance(baseline_commit, str)
        or re.fullmatch(r"[0-9a-f]{40}", baseline_commit) is None
        or semver.get("packages") != "publishable_workspace"
        or not isinstance(package_count, int)
        or isinstance(package_count, bool)
        or package_count <= 0
        or not isinstance(baseline_package_count, int)
        or isinstance(baseline_package_count, bool)
        or baseline_package_count < 0
        or not isinstance(new_packages, list)
        or new_packages != sorted(set(new_packages))
        or not all(
            isinstance(package, str) and re.fullmatch(r"[a-z0-9][a-z0-9_-]*", package)
            for package in new_packages
        )
        or baseline_package_count + len(new_packages) != package_count
    ):
        raise ValueError(
            "ASSURANCE-POLICY-005: SemVer package boundary is incomplete or ambiguous"
        )
    return policy


def require_json_safe_numbers(value: Any) -> None:
    """Reject the non-standard NaN/Infinity values accepted by Python's JSON parser."""

    if isinstance(value, float) and not math.isfinite(value):
        raise ValueError("ASSURANCE-RECEIPT-009: receipt contains a non-finite number")
    if isinstance(value, dict):
        for child in value.values():
            require_json_safe_numbers(child)
    elif isinstance(value, list):
        for child in value:
            require_json_safe_numbers(child)


def load_canonical_receipt(path: Path) -> dict[str, Any]:
    """Parse only the canonical, duplicate-free JSON emitted by this runner."""

    def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(
                    f"ASSURANCE-RECEIPT-010: receipt contains duplicate key {key!r}"
                )
            result[key] = value
        return result

    def reject_constant(value: str) -> None:
        raise ValueError(
            f"ASSURANCE-RECEIPT-009: receipt contains non-finite number {value}"
        )

    rendered = path.read_text()
    try:
        receipt = json.loads(
            rendered,
            object_pairs_hook=unique_object,
            parse_constant=reject_constant,
        )
    except json.JSONDecodeError as error:
        raise ValueError("ASSURANCE-RECEIPT-010: receipt is not valid JSON") from error
    if not isinstance(receipt, dict) or rendered != (
        json.dumps(receipt, allow_nan=False, indent=2, sort_keys=True) + "\n"
    ):
        raise ValueError("ASSURANCE-RECEIPT-010: receipt bytes are not canonical")
    return receipt


def validate_receipt(receipt: dict[str, Any], policy: dict[str, Any]) -> None:
    """Reject a misleading PASS before it can become release evidence."""

    require_json_safe_numbers(receipt)
    if set(receipt) != RECEIPT_KEYS:
        raise ValueError("ASSURANCE-RECEIPT-011: receipt fields differ from the exact schema")
    if receipt.get("schema_version") != 1 or receipt.get("kind") != "minco.quality-assurance.v1":
        raise ValueError("ASSURANCE-RECEIPT-001: quality receipt requires schema 1")
    if receipt.get("production_slo") is not False or receipt.get("provider_contact") is not False:
        raise ValueError(
            "ASSURANCE-RECEIPT-002: local assurance cannot claim a production SLO or provider contact"
        )
    status = receipt.get("status")
    if status not in {"PASS", "NOT RUN"}:
        raise ValueError("ASSURANCE-RECEIPT-001: receipt status must be PASS or NOT RUN")
    source = receipt.get("source")
    if (
        not isinstance(source, dict)
        or not isinstance(source.get("version"), str)
        or not isinstance(source.get("source_tree_sha256"), str)
        or SHA256.fullmatch(source["source_tree_sha256"]) is None
    ):
        raise ValueError("ASSURANCE-RECEIPT-005: receipt requires exact source identity")
    policy_identity = receipt.get("policy")
    if (
        not isinstance(policy_identity, dict)
        or not isinstance(policy_identity.get("sha256"), str)
        or SHA256.fullmatch(policy_identity["sha256"]) is None
    ):
        raise ValueError("ASSURANCE-RECEIPT-006: receipt requires exact policy identity")
    runner = receipt.get("runner")
    dimensions = runner.get("dimensions") if isinstance(runner, dict) else None
    if (
        not isinstance(runner, dict)
        or runner.get("scope") != "local"
        or not isinstance(dimensions, dict)
        or set(dimensions) != {"os", "os_release", "architecture", "python"}
        or not all(isinstance(value, str) and value.strip() for value in dimensions.values())
        or runner.get("fingerprint_sha256")
        != hashlib.sha256(
            json.dumps(dimensions, separators=(",", ":"), sort_keys=True).encode()
        ).hexdigest()
    ):
        raise ValueError("ASSURANCE-RECEIPT-008: receipt requires verified local runner identity")
    tools = receipt.get("tools")
    if not isinstance(tools, dict) or set(tools) != set(TOOL_IDS):
        raise ValueError("ASSURANCE-RECEIPT-003: receipt requires exact tool results")
    for identifier, version in policy["tools"].items():
        tool = tools.get(identifier)
        if (
            not isinstance(tool, dict)
            or tool.get("version") != version
            or tool.get("status") not in {"PASS", "NOT RUN"}
            or (status == "PASS" and tool.get("status") != "PASS")
        ):
            raise ValueError(
                f"ASSURANCE-RECEIPT-003: receipt tool {identifier} does not match policy"
            )
    gates = receipt.get("gates")
    if not isinstance(gates, dict) or set(gates) != set(GATE_IDS):
        raise ValueError("ASSURANCE-RECEIPT-004: receipt requires gate results")
    for identifier in GATE_IDS:
        gate = gates.get(identifier)
        if not isinstance(gate, dict):
            if status == "PASS":
                raise ValueError(
                    f"ASSURANCE-RECEIPT-004: PASS receipt omits required gate {identifier}"
                )
            continue
        if gate.get("status") not in {"PASS", "NOT RUN"}:
            raise ValueError(f"ASSURANCE-RECEIPT-004: gate {identifier} has invalid status")
        if status == "PASS" and gate.get("status") != "PASS":
            raise ValueError(
                f"ASSURANCE-RECEIPT-004: PASS receipt contains non-PASS gate {identifier}"
            )
    if status == "PASS":
        coverage = gates["coverage"]
        line_percent = coverage.get("line_percent")
        minimum_line_percent = policy["coverage"].get("minimum_line_percent")
        if (
            not isinstance(line_percent, (int, float))
            or isinstance(line_percent, bool)
            or not isinstance(minimum_line_percent, (int, float))
            or line_percent < minimum_line_percent
        ):
            raise ValueError(
                "ASSURANCE-COVERAGE-001: "
                f"line coverage {line_percent} is below measured floor {minimum_line_percent}"
            )
        function_percent = coverage.get("function_percent")
        minimum_function_percent = policy["coverage"].get("minimum_function_percent")
        if (
            not isinstance(function_percent, (int, float))
            or isinstance(function_percent, bool)
            or not isinstance(minimum_function_percent, (int, float))
            or function_percent < minimum_function_percent
        ):
            raise ValueError(
                "ASSURANCE-COVERAGE-002: "
                f"function coverage {function_percent} is below measured floor {minimum_function_percent}"
            )
        mutation = gates["mutation"]
        mutation_policy = policy["mutation"]
        mutation_values = {
            key: mutation.get(key)
            for key in ("total_mutants", "caught", "missed", "timeouts", "unviable")
        }
        if (
            any(not isinstance(value, int) or isinstance(value, bool) or value < 0 for value in mutation_values.values())
            or mutation_values["total_mutants"] != mutation_policy.get("total_mutants")
            or mutation_values["caught"] < mutation_policy.get("minimum_caught", 0)
            or mutation_values["missed"] > mutation_policy.get("maximum_missed", 0)
            or mutation_values["timeouts"] > mutation_policy.get("maximum_timeouts", 0)
            or mutation_values["unviable"] > mutation_policy.get("maximum_unviable", 0)
            or mutation_values["caught"]
            + mutation_values["missed"]
            + mutation_values["timeouts"]
            + mutation_values["unviable"]
            != mutation_values["total_mutants"]
        ):
            raise ValueError(
                "ASSURANCE-MUTATION-001: mutation result exceeds the measured baseline"
            )
        nextest = gates["nextest_parity"]
        expected_executable = policy["nextest"].get("executable_test_count")
        expected_doctests = policy["nextest"].get("doctest_count")
        if (
            nextest.get("nextest_test_count") != expected_executable
            or nextest.get("doctest_count") != expected_doctests
            or nextest.get("cargo_test_count") != expected_executable + expected_doctests
        ):
            raise ValueError(
                "ASSURANCE-NEXTEST-001: nextest plus doctests does not preserve the measured Cargo inventory"
            )
        semver = gates["semver"]
        if (
            semver.get("baseline_tag") != policy["semver"].get("baseline_tag")
            or semver.get("baseline_commit")
            != policy["semver"].get("baseline_commit")
            or semver.get("package_count") != policy["semver"].get("package_count")
            or semver.get("checked_package_count")
            != policy["semver"].get("baseline_package_count")
            or semver.get("new_packages") != policy["semver"].get("new_packages")
        ):
            raise ValueError("ASSURANCE-SEMVER-001: SemVer evidence differs from policy")
        performance = gates["local_performance"]
        if (
            performance.get("runner_scope") != "local"
            or performance.get("production_slo") is not False
            or performance.get("provider_contact") is not False
        ):
            raise ValueError(
                "ASSURANCE-PERFORMANCE-001: local performance evidence exceeds its authority"
            )
    commands = receipt.get("commands")
    if status == "PASS" and (not isinstance(commands, list) or not commands):
        raise ValueError("ASSURANCE-RECEIPT-012: PASS receipt requires command evidence")
    if not isinstance(commands, list):
        raise ValueError("ASSURANCE-RECEIPT-012: receipt commands must be an array")
    command_ids: set[str] = set()
    for command in commands:
        log = command.get("log") if isinstance(command, dict) else None
        identifier = command.get("id") if isinstance(command, dict) else None
        arguments = command.get("command") if isinstance(command, dict) else None
        duration = command.get("duration_seconds") if isinstance(command, dict) else None
        if (
            not isinstance(command, dict)
            or set(command)
            != {"id", "command", "status", "exit_code", "duration_seconds", "log"}
            or not isinstance(identifier, str)
            or not identifier
            or identifier in command_ids
            or not isinstance(arguments, list)
            or not arguments
            or not all(isinstance(argument, str) and argument for argument in arguments)
            or command.get("status") != "PASS"
            or not isinstance(command.get("exit_code"), int)
            or isinstance(command["exit_code"], bool)
            or command["exit_code"] != 0
            or not isinstance(duration, (int, float))
            or isinstance(duration, bool)
            or duration < 0
            or not isinstance(log, dict)
            or set(log) != {"path", "bytes", "sha256"}
            or not isinstance(log.get("path"), str)
            or not log["path"].startswith(f"{TARGET_RELATIVE}/logs/")
            or not isinstance(log.get("bytes"), int)
            or isinstance(log["bytes"], bool)
            or log["bytes"] < 0
            or not isinstance(log.get("sha256"), str)
            or SHA256.fullmatch(log["sha256"]) is None
        ):
            raise ValueError("ASSURANCE-RECEIPT-012: command evidence is malformed")
        command_ids.add(identifier)
    limitations = receipt.get("limitations")
    if (
        not isinstance(limitations, list)
        or not limitations
        or not all(isinstance(item, str) and item.strip() for item in limitations)
    ):
        raise ValueError("ASSURANCE-RECEIPT-007: receipt requires explicit limitations")


def validate_current_receipt(
    receipt: dict[str, Any],
    policy: dict[str, Any],
    *,
    root: Path = ROOT,
) -> None:
    """Bind a structurally valid receipt to current source and policy bytes."""

    validate_receipt(receipt, policy)
    source = load_source_manifest(root)
    if receipt["source"] != {
        "version": source["version"],
        "source_tree_sha256": source["source_tree_sha256"],
    }:
        raise ValueError("ASSURANCE-SOURCE-003: receipt source differs from the current tree")
    policy_path = root / POLICY_RELATIVE
    if receipt["policy"] != {
        "path": str(POLICY_RELATIVE),
        "sha256": digest(policy_path),
    }:
        raise ValueError("ASSURANCE-POLICY-005: receipt policy digest is stale")
    if receipt.get("effective_date") != effective_date(root):
        raise ValueError("ASSURANCE-DATE-002: receipt effective date is stale")
    semver = receipt["gates"]["semver"]
    if semver.get("packages") != publishable_packages(root):
        raise ValueError("ASSURANCE-SEMVER-001: SemVer package inventory is stale")
    for command in receipt["commands"]:
        validate_artifact_identity(
            command["log"],
            allowed_prefixes=(TARGET_RELATIVE / "logs",),
            root=root,
        )
    validate_artifact_identity(
        receipt["gates"]["coverage"].get("report"),
        allowed_prefixes=(TARGET_RELATIVE,),
        root=root,
    )
    mutation_scopes = receipt["gates"]["mutation"].get("scopes")
    if not isinstance(mutation_scopes, dict) or not mutation_scopes:
        raise ValueError(
            "ASSURANCE-RECEIPT-013: assurance artifact identity is stale or unsafe"
        )
    for scope in mutation_scopes.values():
        validate_artifact_identity(
            scope.get("report") if isinstance(scope, dict) else None,
            allowed_prefixes=(TARGET_RELATIVE,),
            root=root,
        )
    performance = receipt["gates"]["local_performance"]
    performance_receipt = performance.get("receipt")
    expected_path = Path(policy["performance"]["local_receipt"])
    performance_path = (
        Path(performance_receipt.get("path"))
        if isinstance(performance_receipt, dict)
        and isinstance(performance_receipt.get("path"), str)
        else None
    )
    if performance_path is None or (
        performance_path != expected_path
        and not performance_path.is_relative_to(TARGET_RELATIVE)
    ):
        raise ValueError(
            "ASSURANCE-PERFORMANCE-002: local performance receipt identity is stale"
        )
    try:
        validate_artifact_identity(
            performance_receipt,
            allowed_prefixes=(expected_path, TARGET_RELATIVE),
            root=root,
        )
    except ValueError as error:
        raise ValueError(
            "ASSURANCE-PERFORMANCE-002: local performance receipt identity is stale"
        ) from error


def command_log(index: int, identifier: str, root: Path = ROOT) -> Path:
    safe = re.sub(r"[^a-z0-9-]+", "-", identifier.lower()).strip("-")
    return safe_output_path(TARGET_RELATIVE / "logs" / f"{index:02d}-{safe}.log", root)


def run_command(
    identifier: str,
    arguments: list[str],
    *,
    index: int,
    root: Path = ROOT,
    environment: dict[str, str] | None = None,
    accepted_exit_codes: set[int] | None = None,
    timeout_seconds: int = 5400,
) -> tuple[str, dict[str, Any]]:
    """Run one bounded command and retain only a private, digest-addressed log."""

    log_path = command_log(index, identifier, root)
    started = time.perf_counter()
    process = subprocess.run(
        arguments,
        cwd=root,
        env=environment,
        capture_output=True,
        text=True,
        timeout=timeout_seconds,
        check=False,
    )
    output = process.stdout + process.stderr
    log_path.write_text(output)
    accepted = accepted_exit_codes or {0}
    status = "PASS" if process.returncode in accepted else "FAIL"
    result = {
        "id": identifier,
        "command": arguments,
        "status": status,
        "exit_code": process.returncode,
        "duration_seconds": round(time.perf_counter() - started, 3),
        "log": {
            "path": str(log_path.relative_to(root)),
            "bytes": log_path.stat().st_size,
            "sha256": digest(log_path),
        },
    }
    if status != "PASS":
        raise RuntimeError(
            f"ASSURANCE-COMMAND-001: {identifier} failed with exit {process.returncode}; see {result['log']['path']}"
        )
    return process.stdout, result


def command_environment(root: Path = ROOT) -> dict[str, str]:
    environment = os.environ.copy()
    try:
        git_dir = subprocess.run(
            ["jj", "git", "root"],
            cwd=root,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
        if git_dir.returncode == 0 and git_dir.stdout.strip():
            environment["GIT_DIR"] = git_dir.stdout.strip()
    except (OSError, subprocess.TimeoutExpired):
        pass
    return environment


def tool_results(policy: dict[str, Any], root: Path = ROOT) -> dict[str, Any]:
    commands = {
        "cargo-nextest": ["cargo", "nextest", "--version"],
        "cargo-llvm-cov": ["cargo", "llvm-cov", "--version"],
        "cargo-mutants": ["cargo", "mutants", "--version"],
        "cargo-semver-checks": ["cargo", "semver-checks", "--version"],
    }
    results = {}
    for identifier, arguments in commands.items():
        process = subprocess.run(
            arguments,
            cwd=root,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
        expected = policy["tools"][identifier]
        observed = process.stdout + process.stderr
        if process.returncode != 0 or re.search(rf"\b{re.escape(expected)}\b", observed) is None:
            raise RuntimeError(
                f"ASSURANCE-TOOL-001: {identifier} {expected} is required on PATH"
            )
        results[identifier] = {"version": expected, "status": "PASS"}
    return results


def package_arguments(packages: list[str]) -> list[str]:
    return [argument for package in packages for argument in ("--package", package)]


def publishable_packages(root: Path = ROOT) -> list[str]:
    workspace = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]
    packages = []
    for member in workspace["members"]:
        manifest = tomllib.loads((root / member / "Cargo.toml").read_text())["package"]
        if manifest.get("publish", True) is not False and manifest.get("publish") != []:
            packages.append(str(manifest["name"]))
    return sorted(packages)


def baseline_package_names(
    revision: str,
    environment: dict[str, str],
    root: Path = ROOT,
) -> set[str]:
    """Read package names from one immutable Git revision without checking it out."""

    listing = subprocess.run(
        ["git", "ls-tree", "-r", "--name-only", revision],
        cwd=root,
        env=environment,
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )
    if listing.returncode != 0:
        raise RuntimeError(
            "ASSURANCE-SEMVER-003: immutable baseline tag moved or is missing"
        )
    packages: set[str] = set()
    for manifest_path in sorted(
        path for path in listing.stdout.splitlines() if path.endswith("Cargo.toml")
    ):
        manifest = subprocess.run(
            ["git", "show", f"{revision}:{manifest_path}"],
            cwd=root,
            env=environment,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
        if manifest.returncode != 0:
            raise RuntimeError(
                "ASSURANCE-SEMVER-004: baseline package inventory is unreadable"
            )
        try:
            package = tomllib.loads(manifest.stdout).get("package", {})
        except tomllib.TOMLDecodeError as error:
            raise RuntimeError(
                "ASSURANCE-SEMVER-004: baseline package inventory is unreadable"
            ) from error
        name = package.get("name") if isinstance(package, dict) else None
        if isinstance(name, str):
            packages.add(name)
    return packages


def nextest_gate(
    policy: dict[str, Any], commands: list[dict[str, Any]], root: Path = ROOT
) -> dict[str, Any]:
    packages = list(policy["nextest"]["packages"])
    selected = package_arguments(packages)
    cargo_output, result = run_command(
        "cargo-test-inventory",
        ["cargo", "test", *selected, "--all-features", "--locked", "--", "--list"],
        index=len(commands) + 1,
        root=root,
    )
    commands.append(result)
    cargo_count = sum(line.endswith(": test") for line in cargo_output.splitlines())
    nextest_output, result = run_command(
        "nextest-inventory",
        [
            "cargo",
            "nextest",
            "list",
            *selected,
            "--all-features",
            "--locked",
            "--message-format",
            "json",
        ],
        index=len(commands) + 1,
        root=root,
    )
    commands.append(result)
    nextest_count = json.loads(nextest_output)["test-count"]
    doctest_count = cargo_count - nextest_count
    _, result = run_command(
        "nextest-run",
        ["cargo", "nextest", "run", *selected, "--all-features", "--locked"],
        index=len(commands) + 1,
        root=root,
    )
    commands.append(result)
    _, result = run_command(
        "cargo-doctest-run",
        ["cargo", "test", *selected, "--all-features", "--locked", "--doc"],
        index=len(commands) + 1,
        root=root,
    )
    commands.append(result)
    gate = {
        "status": "PASS",
        "packages": packages,
        "nextest_test_count": nextest_count,
        "doctest_count": doctest_count,
        "cargo_test_count": cargo_count,
    }
    expected = policy["nextest"]
    if (
        nextest_count != expected["executable_test_count"]
        or doctest_count != expected["doctest_count"]
    ):
        raise RuntimeError(
            "ASSURANCE-NEXTEST-001: current test inventory differs from the measured baseline"
        )
    return gate


def coverage_gate(
    policy: dict[str, Any], commands: list[dict[str, Any]], root: Path = ROOT
) -> dict[str, Any]:
    report = safe_output_path(TARGET_RELATIVE / "coverage.json", root)
    if report.exists():
        report.unlink()
    packages = list(policy["coverage"]["packages"])
    _, result = run_command(
        "coverage",
        [
            "cargo",
            "llvm-cov",
            *package_arguments(packages),
            "--all-features",
            "--locked",
            "--json",
            "--summary-only",
            "--output-path",
            str(report.relative_to(root)),
        ],
        index=len(commands) + 1,
        root=root,
    )
    commands.append(result)
    document = json.loads(report.read_text())
    totals = document["data"][0]["totals"]
    gate = {
        "status": "PASS",
        "packages": packages,
        "line_percent": round(float(totals["lines"]["percent"]), 2),
        "function_percent": round(float(totals["functions"]["percent"]), 2),
        "region_percent": round(float(totals["regions"]["percent"]), 2),
        "report": {
            "path": str(report.relative_to(root)),
            "bytes": report.stat().st_size,
            "sha256": digest(report),
        },
    }
    if gate["line_percent"] < policy["coverage"]["minimum_line_percent"]:
        raise RuntimeError("ASSURANCE-COVERAGE-001: measured line coverage is below policy")
    if gate["function_percent"] < policy["coverage"]["minimum_function_percent"]:
        raise RuntimeError("ASSURANCE-COVERAGE-002: measured function coverage is below policy")
    return gate


def reset_private_directory(path: Path, root: Path = ROOT) -> None:
    target = (root / TARGET_RELATIVE).resolve()
    resolved = path.resolve(strict=False)
    if not resolved.is_relative_to(target) or resolved == target or path.is_symlink():
        raise ValueError("ASSURANCE-PATH-002: refusing unsafe private-directory replacement")
    if path.exists():
        shutil.rmtree(path)


def mutation_command(
    *,
    identifier: str,
    package: str,
    file: str,
    function_regex: str,
    output: Path,
    commands: list[dict[str, Any]],
    root: Path,
) -> dict[str, Any]:
    reset_private_directory(output, root)
    _, result = run_command(
        identifier,
        [
            "cargo",
            "mutants",
            "--no-config",
            "--no-times",
            "--colors",
            "never",
            "--annotations",
            "none",
            "--baseline",
            "run",
            "--build-timeout",
            "180",
            "--minimum-test-timeout",
            "20",
            "-j",
            "4",
            "--package",
            package,
            "--file",
            file,
            "--re",
            function_regex,
            "--output",
            str(output.relative_to(root)),
        ],
        index=len(commands) + 1,
        root=root,
    )
    commands.append(result)
    report = output / "mutants.out" / "outcomes.json"
    document = json.loads(report.read_text())
    return {
        key: int(document[key])
        for key in ("total_mutants", "caught", "missed", "timeout", "unviable")
    } | {
        "report": {
            "path": str(report.relative_to(root)),
            "bytes": report.stat().st_size,
            "sha256": digest(report),
        }
    }


def mutation_gate(
    policy: dict[str, Any], commands: list[dict[str, Any]], root: Path = ROOT
) -> dict[str, Any]:
    settings = policy["mutation"]
    plan = mutation_command(
        identifier="mutation-plan-cost",
        package="minco-plan",
        file="crates/minco-plan/src/cost.rs",
        function_regex=settings["plan_function_regex"],
        output=root / TARGET_RELATIVE / "mutants-plan",
        commands=commands,
        root=root,
    )
    release = mutation_command(
        identifier="mutation-release-authority",
        package="minco-release",
        file="crates/minco-release/src/lib.rs",
        function_regex=settings["release_function_regex"],
        output=root / TARGET_RELATIVE / "mutants-release",
        commands=commands,
        root=root,
    )
    gate = {
        "status": "PASS",
        "total_mutants": plan["total_mutants"] + release["total_mutants"],
        "caught": plan["caught"] + release["caught"],
        "missed": plan["missed"] + release["missed"],
        "timeouts": plan["timeout"] + release["timeout"],
        "unviable": plan["unviable"] + release["unviable"],
        "scopes": {"plan_cost": plan, "release_authority": release},
    }
    if (
        gate["total_mutants"] != settings["total_mutants"]
        or gate["caught"] < settings["minimum_caught"]
        or gate["missed"] > settings["maximum_missed"]
        or gate["timeouts"] > settings["maximum_timeouts"]
        or gate["unviable"] > settings["maximum_unviable"]
    ):
        raise RuntimeError("ASSURANCE-MUTATION-001: mutation evidence exceeds policy")
    return gate


def semver_gate(
    policy: dict[str, Any], commands: list[dict[str, Any]], root: Path = ROOT
) -> dict[str, Any]:
    settings = policy["semver"]
    packages = publishable_packages(root)
    new_packages = list(settings["new_packages"])
    checked_packages = [package for package in packages if package not in new_packages]
    if (
        len(packages) != settings["package_count"]
        or any(package not in packages for package in new_packages)
        or len(checked_packages) != settings["baseline_package_count"]
    ):
        raise RuntimeError(
            "ASSURANCE-SEMVER-001: publishable package inventory differs from policy"
        )
    environment = command_environment(root)
    if "GIT_DIR" not in environment and not (root / ".git").exists():
        raise RuntimeError("ASSURANCE-SEMVER-002: colocated Git repository is unavailable")
    git = subprocess.run(
        ["git", "rev-parse", f"{settings['baseline_tag']}^{{commit}}"],
        cwd=root,
        env=environment,
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )
    if git.returncode != 0 or git.stdout.strip() != settings["baseline_commit"]:
        raise RuntimeError("ASSURANCE-SEMVER-003: immutable baseline tag moved or is missing")
    baseline_packages = baseline_package_names(
        settings["baseline_tag"], environment, root
    )
    if (
        any(package not in baseline_packages for package in checked_packages)
        or any(package in baseline_packages for package in new_packages)
    ):
        raise RuntimeError(
            "ASSURANCE-SEMVER-004: reviewed new-package boundary differs from the baseline"
        )
    _, result = run_command(
        "semver",
        [
            "cargo",
            "semver-checks",
            *package_arguments(checked_packages),
            "--baseline-rev",
            settings["baseline_tag"],
            "--all-features",
            "--color",
            "never",
        ],
        index=len(commands) + 1,
        root=root,
        environment=environment,
    )
    commands.append(result)
    return {
        "status": "PASS",
        "baseline_tag": settings["baseline_tag"],
        "baseline_commit": settings["baseline_commit"],
        "package_count": len(packages),
        "checked_package_count": len(checked_packages),
        "packages": packages,
        "checked_packages": checked_packages,
        "new_packages": new_packages,
    }


def local_performance_gate(
    policy: dict[str, Any],
    source: dict[str, Any],
    commands: list[dict[str, Any]],
    root: Path = ROOT,
    *,
    receipt_relative: Path | None = None,
) -> dict[str, Any]:
    receipt_relative = receipt_relative or Path(policy["performance"]["local_receipt"])
    if receipt_relative != Path(policy["performance"]["local_receipt"]):
        safe_output_path(receipt_relative, root)
    receipt_path = root / receipt_relative
    _, result = run_command(
        "local-performance",
        [
            "scripts/release/candidate-load.sh",
            "--output",
            str(receipt_relative),
        ],
        index=len(commands) + 1,
        root=root,
    )
    commands.append(result)
    receipt = json.loads(receipt_path.read_text())
    if (
        receipt.get("schema_version") != 2
        or receipt.get("kind") != "minco.candidate-load-qualification.v2"
        or receipt.get("status") != "PASS"
        or receipt.get("production_slo") is not False
        or receipt.get("provider_contact") is not False
        or receipt.get("runner", {}).get("scope") != "local"
        or receipt.get("source", {}).get("version") != source["version"]
        or receipt.get("source", {}).get("source_tree_sha256")
        != source["source_tree_sha256"]
    ):
        raise RuntimeError(
            "ASSURANCE-PERFORMANCE-001: local performance receipt exceeds or differs from current authority"
        )
    return {
        "status": "PASS",
        "runner_scope": "local",
        "production_slo": False,
        "provider_contact": False,
        "api_requests": receipt["api"]["requests"],
        "api_p95_ms": receipt["api"]["latency"]["p95_ms"],
        "api_p99_ms": receipt["api"]["latency"]["p99_ms"],
        "api_throughput_requests_per_second": receipt["api"][
            "throughput_requests_per_second"
        ],
        "worker_messages": receipt["worker"]["messages"],
        "worker_throughput_messages_per_second": receipt["worker"][
            "throughput_messages_per_second"
        ],
        "receipt": {
            "path": str(receipt_relative),
            "bytes": receipt_path.stat().st_size,
            "sha256": digest(receipt_path),
        },
        "hosted_baseline_status": "NOT RUN",
    }


def runner_identity() -> dict[str, Any]:
    dimensions = {
        "os": platform.system().lower(),
        "os_release": platform.release(),
        "architecture": platform.machine().lower(),
        "python": platform.python_version(),
    }
    return {
        "scope": "local",
        "dimensions": dimensions,
        "fingerprint_sha256": hashlib.sha256(
            json.dumps(dimensions, separators=(",", ":"), sort_keys=True).encode()
        ).hexdigest(),
    }


def effective_date(root: Path = ROOT) -> str:
    value = tomllib.loads(
        (root / "verification" / "repository-truth.toml").read_text()
    ).get("operational_evidence_effective_date")
    if isinstance(value, datetime.date):
        return value.isoformat()
    if isinstance(value, str):
        try:
            parsed = datetime.date.fromisoformat(value)
        except ValueError as error:
            raise ValueError(
                "ASSURANCE-DATE-001: operational effective date must be exact YYYY-MM-DD"
            ) from error
        if parsed.isoformat() == value:
            return value
    raise ValueError(
        "ASSURANCE-DATE-001: operational effective date must be exact YYYY-MM-DD"
    )


def execute(
    policy: dict[str, Any],
    root: Path = ROOT,
    *,
    performance_receipt_relative: Path | None = None,
) -> dict[str, Any]:
    source = load_source_manifest(root)
    commands: list[dict[str, Any]] = []
    gates = {
        "nextest_parity": nextest_gate(policy, commands, root),
        "coverage": coverage_gate(policy, commands, root),
        "mutation": mutation_gate(policy, commands, root),
        "semver": semver_gate(policy, commands, root),
        "local_performance": local_performance_gate(
            policy,
            source,
            commands,
            root,
            receipt_relative=performance_receipt_relative,
        ),
    }
    receipt = {
        "schema_version": 1,
        "kind": "minco.quality-assurance.v1",
        "status": "PASS",
        "production_slo": False,
        "provider_contact": False,
        "effective_date": effective_date(root),
        "source": {
            "version": source["version"],
            "source_tree_sha256": source["source_tree_sha256"],
        },
        "policy": {
            "path": str(POLICY_RELATIVE),
            "sha256": digest(root / POLICY_RELATIVE),
        },
        "runner": runner_identity(),
        "tools": tool_results(policy, root),
        "gates": gates,
        "commands": commands,
        "limitations": [
            "Coverage and mutation baselines cover only the reviewed core, Plan and release scopes.",
            "Nextest does not execute doctests; the separate Cargo doctest command preserves that lane.",
            "Local macOS performance is machine-specific diagnostic evidence, not hosted Linux, AWS, production or SLO proof.",
            "SemVer checks are an additional compatibility signal and do not prove serialized schema or behavioral compatibility.",
        ],
    }
    validate_current_receipt(receipt, policy, root=root)
    return receipt


def write_receipt(receipt: dict[str, Any], requested: Path, root: Path = ROOT) -> Path:
    output = safe_output_path(requested, root)
    temporary = output.with_name(f".{output.name}.tmp")
    if temporary.is_symlink() or (temporary.exists() and not temporary.is_file()):
        raise ValueError("ASSURANCE-PATH-003: temporary assurance output is unsafe")
    rendered = json.dumps(receipt, allow_nan=False, indent=2, sort_keys=True) + "\n"
    with temporary.open("w") as handle:
        handle.write(rendered)
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, output)
    return output


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--execute", action="store_true")
    parser.add_argument("--output", type=Path, default=OUTPUT_RELATIVE)
    parser.add_argument("--check-output", type=Path)
    parser.add_argument("--performance-output", type=Path)
    parser.add_argument(
        "--tool-root",
        type=Path,
        help="prepend an explicitly selected Cargo installation root to PATH",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_arguments()
    if args.tool_root is not None:
        bin_path = args.tool_root.resolve() / "bin"
        os.environ["PATH"] = f"{bin_path}{os.pathsep}{os.environ.get('PATH', '')}"
    policy = load_policy()
    if args.execute and args.check_output is not None:
        raise SystemExit("--execute and --check-output are mutually exclusive")
    if args.execute:
        if args.performance_output is not None and args.performance_output == args.output:
            raise ValueError(
                "ASSURANCE-PATH-001: performance and assurance outputs must differ"
            )
        receipt = execute(
            policy,
            performance_receipt_relative=args.performance_output,
        )
        output = write_receipt(receipt, args.output)
        print(f"Quality assurance PASS: {output.relative_to(ROOT)}")
        return 0
    if args.check_output is not None:
        path = safe_output_path(args.check_output)
        validate_current_receipt(load_canonical_receipt(path), policy)
        print(f"Verified {path.relative_to(ROOT)}.")
        return 0
    print(f"Validated {POLICY_RELATIVE}.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (
        AttributeError,
        KeyError,
        TypeError,
        ValueError,
        RuntimeError,
        OSError,
        subprocess.SubprocessError,
    ) as error:
        print(error, file=sys.stderr)
        raise SystemExit(1) from None
