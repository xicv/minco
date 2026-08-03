#!/usr/bin/env python3
"""Generate deterministic Minco reference pages from repository authorities."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tomllib
from collections.abc import Iterable, Mapping
from pathlib import Path
from typing import Any

GENERATOR_SCHEMA = 1
OUTPUT_DIRECTORY = Path("docs/reference/generated")
SOURCE_ROOT = Path(__file__).resolve().parents[2]


def read_toml(path: Path) -> dict[str, Any]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def repository_file(root: Path, relative: Path, label: str) -> Path:
    root = root.resolve()
    candidate = root / relative
    if candidate.is_symlink():
        raise ValueError(f"{label} cannot be a symlink")
    resolved = candidate.resolve(strict=True)
    if not resolved.is_file() or not resolved.is_relative_to(root):
        raise ValueError(f"{label} must be a regular file inside the repository")
    return resolved


def markdown_cell(value: Any) -> str:
    if isinstance(value, bool):
        rendered = "yes" if value else "no"
    elif value is None:
        rendered = "—"
    elif isinstance(value, (dict, list)):
        rendered = json.dumps(value, sort_keys=True, separators=(",", ":"))
    elif isinstance(value, str) and (not value or value != value.strip()):
        rendered = json.dumps(value)
    else:
        rendered = str(value)
    return rendered.replace("|", "\\|").replace("\n", " ")


def generated_header(title: str, authorities: Iterable[str]) -> str:
    authority_lines = "\n".join(f"- `{authority}`" for authority in authorities)
    return (
        f"# {title}\n\n"
        "<!-- @generated; do not edit by hand -->\n"
        f"<!-- generated-reference-schema: {GENERATOR_SCHEMA} -->\n\n"
        f"Generator: `scripts/docs/generate_reference.py` schema `{GENERATOR_SCHEMA}`.\n\n"
        "Authorities:\n\n"
        f"{authority_lines}\n\n"
        "Regenerate with `scripts/docs/generate-reference.sh`; use `--check` to "
        "verify byte-for-byte freshness.\n\n"
    )


def workspace_metadata(root: Path) -> tuple[dict[str, Any], dict[str, Any]]:
    workspace = read_toml(root / "Cargo.toml")
    facade = read_toml(root / "crates/minco/Cargo.toml")
    return workspace, facade


def package_path(workspace: Mapping[str, Any], package: str) -> Path:
    dependency = workspace["workspace"]["dependencies"].get(package)
    if not isinstance(dependency, dict) or "path" not in dependency:
        raise ValueError(f"publishable package {package} has no workspace path")
    path = Path(str(dependency["path"]))
    if path.is_absolute() or ".." in path.parts:
        raise ValueError(f"publishable package {package} has an unsafe path")
    return path


def render_packages(root: Path, workspace: Mapping[str, Any]) -> str:
    workspace_package = workspace["workspace"]["package"]
    release = workspace["workspace"]["metadata"]["minco"]["release"]
    publish = list(release["publish"])
    package_tests = set(release.get("package_tests", []))
    version = str(workspace_package["version"])
    output = generated_header(
        "Package reference",
        [
            "Cargo.toml [workspace.package]",
            "Cargo.toml [workspace.metadata.minco.release]",
            "each publishable package Cargo.toml",
        ],
    )
    output += (
        f"Workspace version: `{version}`. MSRV: `{workspace_package['rust-version']}`. "
        f"Publishable packages: `{len(publish)}`.\n\n"
        "Publication is dependency ordered. A docs.rs link is present for every public "
        "package; archive-smoke packages are the subset exercised independently before "
        "publication.\n\n"
        "| Order | Package | Purpose | Archive smoke | Rust API |\n"
        "|---:|---|---|:---:|---|\n"
    )
    for order, name in enumerate(publish, start=1):
        relative = package_path(workspace, name)
        manifest_path = repository_file(
            root, relative / "Cargo.toml", f"package {name} manifest"
        )
        manifest = read_toml(manifest_path)["package"]
        if manifest.get("name") != name:
            raise ValueError(f"workspace package {name} resolves to {manifest.get('name')}")
        crate_name = name.replace("-", "_")
        docs = f"https://docs.rs/{name}/{version}/{crate_name}/"
        output += (
            f"| {order} | `{name}` | {markdown_cell(manifest.get('description', ''))} | "
            f"{'yes' if name in package_tests else 'no'} | [docs.rs]({docs}) |\n"
        )
    return output


def feature_kind(name: str) -> str:
    if name == "default" or name in {"full", "official-plugins", "default-plugins"}:
        return "bundle"
    if name.startswith("plugin-"):
        return "plugin"
    if name.startswith("sqlx-"):
        return "database adapter"
    if name.startswith("aws-"):
        return "AWS adapter/runtime"
    return "framework plane"


def render_features(facade: Mapping[str, Any]) -> str:
    features = facade["features"]
    output = generated_header(
        "Facade feature reference",
        [
            "crates/minco/Cargo.toml [features]",
            "crates/minco/Cargo.toml [dependencies]",
        ],
    )
    output += (
        "Features are compile-time composition only. They do not discover plugins, "
        "select providers at runtime, or create AWS resources by themselves.\n\n"
        "| Feature | Kind | Enables |\n"
        "|---|---|---|\n"
    )
    for name in sorted(features):
        enabled = features[name]
        rendered = ", ".join(f"`{item}`" for item in enabled) if enabled else "—"
        output += f"| `{name}` | {feature_kind(name)} | {rendered} |\n"
    return output


def observed_type(value: Any) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, int):
        return "integer"
    if isinstance(value, float):
        return "number"
    if isinstance(value, str):
        return "string"
    if isinstance(value, list):
        return "array"
    if isinstance(value, dict):
        return "object"
    return type(value).__name__


def canonical_digest(value: Any) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def field_inventory(records: list[Mapping[str, Any]]) -> list[tuple[str, str, bool]]:
    fields = sorted({key for record in records for key in record})
    inventory = []
    for field in fields:
        types = sorted({observed_type(record[field]) for record in records if field in record})
        required = all(field in record for record in records)
        inventory.append((field, " or ".join(types), required))
    return inventory


def load_plugins(root: Path) -> list[dict[str, Any]]:
    catalog = read_toml(root / "plugins/catalog.toml")
    plugins = []
    for entry in catalog.get("plugin", []):
        relative = Path(entry["path"])
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError(f"plugin {entry['id']} has an unsafe path")
        distribution_path = repository_file(
            root,
            relative / "minco-plugin.json",
            f"plugin {entry['id']} distribution manifest",
        )
        if distribution_path.stat().st_size > 1024 * 1024:
            raise ValueError(f"plugin {entry['id']} distribution manifest exceeds 1 MiB")
        distribution = json.loads(distribution_path.read_text())
        for key in ("id", "kind", "feature", "default_enabled", "stability"):
            if entry[key] != distribution[key]:
                raise ValueError(f"plugin {entry['id']} disagrees on {key}")
        plugins.append({**entry, "distribution": distribution})
    return plugins


def render_plugins(root: Path) -> str:
    plugins = load_plugins(root)
    output = generated_header(
        "Plugin and adapter reference",
        [
            "plugins/catalog.toml",
            "package-root minco-plugin.json distribution manifests",
            "ADR 0027 authority split",
        ],
    )
    output += (
        "This is pre-link distribution metadata. Enabling remains an explicit Cargo "
        "feature plus typed constructor registration. Secret values and provider "
        "credentials have no field in this reference.\n\n"
        "| ID | Crate | Kind | Facade feature | Default | Stability | Description | Runtimes | Databases | Idle cost | Wake sources | Metadata digests |\n"
        "|---|---|---|---|:---:|---|---|---|---|---|---|---|\n"
    )
    for plugin in plugins:
        distribution = plugin["distribution"]
        resources = distribution.get("resources", [])
        idle = sorted({str(item.get("idle_cost", "unspecified")) for item in resources})
        wakes = sorted(
            {
                str(wake)
                for item in resources
                for wake in item.get("wake_sources", [])
            }
        )
        output += (
            f"| `{plugin['id']}` | `{plugin['crate']}` | `{plugin['kind']}` | "
            f"`{plugin['feature']}` | "
            f"{'yes' if plugin['default_enabled'] else 'no'} | `{plugin['stability']}` | "
            f"{markdown_cell(plugin['description'])} | "
            f"{markdown_cell(distribution.get('runtimes', []))} | "
            f"{markdown_cell(distribution.get('databases', []))} | "
            f"{markdown_cell(idle)} | {markdown_cell(wakes)} | "
            f"`{canonical_digest({key: value for key, value in plugin.items() if key != 'distribution'})[:12]}` / "
            f"`{canonical_digest(distribution)[:12]}` |\n"
        )

    output += "\n## Catalog fields\n\n| Field | Observed type | Present on every entry |\n|---|---|:---:|\n"
    for field, kind, required in field_inventory(plugins):
        if field == "distribution":
            continue
        output += f"| `{field}` | `{kind}` | {'yes' if required else 'no'} |\n"

    manifests = [plugin["distribution"] for plugin in plugins]
    output += (
        "\n## Distribution fields\n\n"
        "Unknown fields and unknown schema versions fail validation. Fields may be "
        "optional for a component with no behavior in that dimension.\n\n"
        "| Field | Observed type | Present on every manifest |\n|---|---|:---:|\n"
    )
    for field, kind, required in field_inventory(manifests):
        output += f"| `{field}` | `{kind}` | {'yes' if required else 'no'} |\n"
    return output


def cli_binary(root: Path) -> Path:
    for base in (root, SOURCE_ROOT):
        candidate = base / "target/debug/cargo-minco"
        if candidate.is_symlink():
            raise ValueError("cargo-minco binary cannot be a symlink")
        if not candidate.exists():
            continue
        resolved = candidate.resolve(strict=True)
        if not resolved.is_relative_to(base.resolve()):
            raise ValueError("cargo-minco binary must remain inside its repository")
        if resolved.is_file() and os.access(resolved, os.X_OK):
            return resolved
    raise RuntimeError(
        "cargo-minco binary is unavailable; run scripts/docs/generate-reference.sh"
    )


def run_cli(root: Path, arguments: list[str]) -> str:
    environment = os.environ.copy()
    environment["NO_COLOR"] = "1"
    environment["RUST_BACKTRACE"] = "0"
    completed = subprocess.run(
        [str(cli_binary(root)), "--root", str(root), *arguments],
        cwd=root,
        env=environment,
        check=False,
        capture_output=True,
        text=True,
        timeout=30,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            f"cargo-minco {' '.join(arguments)} failed: {completed.stderr.strip()}"
        )
    return "\n".join(line.rstrip() for line in completed.stdout.rstrip().splitlines()) + "\n"


def help_subcommands(help_text: str) -> list[str]:
    commands: list[str] = []
    in_commands = False
    for line in help_text.splitlines():
        if line == "Commands:":
            in_commands = True
            continue
        if not in_commands:
            continue
        if not line.strip():
            break
        if line.startswith("  ") and not line.startswith("   "):
            command = line.strip().split(maxsplit=1)[0]
            if re.fullmatch(r"[a-z][a-z0-9-]*", command) and command != "help":
                commands.append(command)
    return commands


def collect_cli_help(root: Path) -> dict[tuple[str, ...], str]:
    collected: dict[tuple[str, ...], str] = {}
    pending: list[tuple[str, ...]] = [()]
    while pending:
        path = pending.pop(0)
        if path in collected:
            continue
        help_text = run_cli(root, [*path, "--help"])
        collected[path] = help_text
        if len(path) < 4:
            pending.extend((*path, child) for child in help_subcommands(help_text))
    return collected


def render_cli(root: Path) -> str:
    help_pages = collect_cli_help(root)
    output = generated_header(
        "CLI reference",
        [
            "cargo-minco Clap command model",
            "cargo-minco generated --help output",
        ],
    )
    output += (
        "The executable is `cargo-minco`; Cargo exposes it as `cargo minco`. Hidden "
        "implementation commands are excluded by Clap. Mutation authority still comes "
        "from the relevant command's guards and documentation.\n\n"
        "## Command tree\n\n"
    )
    for path in sorted(help_pages, key=lambda item: (len(item), item)):
        if not path:
            continue
        indent = "  " * (len(path) - 1)
        output += f"{indent}- `cargo minco {' '.join(path)}`\n"
    output += "\n## Generated help\n"
    for path in sorted(help_pages, key=lambda item: (len(item), item)):
        label = "cargo minco" if not path else f"cargo minco {' '.join(path)}"
        depth = min(3 + len(path), 6)
        output += f"\n{'#' * depth} `{label}`\n\n```text\n{help_pages[path]}```\n"
    return output


def deployment_plan_fields(root: Path) -> list[tuple[str, str]]:
    source = repository_file(
        root, Path("crates/minco-plan/src/model.rs"), "Plan model"
    ).read_text()
    marker = "pub struct DeploymentPlan {"
    if marker not in source:
        raise ValueError("DeploymentPlan declaration is missing")
    body = source.split(marker, maxsplit=1)[1].split("\n}", maxsplit=1)[0]
    fields = []
    for line in body.splitlines():
        match = re.match(r"\s*pub ([a-z_][a-z0-9_]*): (.+),$", line)
        if match:
            fields.append((match.group(1), match.group(2)))
    if not fields:
        raise ValueError("DeploymentPlan has no public fields")
    return fields


def json_paths(value: Any, prefix: str = "") -> dict[str, str]:
    paths: dict[str, str] = {}
    if isinstance(value, dict):
        for key in sorted(value):
            path = f"{prefix}.{key}" if prefix else key
            paths[path] = observed_type(value[key])
            paths.update(json_paths(value[key], path))
    elif isinstance(value, list) and value:
        path = f"{prefix}[]"
        paths[path] = observed_type(value[0])
        paths.update(json_paths(value[0], path))
    return paths


def render_schemas(root: Path) -> str:
    config = json.loads(run_cli(root, ["config", "schema", "--json"]))
    plan = json.loads(run_cli(root, ["deploy", "plan", "--stdout", "--json"]))
    plan_source = repository_file(
        root, Path("crates/minco-plan/src/model.rs"), "Plan model"
    ).read_text()
    output = generated_header(
        "Configuration and Plan schema reference",
        [
            "cargo minco config schema --json",
            "crates/minco-plan/src/model.rs DeploymentPlan",
            "cargo minco deploy plan --stdout --json reference output",
        ],
    )
    output += f"Plan model source SHA-256: `{hashlib.sha256(plan_source.encode()).hexdigest()}`.\n\n"
    output += (
        "## Composed configuration schema\n\n"
        f"Schema version: `{config['schema']}`. Secret fields expose names, kinds, and "
        "descriptions only; defaults are never rendered for secret fields.\n\n"
        "| Path | Kind | Required | Secret | Default | Description |\n"
        "|---|---|:---:|:---:|---|---|\n"
    )
    for path, field in sorted(config["fields"].items()):
        default = None if field.get("secret") else field.get("default")
        output += (
            f"| `{path}` | `{field['kind']}` | {'yes' if field['required'] else 'no'} | "
            f"{'yes' if field['secret'] else 'no'} | {markdown_cell(default)} | "
            f"{markdown_cell(field['description'])} |\n"
        )

    output += (
        "\n## DeploymentPlan top-level schema\n\n"
        "Rust types are shown exactly as declared. Serde attributes may omit empty or "
        "optional fields from a particular serialized plan.\n\n"
        "| Field | Rust type | Present in reference plan |\n|---|---|:---:|\n"
    )
    for field, rust_type in deployment_plan_fields(root):
        output += (
            f"| `{field}` | `{rust_type}` | {'yes' if field in plan else 'no'} |\n"
        )

    output += (
        "\n## Reference serialized Plan paths\n\n"
        f"Reference schema version: `{plan['schema_version']}`. This inventory records "
        "the checked-in reference application's selected profile; omitted optional schema "
        "2 topology remains visible in the Rust type table above.\n\n"
        "| JSON path | Observed type |\n|---|---|\n"
    )
    for path, kind in sorted(json_paths(plan).items()):
        output += f"| `{path}` | `{kind}` |\n"
    return output


DIAGNOSTIC_PATTERN = re.compile(
    r'"('
    r'(?:MINCO|STATIC|PUBLISH|DEEP|CONFORMANCE)-[A-Z0-9](?:[A-Z0-9.-]*[A-Z0-9])?'
    r'|config\.[a-z0-9_.-]+'
    r'|(?:operation|schema|request|resource|conformance|documentation|package|plugin)\.[a-z0-9_.-]+'
    r')"'
)


def diagnostic_codes(root: Path) -> dict[str, list[str]]:
    locations: dict[str, set[str]] = {}
    candidates = [root / "crates", root / "plugins", root / "extensions", root / "scripts"]
    for base in candidates:
        for path in sorted(base.rglob("*")):
            if path.suffix not in {".rs", ".py"} or not path.is_file():
                continue
            relative = path.relative_to(root).as_posix()
            for number, line in enumerate(path.read_text(errors="replace").splitlines(), start=1):
                for code in DIAGNOSTIC_PATTERN.findall(line):
                    locations.setdefault(code, set()).add(f"{relative}:{number}")
    return {code: sorted(paths) for code, paths in sorted(locations.items())}


def diagnostic_family(code: str) -> str:
    if code.startswith("config."):
        return "configuration"
    if code.startswith("STATIC-"):
        return "repository truth"
    if code.startswith("PUBLISH-"):
        return "publication"
    if code.startswith("DEEP-"):
        return "deep review"
    if code.startswith("CONFORMANCE-") or code.startswith("conformance."):
        return "plugin conformance"
    if code.startswith("MINCO-"):
        return code.split("-", maxsplit=2)[1].lower()
    return code.split(".", maxsplit=1)[0]


def render_diagnostics(root: Path) -> str:
    codes = diagnostic_codes(root)
    output = generated_header(
        "Diagnostic code reference",
        [
            "diagnostic string literals in crates/**/*.rs",
            "diagnostic string literals in plugins/**/*.rs and extensions/**/*.rs",
            "repository validation diagnostics in scripts/**/*.py",
        ],
    )
    output += (
        "This inventory lists source-declared stable code identities, not every possible "
        "runtime message. Messages may gain context while codes remain the automation "
        "contract. A code's presence does not claim that every profile can emit it.\n\n"
        f"Declared codes: `{len(codes)}`.\n\n"
        "| Code | Family | First declaration | Additional references |\n"
        "|---|---|---|---:|\n"
    )
    for code, locations in codes.items():
        output += (
            f"| `{code}` | {diagnostic_family(code)} | `{locations[0]}` | "
            f"{len(locations) - 1} |\n"
        )
    return output


def render_index() -> str:
    output = generated_header(
        "Generated reference",
        ["the authority list on each generated page"],
    )
    output += (
        "These pages replace hand-maintained exhaustive inventories. Human-authored "
        "guides explain decisions and workflows; generated pages describe the exact "
        "current checkout.\n\n"
        "- [Package publication order and docs.rs links](packages.md)\n"
        "- [Facade feature graph](features.md)\n"
        "- [Plugin and adapter distribution metadata](plugins.md)\n"
        "- [CLI command tree and generated help](cli.md)\n"
        "- [Configuration and Plan schemas](schemas.md)\n"
        "- [Stable diagnostic codes](diagnostics.md)\n"
    )
    return output


def render_outputs(root: Path) -> dict[str, str]:
    root = root.resolve()
    workspace, facade = workspace_metadata(root)
    return {
        (OUTPUT_DIRECTORY / "index.md").as_posix(): render_index(),
        (OUTPUT_DIRECTORY / "packages.md").as_posix(): render_packages(root, workspace),
        (OUTPUT_DIRECTORY / "features.md").as_posix(): render_features(facade),
        (OUTPUT_DIRECTORY / "plugins.md").as_posix(): render_plugins(root),
        (OUTPUT_DIRECTORY / "cli.md").as_posix(): render_cli(root),
        (OUTPUT_DIRECTORY / "schemas.md").as_posix(): render_schemas(root),
        (OUTPUT_DIRECTORY / "diagnostics.md").as_posix(): render_diagnostics(root),
    }


def stale_outputs(root: Path) -> list[str]:
    expected = render_outputs(root)
    output_root = root / OUTPUT_DIRECTORY
    if output_root.is_symlink() or (
        output_root.exists() and not output_root.resolve().is_relative_to(root.resolve())
    ):
        raise ValueError("generated reference output must remain inside the repository")
    stale = []
    for relative, content in expected.items():
        path = root / relative
        if path.is_symlink():
            raise ValueError(f"generated reference cannot be a symlink: {relative}")
        if not path.is_file() or path.read_text() != content:
            stale.append(relative)
    if output_root.is_dir():
        extras = {
            path.relative_to(root).as_posix()
            for path in output_root.glob("*.md")
            if path.relative_to(root).as_posix() not in expected
        }
        stale.extend(sorted(extras))
    return sorted(stale)


def write_outputs(root: Path) -> None:
    outputs = render_outputs(root)
    output_root = root / OUTPUT_DIRECTORY
    if output_root.is_symlink() or (output_root.exists() and not output_root.is_dir()):
        raise ValueError("generated reference output must be a repository directory")
    output_root.mkdir(parents=True, exist_ok=True)
    if not output_root.resolve().is_relative_to(root.resolve()):
        raise ValueError("generated reference output must remain inside the repository")
    for relative, content in outputs.items():
        path = root / relative
        if path.is_symlink():
            raise ValueError(f"generated reference cannot be a symlink: {relative}")
        path.write_text(content)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail when generated pages are stale")
    parser.add_argument("--root", type=Path, default=SOURCE_ROOT, help="repository root")
    args = parser.parse_args(argv)
    root = args.root.resolve()
    if args.check:
        stale = stale_outputs(root)
        if stale:
            for relative in stale:
                print(f"stale generated reference: {relative}", file=sys.stderr)
            return 1
        print(f"generated reference is current ({len(render_outputs(root))} files)")
        return 0
    write_outputs(root)
    print(f"generated {len(render_outputs(root))} reference files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
