#!/usr/bin/env python3
"""Conservative direct-dependency audit for Rust workspace packages.

Cargo requires crates referenced by a package's Rust source to be declared in
that package, even when another dependency also uses the crate. This check is
not a compiler replacement; it catches a stable set of common missing direct
dependencies before the real Cargo gates run.
"""
from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
KNOWN_ROOTS = {
    "anyhow", "async_trait", "aws_config", "aws_sdk_ssm", "axum", "base64",
    "bytes", "chrono", "clap", "hmac", "http", "http_body_util", "lambda_http",
    "lambda_runtime", "reqwest", "semver", "serde", "serde_json", "serde_yaml_ng",
    "sha2", "sqlx", "subtle", "tempfile", "thiserror", "tokio", "toml", "tower",
    "tower_http", "tracing", "tracing_subscriber", "uuid",
}
ROOT_PATTERN = re.compile(r"(?<![A-Za-z0-9_])([A-Za-z_][A-Za-z0-9_]*)::")


def strip_comments_and_literals(source: str) -> str:
    out: list[str] = []
    i = 0
    block_depth = 0
    while i < len(source):
        if block_depth:
            if source.startswith("/*", i):
                block_depth += 1
                out.extend("  ")
                i += 2
            elif source.startswith("*/", i):
                block_depth -= 1
                out.extend("  ")
                i += 2
            else:
                out.append("\n" if source[i] == "\n" else " ")
                i += 1
            continue
        if source.startswith("//", i):
            end = source.find("\n", i)
            if end == -1:
                out.extend(" " * (len(source) - i))
                break
            out.extend(" " * (end - i))
            out.append("\n")
            i = end + 1
            continue
        if source.startswith("/*", i):
            block_depth = 1
            out.extend("  ")
            i += 2
            continue
        # Rust raw strings: r"...", r#"..."#, br#"..."#.
        match = re.match(r"(?:b)?r(#+)?\"", source[i:])
        if match:
            hashes = match.group(1) or ""
            prefix_len = match.end()
            terminator = '"' + hashes
            end = source.find(terminator, i + prefix_len)
            end = len(source) if end == -1 else end + len(terminator)
            segment = source[i:end]
            out.extend("\n" if char == "\n" else " " for char in segment)
            i = end
            continue
        if source[i] in {'"', "'"}:
            quote = source[i]
            # A lifetime such as 'a is not a character literal.
            if quote == "'" and i + 1 < len(source) and source[i + 1].isalpha():
                out.append(source[i])
                i += 1
                continue
            start = i
            i += 1
            escaped = False
            while i < len(source):
                char = source[i]
                i += 1
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == quote:
                    break
            segment = source[start:i]
            out.extend("\n" if char == "\n" else " " for char in segment)
            continue
        out.append(source[i])
        i += 1
    return "".join(out)


def dependency_roots(document: dict) -> set[str]:
    roots: set[str] = set()
    for section in ("dependencies", "dev-dependencies", "build-dependencies"):
        for alias, spec in document.get(section, {}).items():
            package = spec.get("package", alias) if isinstance(spec, dict) else alias
            roots.add(alias.replace("-", "_"))
            roots.add(str(package).replace("-", "_"))
    return roots


def source_files(package: Path) -> list[Path]:
    candidates: list[Path] = []
    for directory in ("src", "tests", "examples", "benches"):
        root = package / directory
        if root.exists():
            candidates.extend(root.rglob("*.rs"))
    build = package / "build.rs"
    if build.exists():
        candidates.append(build)
    return sorted(set(candidates))


def main() -> int:
    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())
    members = workspace["workspace"]["members"]
    internal_roots = {
        tomllib.loads((ROOT / member / "Cargo.toml").read_text())["package"]["name"].replace(
            "-", "_"
        )
        for member in members
    }
    known = KNOWN_ROOTS | internal_roots
    findings: list[dict[str, str]] = []

    for member in members:
        package = ROOT / member
        cargo = package / "Cargo.toml"
        document = tomllib.loads(cargo.read_text())
        declared = dependency_roots(document)
        own_root = document["package"]["name"].replace("-", "_")
        referenced: dict[str, Path] = {}
        for path in source_files(package):
            source = strip_comments_and_literals(path.read_text())
            for root in ROOT_PATTERN.findall(source):
                if root in known and root != own_root:
                    referenced.setdefault(root, path)
        for root in sorted(set(referenced) - declared):
            findings.append({
                "package": document["package"]["name"],
                "dependency_root": root,
                "path": str(referenced[root].relative_to(ROOT)),
            })

    report = {
        "schema_version": 1,
        "status": "passed" if not findings else "failed",
        "workspace_packages": len(members),
        "findings": findings,
        "limitations": [
            "This is a conservative lexical check for known crate roots, not Cargo resolution or compilation."
        ],
    }
    output = ROOT / "verification/rust-dependency-hygiene.json"
    output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if not findings else 1


if __name__ == "__main__":
    sys.exit(main())
