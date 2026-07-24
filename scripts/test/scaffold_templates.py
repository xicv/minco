#!/usr/bin/env python3
"""Render and validate cargo-minco application templates without compiling Rust."""
from __future__ import annotations

import json
import re
import tempfile
import tomllib
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
TEMPLATE_ROOT = ROOT / "crates/minco-cli/templates/app"
COMMON = [
    "Cargo.toml",
    "README.md",
    "AGENTS.md",
    ".gitignore",
    ".env.example",
    "rust-toolchain.toml",
    "minco.toml",
    "quality.toml",
    "plugins/catalog.toml",
    "roadmap/roadmap.yaml",
    "tasks/M0/M0-T01-foundation.md",
    "openapi/openapi.yaml",
    "crates/domain/Cargo.toml",
    "crates/domain/src/lib.rs",
    "crates/application/Cargo.toml",
    "crates/application/src/lib.rs",
    "crates/adapters/Cargo.toml",
    "crates/api/Cargo.toml",
    "crates/api/src/lib.rs",
    "services/app/Cargo.toml",
    "services/app/src/lib.rs",
    "services/app/src/main.rs",
    "services/app/src/bin/lambda.rs",
    "services/app/src/bin/migrate.rs",
]


def template(path: str) -> str:
    return (TEMPLATE_ROOT / f"{path}.tmpl").read_text()


def render(source: str, values: dict[str, str]) -> str:
    for key, value in values.items():
        source = source.replace(key, value)
    unresolved = sorted(set(re.findall(r"\{\{[A-Z_]+\}\}", source)))
    if unresolved:
        raise AssertionError(f"unresolved template values: {unresolved}")
    return source


def render_profile(destination: Path, database: str) -> None:
    values = {
        "{{PACKAGE}}": "sample-api",
        "{{CRATE}}": "sample_api",
        "{{TITLE}}": "Sample Api",
        "{{MINCO_VERSION}}": "0.1.0",
        "{{DB_FEATURE}}": "sqlx-postgres" if database == "postgres" else "sqlx-sqlite",
        "{{MIGRATION_DIR}}": f"migrations/{database}",
        "{{DATABASE_ENV}}": (
            "DATABASE_URL=postgresql://postgres:postgres@127.0.0.1:5432/app\n"
            "DATABASE_MIGRATION_URL=postgresql://postgres:postgres@127.0.0.1:5432/app"
            if database == "postgres"
            else "DATABASE_PATH=var/app.db"
        ),
    }
    for relative in COMMON:
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(render(template(relative), values))

    adapter = template(f"crates/adapters/src/lib-{database}.rs")
    target = destination / "crates/adapters/src/lib.rs"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(render(adapter, values))
    environment = template(f"environments/dev-{database}.toml")
    target = destination / "environments/dev.toml"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(render(environment, values))
    migration = template(f"migrations/{database}/0001_foundation.sql")
    target = destination / f"migrations/{database}/0001_foundation.sql"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(render(migration, values))


def check_profile(root: Path, database: str) -> dict[str, object]:
    toml_files = sorted(root.rglob("*.toml"))
    yaml_files = sorted([*root.rglob("*.yaml"), *root.rglob("*.yml")])
    for path in toml_files:
        tomllib.loads(path.read_text())
    for path in yaml_files:
        yaml.safe_load(path.read_text())

    workspace = tomllib.loads((root / "Cargo.toml").read_text())
    expected_members = {
        "crates/domain",
        "crates/application",
        "crates/adapters",
        "crates/api",
        "services/app",
    }
    assert set(workspace["workspace"]["members"]) == expected_members
    assert workspace["workspace"]["dependencies"]["minco"]["version"] == "0.1.0"
    assert (
        "sqlx-postgres" if database == "postgres" else "sqlx-sqlite"
    ) in workspace["workspace"]["dependencies"]["minco"]["features"]

    manifest = tomllib.loads((root / "minco.toml").read_text())
    assert manifest["contract"] == "openapi/openapi.yaml"
    assert manifest["generated"] == "crates/api/src/generated.rs"
    assert manifest["migrations"]["roots"] == [f"migrations/{database}"]
    assert set(manifest["operations"]) == {"healthLive", "getPlatform"}

    contract = yaml.safe_load((root / "openapi/openapi.yaml").read_text())
    assert str(contract["openapi"]).startswith("3.1.")
    operation_ids = {
        operation["operationId"]
        for item in contract["paths"].values()
        for method, operation in item.items()
        if method in {"get", "post", "put", "patch", "delete", "head", "options"}
    }
    assert operation_ids == {"healthLive", "getPlatform"}
    for schema in contract["components"]["schemas"].values():
        if schema.get("type") == "object":
            assert schema.get("additionalProperties") is False

    source = "\n".join(path.read_text() for path in root.rglob("*.rs"))
    assert "{{" not in source
    assert "examples/orders" not in source
    assert "sample_api_application::GetPlatformInfo" in source
    assert (root / f"migrations/{database}/0001_foundation.sql").is_file()
    return {
        "database": database,
        "toml_files": len(toml_files),
        "yaml_files": len(yaml_files),
        "rust_files": len(list(root.rglob("*.rs"))),
        "workspace_members": len(expected_members),
        "operations": sorted(operation_ids),
    }


def main() -> int:
    reports = []
    with tempfile.TemporaryDirectory(prefix="minco-scaffold-") as temporary:
        parent = Path(temporary)
        for database in ["postgres", "sqlite"]:
            root = parent / database
            root.mkdir()
            render_profile(root, database)
            reports.append(check_profile(root, database))
    print(json.dumps({"status": "ok", "profiles": reports}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
