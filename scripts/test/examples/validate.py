#!/usr/bin/env python3
"""Validate the checked-in Minco example and recipe authority."""
from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[3]
MATRIX = "examples/recipes.toml"
RECIPE_FIELDS = {
    "id",
    "title",
    "kind",
    "example",
    "documentation",
    "features",
    "runtime",
    "database",
    "provider_assumptions",
    "cost_classes",
    "wake_sources",
    "checks",
    "unsupported_gates",
}
RECIPE_ID = re.compile(r"[a-z][a-z0-9-]*")
REQUIRED_DOCUMENTATION_SECTIONS = (
    "## Features",
    "## Provider assumptions",
    "## Cost and wake behavior",
    "## Verification",
    "## Unsupported gates",
)
ALLOWED_COST_CLASSES = {
    "fixed_monthly",
    "request_only",
    "scheduled_wakeup",
    "storage_only",
    "zero_compute",
}
ALLOWED_KINDS = {
    "application",
    "database",
    "deployment",
    "feedback",
    "http-api",
    "plugin",
    "scaffold",
    "static-site",
    "worker",
}
ALLOWED_RUNTIMES = {"lambda_zip_arm64", "local_native", "static_assets"}
ALLOWED_DATABASES = {"dynamodb", "memory", "none", "postgres", "sqlite"}
ALLOWED_WAKE_SOURCES = {
    "http_request",
    "manual",
    "object_event",
    "queue_message",
    "schedule",
}
CHECK_COMMANDS = {
    "minco-desk-example": (
        "cargo",
        "test",
        "--locked",
        "-p",
        "minco-desk-example",
    ),
    "orders-contract": (
        "cargo",
        "run",
        "--locked",
        "-p",
        "cargo-minco",
        "--",
        "--root",
        ".",
        "contract",
        "check",
        "--json",
    ),
    "orders-explain": (
        "cargo",
        "run",
        "--locked",
        "-p",
        "cargo-minco",
        "--",
        "--root",
        ".",
        "explain",
        "placeOrder",
        "--json",
    ),
    "orders-application": (
        "cargo",
        "test",
        "--locked",
        "-p",
        "orders-domain",
        "-p",
        "orders-application",
    ),
    "orders-resource-api": (
        "cargo",
        "test",
        "--locked",
        "-p",
        "orders-application",
        "-p",
        "orders-adapters",
        "-p",
        "orders-api",
        "--all-features",
    ),
    "local-sqlite-plan": (
        "cargo",
        "run",
        "--locked",
        "-p",
        "cargo-minco",
        "--",
        "--root",
        ".",
        "deploy",
        "plan",
        "--config",
        "examples/orders/config/minco.local-sqlite.toml",
        "--stdout",
        "--json",
    ),
    "orders-sqlite": (
        "cargo",
        "test",
        "--locked",
        "-p",
        "orders-adapters",
        "--no-default-features",
        "--features",
        "sqlite",
    ),
    "orders-postgres": (
        "cargo",
        "test",
        "--locked",
        "-p",
        "orders-adapters",
        "--no-default-features",
        "--features",
        "postgres",
    ),
    "sqlx-isolation": ("bash", "scripts/test/sqlx_feature_isolation.sh"),
    "cost-neon": (
        "cargo",
        "run",
        "--locked",
        "-p",
        "cargo-minco",
        "--",
        "--root",
        ".",
        "cost",
        "--config",
        "examples/orders/config/minco.neon-launch.toml",
        "--json",
    ),
    "cost-aurora": (
        "cargo",
        "run",
        "--locked",
        "-p",
        "cargo-minco",
        "--",
        "--root",
        ".",
        "cost",
        "--config",
        "examples/orders/config/minco.aurora-serverless-v2.toml",
        "--json",
    ),
    "cost-rds": (
        "cargo",
        "run",
        "--locked",
        "-p",
        "cargo-minco",
        "--",
        "--root",
        ".",
        "cost",
        "--config",
        "examples/orders/config/minco.rds-postgres.toml",
        "--json",
    ),
    "cost-dynamodb": (
        "cargo",
        "run",
        "--locked",
        "-p",
        "cargo-minco",
        "--",
        "--root",
        ".",
        "cost",
        "--config",
        "examples/orders/config/minco.dynamodb.toml",
        "--json",
    ),
    "worker-runtime": ("cargo", "test", "--locked", "-p", "minco-aws-worker"),
    "worker-plan": (
        "cargo",
        "test",
        "--locked",
        "-p",
        "minco-plan",
        "--test",
        "multi_runtime",
    ),
    "zero-idle-plan": (
        "cargo",
        "run",
        "--locked",
        "-p",
        "cargo-minco",
        "--",
        "--root",
        ".",
        "deploy",
        "plan",
        "--config",
        "examples/orders/config/minco.dev.toml",
        "--stdout",
        "--json",
    ),
    "static-site-contract": (
        "cargo",
        "test",
        "--locked",
        "-p",
        "cargo-minco",
        "static_site",
    ),
    "third-party-plugin": (
        "cargo",
        "test",
        "--locked",
        "--manifest-path",
        "examples/plugins/third-party-minimal/Cargo.toml",
        "--all-features",
    ),
    "generated-applications": ("./scripts/test/generated_apps.sh",),
    "feedback-review-loop": (
        "cargo",
        "test",
        "--locked",
        "-p",
        "minco-plugin-feedback",
        "--all-features",
    ),
}
REMOVED_PROVIDER_ENVIRONMENT = {
    "DATABASE_URL",
    "MINCO_ORDERS_TEST_POSTGRES_URL",
}
INERT_AWS_ENVIRONMENT = {
    "AWS_CONFIG_FILE": os.devnull,
    "AWS_SHARED_CREDENTIALS_FILE": os.devnull,
    "AWS_EC2_METADATA_DISABLED": "true",
    "AWS_ENDPOINT_URL": "http://127.0.0.1:9",
}


