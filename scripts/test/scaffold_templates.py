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
GENERATOR_TEMPLATE_ROOT = ROOT / "crates/minco-cli/templates/generator"
MINCO_VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
COMMON = [
    "Cargo.toml",
    "README.md",
    "AGENTS.md",
    ".gitignore",
    ".env.example",
    "rust-toolchain.toml",
    "minco.toml",
    "config/environments/default.toml",
    "config/environments/dev.toml",
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
GENERATOR_STUBS = {
    "adapter-test.rs.tmpl",
    "adapter.rs.tmpl",
    "adapter.md.tmpl",
    "application-test.rs.tmpl",
    "http-test.rs.tmpl",
    "migration.sql.tmpl",
    "migration.md.tmpl",
    "module-application.rs.tmpl",
    "module-domain.rs.tmpl",
    "module-test.rs.tmpl",
    "module.md.tmpl",
    "operation.md.tmpl",
    "plugin-lib.rs.tmpl",
    "plugin-readme.md.tmpl",
    "seeder-verify.sql.tmpl",
    "seeder.sql.tmpl",
    "seeder.md.tmpl",
    "worker-test.rs.tmpl",
    "worker.rs.tmpl",
    "worker.md.tmpl",
}


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
        "{{MINCO_VERSION}}": MINCO_VERSION,
        "{{DB_FEATURE}}": "sqlx-postgres" if database == "postgres" else "sqlx-sqlite",
        "{{DATABASE}}": database,
        "{{DATABASE_SETUP}}": (
            "mkdir -p target/minco"
            if database == "postgres"
            else "mkdir -p target/minco var"
        ),
        "{{MIGRATION_DIR}}": f"migrations/{database}",
        "{{DATABASE_ENV}}": (
            "DATABASE_URL=postgresql://postgres:postgres@127.0.0.1:5432/app\n"
            "DATABASE_MIGRATION_URL=postgresql://postgres:postgres@127.0.0.1:5432/app"
            if database == "postgres"
            else "DATABASE_PATH=var/app.db\nDATABASE_MIGRATION_URL=sqlite://var/app.db"
        ),
        "{{DATABASE_SECRET_REFERENCE}}": (
            "env:DATABASE_URL" if database == "postgres" else "env:DATABASE_PATH"
        ),
        "{{PACKAGE_COMMAND}}": (
            "cargo lambda build --release --arm64 --output-format zip "
            "-p sample-api-service --bin sample-api-lambda --features lambda"
            if database == "postgres"
            else "cargo build -p sample-api-service --bin sample-api-local"
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
    lifecycle = template("migrations/.minco-migrations.toml")
    target = destination / f"migrations/{database}/.minco-migrations.toml"
    target.write_text(render(lifecycle, values))


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
    assert workspace["workspace"]["dependencies"]["minco"]["version"] == MINCO_VERSION
    assert (
        "sqlx-postgres" if database == "postgres" else "sqlx-sqlite"
    ) in workspace["workspace"]["dependencies"]["minco"]["features"]

    manifest = tomllib.loads((root / "minco.toml").read_text())
    assert manifest["contract"] == "openapi/openapi.yaml"
    assert manifest["generated"] == "crates/api/src/generated.rs"
    assert manifest["migrations"]["roots"] == [f"migrations/{database}"]
    assert manifest["configuration"]["root"] == "config/environments"
    assert {
        field["key"] for field in manifest["configuration"]["fields"]
    } == {"application.name", "runtime.log_level", "database.connection"}
    assert set(manifest["operations"]) == {"healthLive", "getPlatform"}

    instructions = (root / "AGENTS.md").read_text()
    for required in [
        "$minco-web-application",
        "cargo minco agent plan --target all --json",
        "cargo minco agent context",
        "generated Minco application",
    ]:
        assert required in instructions
    for forbidden in [
        "one JJ workspace per task",
        "task-start.sh",
        "task-finish.sh",
        "$minco-framework-task",
        "cargo minco release",
    ]:
        assert forbidden not in instructions
    assert template("CLAUDE.md") == "# Claude project instructions\n\n@AGENTS.md\n"
    assert not (root / "CLAUDE.md").exists()

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
    assert "HttpConfigurationError" in source
    assert "InvalidHeaderValue" not in source
    assert (root / f"migrations/{database}/0001_foundation.sql").is_file()
    lifecycle = tomllib.loads(
        (root / f"migrations/{database}/.minco-migrations.toml").read_text()
    )
    assert lifecycle["id"] == f"sample-api-{database}"
    assert lifecycle["owner"] == "application:sample-api"
    assert lifecycle["backend"] == database
    assert lifecycle["history_table"] == "_sample_api_migrations"
    assert lifecycle["verify_tables"] == ["minco_schema_metadata"]
    assert lifecycle["migration"] == [
        {"version": 1, "risk": "additive", "reversible": False}
    ]
    assert "database_migrate" not in manifest["commands"]
    if database == "sqlite":
        assert "mkdir -p target/minco var" in (root / "README.md").read_text()
    return {
        "database": database,
        "toml_files": len(toml_files),
        "yaml_files": len(yaml_files),
        "rust_files": len(list(root.rglob("*.rs"))),
        "workspace_members": len(expected_members),
        "operations": sorted(operation_ids),
    }


def check_generator_stubs() -> dict[str, object]:
    paths = sorted(GENERATOR_TEMPLATE_ROOT.glob("*.tmpl"))
    assert {path.name for path in paths} == GENERATOR_STUBS
    values = {
        "{{NAME}}": "sample-widgets",
        "{{SNAKE_NAME}}": "sample_widgets",
        "{{PASCAL_NAME}}": "SampleWidgets",
        "{{OPERATION_ID}}": "getPlatform",
        "{{METHOD}}": "GET",
        "{{PATH}}": "/platform",
        "{{RUST_PATH_LITERAL}}": '"/platform"',
        "{{VERSION}}": "0002",
        "{{LAYER}}": "application",
    }
    rendered = {path.name: render(path.read_text(), values) for path in paths}
    assert all(source.endswith("\n") for source in rendered.values())
    assert "SELECT FALSE;" in rendered["seeder-verify.sql.tmpl"]
    assert "panic!(\"TODO(getPlatform)" in rendered["application-test.rs.tmpl"]
    assert "panic!(\"TODO(getPlatform)" in rendered["http-test.rs.tmpl"]
    assert "PluginId::new(\"sample-widgets\")" in rendered["plugin-lib.rs.tmpl"]
    return {"count": len(rendered), "files": sorted(rendered)}


def main() -> int:
    reports = []
    with tempfile.TemporaryDirectory(prefix="minco-scaffold-") as temporary:
        parent = Path(temporary)
        for database in ["postgres", "sqlite"]:
            root = parent / database
            root.mkdir()
            render_profile(root, database)
            reports.append(check_profile(root, database))
    print(
        json.dumps(
            {
                "status": "ok",
                "profiles": reports,
                "generator_stubs": check_generator_stubs(),
            },
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
