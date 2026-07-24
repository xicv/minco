#!/usr/bin/env python3
"""Derive the local Compose topology from Minco's selected deployment graph."""
from __future__ import annotations

import argparse
import json
import os
import tomllib
from pathlib import Path


def load_toml(path: Path) -> dict:
    with path.open("rb") as source:
        return tomllib.load(source)


def local_topology(root: Path) -> dict:
    manifest = load_toml(root / "minco.toml")
    deployment = load_toml(root / manifest["deployment_config"])
    catalog = load_toml(root / manifest["plugin_catalog"])
    plugin_selection = manifest.get("plugins", {})
    enabled = set(plugin_selection.get("enabled", []))
    disabled = set(plugin_selection.get("disabled", []))
    selected_plugins = {
        entry["id"]
        for entry in catalog.get("plugin", [])
        if entry.get("default_enabled", False)
    }
    selected_plugins.update(enabled)
    selected_plugins.difference_update(disabled)

    aws_services: set[str] = set()
    if deployment["runtime"] == "lambda_zip_arm64":
        aws_services.update(("ssm", "sts"))

    database_kind = deployment["database"]["kind"]
    compose_services = []
    environment = {}
    if database_kind in {
        "aurora_serverless_v2",
        "neon_postgres",
        "rds_postgres",
        "self_hosted_postgres",
    }:
        compose_services.append("postgres")
        local_database_url = os.environ.get(
            "MINCO_LOCAL_DATABASE_URL",
            "postgres://minco:minco@127.0.0.1:55432/minco_orders",
        )
        if not local_database_url:
            raise ValueError("MINCO_LOCAL_DATABASE_URL must not be empty")
        environment.update(
            {
                "DATABASE_KIND": "postgres",
                "DATABASE_URL": local_database_url,
                "MIGRATION_DATABASE_URL": local_database_url,
            }
        )
    if aws_services:
        compose_services.append("rustack")
        rustack_port = int(os.environ.get("MINCO_RUSTACK_PORT", "4566"))
        if not 1 <= rustack_port <= 65535:
            raise ValueError("MINCO_RUSTACK_PORT must be between 1 and 65535")
        environment.update(
            {
                "AWS_ACCESS_KEY_ID": "test",
                "AWS_DEFAULT_REGION": deployment["region"],
                "AWS_EC2_METADATA_DISABLED": "true",
                "AWS_ENDPOINT_URL": f"http://127.0.0.1:{rustack_port}",
                "AWS_SECRET_ACCESS_KEY": "test",
            }
        )

    return {
        "schema_version": 1,
        "aws_services": sorted(aws_services),
        "compose_services": compose_services,
        "environment": environment,
        "region": deployment["region"],
        "selected_plugins": sorted(selected_plugins),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--format", choices=("json", "env"), default="json")
    args = parser.parse_args()
    topology = local_topology(args.root.resolve())
    if args.format == "env":
        for key, value in sorted(topology["environment"].items()):
            print(f"{key}={value}")
    else:
        print(json.dumps(topology, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