def repository_path(root: Path, value: Any, field: str) -> Path:
    if not isinstance(value, str) or not value:
        raise ValueError(f"{field} must be a non-empty repository-relative path")
    relative = Path(value)
    if relative.is_absolute() or ".." in relative.parts:
        raise ValueError(f"{field} must be a safe repository-relative path")
    candidate = root / relative
    if candidate.is_symlink():
        raise ValueError(f"{field} cannot be a symlink")
    resolved = candidate.resolve(strict=True)
    if not resolved.is_relative_to(root.resolve()):
        raise ValueError(f"{field} must remain inside the repository")
    return resolved


def string_list(recipe: dict[str, Any], field: str, *, allow_empty: bool = False) -> list[str]:
    value = recipe.get(field)
    if not isinstance(value, list) or (not value and not allow_empty):
        raise ValueError(f"{field} must be a{' possibly empty' if allow_empty else ' non-empty'} list")
    if any(not isinstance(item, str) or not item.strip() for item in value):
        raise ValueError(f"{field} entries must be non-empty strings")
    return value


def validate_bash_fences(identifier: str, source: str) -> None:
    fence_language: str | None = None
    fence_start = 0
    fence_lines: list[str] = []
    for line_number, line in enumerate(source.splitlines(), start=1):
        stripped = line.strip()
        if fence_language is None:
            if stripped in {"```bash", "```sh"}:
                fence_language = stripped[3:]
                fence_start = line_number
                fence_lines = []
            continue
        if stripped == "```":
            result = subprocess.run(
                ["bash", "-n"],
                input="\n".join(fence_lines) + "\n",
                capture_output=True,
                text=True,
                check=False,
            )
            if result.returncode != 0:
                detail = result.stderr.strip().splitlines()[-1]
                raise ValueError(
                    f"{identifier}: invalid bash fence at line {fence_start}: {detail}"
                )
            fence_language = None
            fence_lines = []
            continue
        fence_lines.append(line)
    if fence_language is not None:
        raise ValueError(
            f"{identifier}: invalid bash fence at line {fence_start}: unclosed fence"
        )


