#!/usr/bin/env python3
"""Resolve local Compose and AWS services from the selected Minco plugins."""

from __future__ import annotations

import argparse
import tomllib
from pathlib import Path

PLUGIN_REQUIREMENTS = {
    "aws-lambda": {"compose": {"rustack"}, "aws": {"ssm"}},
    "sqlx-postgres": {"compose": {"postgres"}, "aws": set()},
}


def resolve(manifest_path: Path) -> tuple[list[str], list[str]]:
    with manifest_path.open("rb") as handle:
        manifest = tomllib.load(handle)
    if manifest.get("schema") != 1:
        raise ValueError(f"{manifest_path} must use schema 1")

    plugins = manifest.get("plugins", {})
    enabled = set(plugins.get("enabled", []))
    disabled = set(plugins.get("disabled", []))
    overlap = enabled & disabled
    if overlap:
        names = ", ".join(sorted(overlap))
        raise ValueError(f"plugins cannot be both enabled and disabled: {names}")

    compose_services: set[str] = set()
    aws_services: set[str] = set()
    for plugin in enabled:
        requirement = PLUGIN_REQUIREMENTS.get(plugin)
        if requirement is None:
            continue
        compose_services.update(requirement["compose"])
        aws_services.update(requirement["aws"])
    return sorted(compose_services), sorted(aws_services)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--field", choices=("compose", "aws"), required=True)
    args = parser.parse_args()

    compose_services, aws_services = resolve(args.manifest)
    selected = compose_services if args.field == "compose" else aws_services
    print(" ".join(selected))


if __name__ == "__main__":
    main()
