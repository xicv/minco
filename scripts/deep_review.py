#!/usr/bin/env python3
"""Opinionated source review producing machine-readable evidence."""
from __future__ import annotations

import json
import re
import subprocess
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    static = subprocess.run(
        ["python3", str(ROOT / "scripts/validate_static.py")],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    try:
        static_report = json.loads(static.stdout)
    except json.JSONDecodeError:
        static_report = {"status": "failed", "raw_stdout": static.stdout, "raw_stderr": static.stderr}

    findings = []
    metrics = Counter()
    for path in sorted(ROOT.rglob("*.rs")):
        if "target" in path.parts:
            continue
        text = path.read_text()
        rel = str(path.relative_to(ROOT))
        metrics["rust_files"] += 1
        metrics["rust_lines"] += len(text.splitlines())
        metrics["unsafe_tokens"] += len(re.findall(r"\bunsafe\b", text))
        production = "#[cfg(test)]" not in text or text.index("#[cfg(test)]") > 0
        pre_test = text.split("#[cfg(test)]", 1)[0] if production else text
        unwraps = len(re.findall(r"\.(?:unwrap|expect)\s*\(", pre_test))
        metrics["production_unwrap_expect"] += unwraps
        if unwraps > 8:
            findings.append({"severity": "warning", "code": "REVIEW-RUST-001", "path": rel, "message": f"{unwraps} unwrap/expect calls before test code"})
        long_lines = [i for i, line in enumerate(text.splitlines(), 1) if len(line) > 120]
        if len(long_lines) > 20:
            findings.append({"severity": "warning", "code": "REVIEW-RUST-002", "path": rel, "message": f"{len(long_lines)} lines exceed 120 columns"})
        if "Box<dyn std::error::Error" in pre_test or "Box<dyn Error" in pre_test:
            findings.append({"severity": "information", "code": "REVIEW-RUST-003", "path": rel, "message": "dynamic error type in production path; confirm boundary use"})

    for path in sorted(ROOT.rglob("*.sql")):
        text = path.read_text().upper()
        rel = str(path.relative_to(ROOT))
        metrics["sql_files"] += 1
        for destructive in ["DROP TABLE", "TRUNCATE ", "DROP SCHEMA"]:
            if destructive in text:
                findings.append({"severity": "warning", "code": "REVIEW-SQL-001", "path": rel, "message": f"migration contains {destructive.strip()}"})

    for path in sorted(ROOT.rglob("*.yaml")):
        text = path.read_text()
        rel = str(path.relative_to(ROOT))
        for secret_pattern in [r"AKIA[0-9A-Z]{16}", r"postgres(?:ql)?://[^<\s]+:[^<\s]+@"]:
            if re.search(secret_pattern, text):
                findings.append({"severity": "error", "code": "REVIEW-SECRET-001", "path": rel, "message": "possible committed credential"})

    errors = sum(item["severity"] == "error" for item in findings)
    report = {
        "schema_version": 1,
        "status": "ok" if static_report.get("status") == "ok" and errors == 0 else "failed",
        "static_validation": static_report,
        "metrics": dict(metrics),
        "findings": findings,
        "compiler_verification": {
            "performed": False,
            "reason": "set by the invoking environment; see VERIFICATION.md",
        },
    }
    output = ROOT / "target/minco/deep-review.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["status"] == "ok" else 1


if __name__ == "__main__":
    raise SystemExit(main())
