#!/usr/bin/env python3
"""Run and record every mandatory current-workspace candidate release gate."""

from __future__ import annotations

import argparse
import hashlib
import json
import shlex
import shutil
import subprocess
import time
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from candidate_qualification import (
    MANDATORY_RELEASE_COMMANDS,
    RELEASE_SERIES,
    ROOT,
    safe_output_path,
    validate_release_gate_record,
)


DEFAULT_OUTPUT = (
    ROOT / "verification" / f"{RELEASE_SERIES}-candidate-release-gates.json"
)


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def executable_available(argument: str) -> bool:
    if "/" in argument:
        path = ROOT / argument
        return path.is_file() and bool(path.stat().st_mode & 0o111)
    return shutil.which(argument) is not None


def run_command(command: str, index: int, log_dir: Path) -> dict[str, Any]:
    arguments = shlex.split(command)
    log_path = log_dir / f"{index:02d}.log"
    if not executable_available(arguments[0]):
        return {
            "command": command,
            "status": "BLOCKED",
            "exit_code": None,
            "reason": f"required executable unavailable: {arguments[0]}",
            "duration_seconds": 0.0,
            "log": None,
        }
    started = time.perf_counter()
    with log_path.open("w") as log:
        try:
            process = subprocess.run(
                arguments,
                cwd=ROOT,
                stdout=log,
                stderr=subprocess.STDOUT,
                text=True,
                timeout=5400,
                check=False,
            )
            status = "PASS" if process.returncode == 0 else "FAIL"
            exit_code: int | None = process.returncode
            reason = None
        except subprocess.TimeoutExpired:
            status = "FAIL"
            exit_code = None
            reason = "command exceeded the 5400-second gate timeout"
    result: dict[str, Any] = {
        "command": command,
        "status": status,
        "exit_code": exit_code,
        "duration_seconds": round(time.perf_counter() - started, 3),
        "log": {
            "path": str(log_path.relative_to(ROOT)),
            "bytes": log_path.stat().st_size,
            "sha256": digest(log_path),
        },
    }
    if reason is not None:
        result["reason"] = reason
    return result


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    return parser.parse_args()


def main() -> int:
    args = parse_arguments()
    log_dir = ROOT / "target" / "minco" / "candidate-release-gates" / "logs"
    log_dir.mkdir(parents=True, exist_ok=True)
    commands = [
        run_command(command, index, log_dir)
        for index, command in enumerate(MANDATORY_RELEASE_COMMANDS, start=1)
    ]
    status = "PASS" if all(command["status"] == "PASS" for command in commands) else "FAIL"
    manifest = json.loads((ROOT / "verification" / "source-manifest.json").read_text())
    record = {
        "schema_version": 1,
        "kind": "minco.candidate-release-qualification.v1",
        "status": status,
        "generated_at": datetime.now(UTC).isoformat(),
        "source": {
            "version": manifest["version"],
            "source_tree_sha256": manifest["source_tree_sha256"],
            "file_count": manifest["file_count"],
        },
        "commands": commands,
        "security": {
            "status": next(item["status"] for item in commands if item["command"] == "./scripts/quality.sh"),
            "critical_or_high_findings": 0 if status == "PASS" else None,
            "covered_by_quality": [
                "cargo deny advisories bans licenses sources",
                "cargo audit",
                "npm audit --audit-level=high",
                "gitleaks --redact",
                "deep review",
            ],
        },
        "documentation_and_consumers": {
            "status": next(item["status"] for item in commands if item["command"] == "./scripts/quality.sh"),
            "covered_by_quality": [
                "generated reference drift",
                "documentation snippets",
                "documentation build and links",
                "desktop and small-screen browser journeys",
                "generated PostgreSQL and SQLite external applications",
            ],
        },
        "provider": {
            "bounded_rehearsal": {
                "status": "PASS",
                "task": "M10-T08",
                "source_revisions": [
                    "9cbe8fdb64a6f68363fd1cac949ddfa554106667",
                    "4573239d83fff91fffd79ea9bda58afbe217ffe9",
                ],
                "phases": ["prior", "current", "prior rollback"],
                "cleanup_boundaries_absent": 14,
            },
            "exact_current_candidate_redeployment": {
                "status": "NOT RUN",
                "reason": "M12-T05 had no fresh exact-account/provider authority; local and historical provider evidence are kept separate.",
            },
        },
        "publication": {
            "status": "NOT RUN",
            "reason": "Only the package/publish dry run is authorized; no tag, crate upload, docs publication, deployment or promotion occurred.",
        },
        "limitations": [
            "Local and emulator gates do not prove current AWS managed-service behavior.",
            "The completed M10 provider rehearsal remains bound to its exact source revisions and does not become an exact-current redeployment claim.",
            "Machine-specific load measurements are bounded smoke evidence, not a production SLO or current provider price quote.",
        ],
    }
    validate_release_gate_record(record)
    output = safe_output_path(args.output)
    output.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    print(f"Candidate release qualification {status}: {output}")
    return 0 if status == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
