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
RELEASE = json.loads((SITE / "release.json").read_text())
STABLE_VERSION = RELEASE["stable"]
WORKSPACE_VERSION = RELEASE["workspace"]
TUTORIAL = SITE / STABLE_VERSION / "tutorials"
WORKSPACE_TUTORIAL = SITE / WORKSPACE_VERSION / "tutorials"
NEXT = SITE / "next"
REQUIRED_NEXT_PAGES = (
    "getting-started/installation.md",
    "getting-started/first-application.md",
    "features/index.md",
    "guides/configuration.md",
    "guides/local-development.md",
    "guides/database-lifecycle.md",
    "guides/background-work.md",
    "guides/identity-and-sessions.md",
    "guides/files-and-static-sites.md",
    "guides/events-and-notifications.md",
    "guides/feedback.md",
    "plugins/index.md",
    "plugins/using-plugins.md",
    "cookbook/index.md",
    "cookbook/orders-api.md",
    "reference/feature-flags.md",
)


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
    tutorial_roots = {TUTORIAL, WORKSPACE_TUTORIAL}
    for tutorial_root in sorted(tutorial_roots):
        version = tutorial_root.parent.name
        for tutorial in sorted(tutorial_root.glob("*.md")):
            source = tutorial.read_text()
            for marker in (f"minco_version: {version}", "rust_version: 1.97.1"):
                if marker not in source:
                    fail(f"{tutorial.relative_to(ROOT)} lacks {marker}", failures)

    stable_sources = []
    checked_sources = sorted((SITE / STABLE_VERSION).rglob("*.md"))
    if WORKSPACE_VERSION != STABLE_VERSION:
        checked_sources.extend(sorted((SITE / WORKSPACE_VERSION).rglob("*.md")))
    next_sources = sorted(NEXT.rglob("*.md"))
    if len(next_sources) < 28:
        fail(
            f"next documentation has {len(next_sources)} pages; expected at least 28 detailed pages",
            failures,
        )
    for relative_path in REQUIRED_NEXT_PAGES:
        if not (NEXT / relative_path).is_file():
            fail(f"next documentation lacks required page: {relative_path}", failures)
    checked_sources.extend(next_sources)
    for path in checked_sources:
        source = path.read_text()
        if path.is_relative_to(SITE / STABLE_VERSION):
            stable_sources.append(source)
        for match in FENCE.finditer(source):
            checked += 1
            check_fence(path, match.group(1), match.group(2), failures)

    combined = "\n".join(stable_sources).lower()
    for forbidden in (
        "candidate publication status",
        f"{STABLE_VERSION} candidate",
        f"{STABLE_VERSION} is unpublished",
    ):
        if forbidden in combined:
            fail(f"stable documentation contains stale release language: {forbidden}", failures)

    next_layout = (SITE / ".vitepress" / "theme" / "Layout.vue").read_text()
    for marker in ("relativePath.startsWith('next/')", "Unreleased documentation."):
        if marker not in next_layout:
            fail(f"next documentation layout lacks persistent marker: {marker}", failures)

    if RELEASE["state"] == "candidate":
        for marker in (
            "release.state === 'candidate'",
            "Release candidate documentation.",
        ):
            if marker not in next_layout:
                fail(f"candidate documentation layout lacks marker: {marker}", failures)

    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print(f"Documentation snippets passed: {checked} fenced blocks.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
