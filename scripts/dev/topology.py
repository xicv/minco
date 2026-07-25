#!/usr/bin/env python3
"""Derive the local Compose topology from Minco's selected deployment graph."""
from __future__ import annotations

import argparse
import json
import os
import subprocess
from pathlib import Path

SUPPORTED_RUSTACK_SERVICES = {"dynamodb", "s3", "sqs", "ssm", "sts"}


def load_plan(root: Path, plan_path: Path | None) -> dict:
    if plan_path is not None:
        with plan_path.open(encoding="utf-8") as source:
            return json.load(source)
    result = subprocess.run(
        [
            "cargo",
            "minco",
            "--root",
            str(root),
            "--json",
            "deploy",
            "plan",
            "--stdout",
        ],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def local_topology(root: Path, plan_path: Path | None = None) -> dict:
    deployment = load_plan(root, plan_path)
    selected_plugins = {
        plugin["id"] for plugin in deployment["application_graph"]["plugins"]
    }
    aws_services = set(deployment["local_aws_services"])
    unsupported_services = aws_services - SUPPORTED_RUSTACK_SERVICES
    if unsupported_services:
        unsupported = ", ".join(sorted(unsupported_services))
        raise ValueError(f"unsupported Rustack service in deployment plan: {unsupported}")

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
    parser.add_argument(
        "--plan",
        type=Path,
        default=Path(os.environ["MINCO_DEPLOYMENT_PLAN"])
        if os.environ.get("MINCO_DEPLOYMENT_PLAN")
        else None,
    )
    parser.add_argument("--format", choices=("json", "env"), default="json")
    args = parser.parse_args()
    topology = local_topology(
        args.root.resolve(),
        args.plan.resolve() if args.plan is not None else None,
    )
    if args.format == "env":
        for key, value in sorted(topology["environment"].items()):
            print(f"{key}={value}")
    else:
        print(json.dumps(topology, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
