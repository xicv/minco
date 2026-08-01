#!/usr/bin/env python3
"""Validate version markers and syntax-check documentation code fences."""
from __future__ import annotations

import json
import re
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
SITE = ROOT / "docs-site"
FENCE = re.compile(r"^```([^\n]*)\n(.*?)^```\s*$", re.MULTILINE | re.DOTALL)
TUTORIAL = SITE / "0.5.0" / "tutorials"
NEXT = SITE / "next"


def fail(message: str, failures: list[str]) -> None:
    failures.append(message)


def check_fence(path: Path, language: str, body: str, failures: list[str]) -> None:
    language = language.split(",", 1)[0].strip()
    try:
        if language == "json":
            json.loads(body)
        elif language == "toml":
            tomllib.loads(body)
        elif language in {"yaml", "yml"}:
            yaml.safe_load(body)
        elif language in {"bash", "sh"}:
            with tempfile.NamedTemporaryFile("w", suffix=".sh") as script:
                script.write(body)
                script.flush()
                result = subprocess.run(
                    ["bash", "-n", script.name],
                    capture_output=True,
                    text=True,
                    check=False,
                )
            if result.returncode:
                raise ValueError(result.stderr.strip())
    except (json.JSONDecodeError, tomllib.TOMLDecodeError, yaml.YAMLError, ValueError) as error:
        fail(f"{path.relative_to(ROOT)} [{language}]: {error}", failures)


def main() -> int:
    failures: list[str] = []
    checked = 0
    for tutorial in sorted(TUTORIAL.glob("*.md")):
        source = tutorial.read_text()
        for marker in ("minco_version: 0.5.0", "rust_version: 1.97.1"):
            if marker not in source:
                fail(f"{tutorial.relative_to(ROOT)} lacks {marker}", failures)

    stable_sources = []
    checked_sources = sorted((SITE / "0.5.0").rglob("*.md"))
    next_sources = sorted(NEXT.rglob("*.md"))
    if len(next_sources) < 10:
        fail(
            f"next documentation has {len(next_sources)} pages; expected at least 10 detailed pages",
            failures,
        )
    checked_sources.extend(next_sources)
    for path in checked_sources:
        source = path.read_text()
        if path.is_relative_to(SITE / "0.5.0"):
            stable_sources.append(source)
        for match in FENCE.finditer(source):
            checked += 1
            check_fence(path, match.group(1), match.group(2), failures)

    combined = "\n".join(stable_sources).lower()
    for forbidden in ("candidate publication status", "0.5.0 candidate", "0.5.0 is unpublished"):
        if forbidden in combined:
            fail(f"stable documentation contains stale release language: {forbidden}", failures)

    next_layout = (SITE / ".vitepress" / "theme" / "Layout.vue").read_text()
    for marker in ("relativePath.startsWith('next/')", "Unreleased documentation."):
        if marker not in next_layout:
            fail(f"next documentation layout lacks persistent marker: {marker}", failures)

    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print(f"Documentation snippets passed: {checked} fenced blocks.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