def validate(root: Path) -> list[dict[str, Any]]:
    root = root.resolve()
    matrix_path = repository_path(root, MATRIX, "recipe matrix")
    matrix = tomllib.loads(matrix_path.read_text())
    if set(matrix) != {"schema_version", "recipe"} or matrix["schema_version"] != 1:
        raise ValueError("recipe matrix must use only schema_version 1 and recipe entries")
    recipes = matrix["recipe"]
    if not isinstance(recipes, list) or not recipes:
        raise ValueError("recipe matrix must contain at least one recipe")

    identifiers: set[str] = set()
    for recipe in recipes:
        if not isinstance(recipe, dict) or set(recipe) != RECIPE_FIELDS:
            raise ValueError("every recipe must contain exactly the schema 1 fields")
        identifier = recipe["id"]
        if not isinstance(identifier, str) or RECIPE_ID.fullmatch(identifier) is None:
            raise ValueError("recipe id must be lowercase kebab-case")
        if identifier in identifiers:
            raise ValueError(f"duplicate recipe id: {identifier}")
        identifiers.add(identifier)
        for field in ("title", "kind", "runtime", "database"):
            if not isinstance(recipe[field], str) or not recipe[field].strip():
                raise ValueError(f"{identifier}: {field} must be a non-empty string")
        classifications = (
            ("kind", ALLOWED_KINDS),
            ("runtime", ALLOWED_RUNTIMES),
            ("database", ALLOWED_DATABASES),
        )
        for field, allowed in classifications:
            if recipe[field] not in allowed:
                raise ValueError(f"{identifier}: unknown {field}: {recipe[field]}")
        repository_path(root, recipe["example"], f"{identifier} example")
        documentation = repository_path(
            root, recipe["documentation"], f"{identifier} documentation"
        )
        if not documentation.is_file() or documentation.suffix != ".md":
            raise ValueError(f"{identifier}: documentation must be a Markdown file")
        documentation_source = documentation.read_text()
        for section in REQUIRED_DOCUMENTATION_SECTIONS:
            if section not in documentation_source:
                raise ValueError(f"{identifier}: missing required section: {section}")
        validate_bash_fences(identifier, documentation_source)
        for field in (
            "features",
            "provider_assumptions",
            "cost_classes",
            "unsupported_gates",
        ):
            string_list(recipe, field)
        for wake_source in string_list(recipe, "wake_sources", allow_empty=True):
            if wake_source not in ALLOWED_WAKE_SOURCES:
                raise ValueError(f"{identifier}: unknown wake source: {wake_source}")
        for check in string_list(recipe, "checks"):
            if check not in CHECK_COMMANDS:
                raise ValueError(f"{identifier}: unknown check id: {check}")
            if f"`{check}`" not in documentation_source:
                raise ValueError(
                    f"{identifier}: documentation does not name check: {check}"
                )
        for cost_class in recipe["cost_classes"]:
            if cost_class not in ALLOWED_COST_CLASSES:
                raise ValueError(f"{identifier}: unknown cost class: {cost_class}")
    return recipes


def run_checks(root: Path, recipes: list[dict[str, Any]], only: list[str]) -> None:
    declared = list(dict.fromkeys(check for recipe in recipes for check in recipe["checks"]))
    unknown = sorted(set(only) - set(declared))
    if unknown:
        raise ValueError(f"requested check is not declared by a recipe: {unknown[0]}")
    selected = only if only else declared
    environment = os.environ.copy()
    for key in list(environment):
        if key.startswith("AWS_"):
            environment.pop(key)
    for key in REMOVED_PROVIDER_ENVIRONMENT:
        environment.pop(key, None)
    environment.update(INERT_AWS_ENVIRONMENT)
    environment["CARGO_TARGET_DIR"] = str(root / "target")
    for check in selected:
        command = CHECK_COMMANDS[check]
        completed = subprocess.run(
            command,
            cwd=root,
            env=environment,
            check=False,
        )
        if completed.returncode != 0:
            raise ValueError(f"recipe check failed: {check}")
        print(f"recipe check passed: {check}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="validate without running recipes")
    mode.add_argument("--run", action="store_true", help="run declared bounded recipe checks")
    parser.add_argument("--only", action="append", default=[], help="run one declared check id")
    parser.add_argument("--root", type=Path, default=ROOT, help="repository root")
    args = parser.parse_args(argv)
    try:
        recipes = validate(args.root)
        if args.only and not args.run:
            raise ValueError("--only requires --run")
        if args.run:
            run_checks(args.root.resolve(), recipes, args.only)
    except (OSError, ValueError, tomllib.TOMLDecodeError) as error:
        print(f"recipe validation failed: {error}", file=sys.stderr)
        return 1
    print(f"recipe matrix passed: {len(recipes)} recipes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
