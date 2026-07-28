#!/usr/bin/env python3
"""Behavioral tests for graph-derived local infrastructure selection."""
from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def write_plan(
    directory: Path,
    *,
    plugins: list[str] | None = None,
    aws_services: list[str] | None = None,
) -> Path:
    path = directory / "plan.json"
    path.write_text(
        json.dumps(
            {
                "application_graph": {
                    "plugins": [
                        {"id": plugin}
                        for plugin in (
                            plugins
                            if plugins is not None
                            else ["health", "idempotency", "observability"]
                        )
                    ]
                },
                "database": {"kind": "neon_postgres"},
                "local_aws_services": aws_services
                if aws_services is not None
                else ["ssm", "sts"],
                "region": "ap-southeast-2",
                "runtime": "lambda_zip_arm64",
            }
        )
    )
    return path


def topology(
    *arguments: str,
    root: Path = ROOT,
    environment: dict[str, str] | None = None,
) -> dict:
    result = subprocess.run(
        [sys.executable, str(ROOT / "scripts/dev/topology.py"), "--root", str(root), *arguments],
        cwd=ROOT,
        env=environment,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def test_reference_graph_selects_only_declared_local_dependencies() -> None:
    result = topology()

    assert result["aws_services"] == ["ssm", "sts"]
    assert result["compose_services"] == ["postgres", "rustack"]


def test_minco_dev_dry_run_exposes_the_safe_graph_before_startup() -> None:
    result = subprocess.run(
        ["cargo", "minco", "dev", "--dry-run", "--json"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    plan = json.loads(result.stdout)

    assert plan["external_aws_contact"] is False
    assert [service["kind"] for service in plan["services"]] == [
        "postgres",
        "rustack",
    ]
    assert [step["kind"] for step in plan["lifecycle"]] == ["migrate"]
    assert [(process["id"], process["role"]) for process in plan["processes"]] == [
        ("api", "api"),
    ]
    serialized = json.dumps(plan)
    assert "DATABASE_URL" not in serialized
    assert "AWS_SECRET_ACCESS_KEY" not in serialized
    assert "postgres://" not in serialized


def test_minco_dev_sqlite_profile_and_port_override_remain_local_and_explicit() -> None:
    result = subprocess.run(
        [
            "cargo",
            "minco",
            "dev",
            "--profile",
            "sqlite",
            "--no-migrate",
            "--port",
            "31000",
            "--dry-run",
            "--json",
        ],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    plan = json.loads(result.stdout)

    assert plan["profile"] == "sqlite"
    assert [service["kind"] for service in plan["services"]] == ["sqlite"]
    assert plan["lifecycle"] == []
    assert plan["processes"][0]["command"]["environment"]["PORT"] == "31000"
    assert plan["processes"][0]["readiness"]["url"] == (
        "http://127.0.0.1:31000/health/ready"
    )


def test_provider_neutral_plugins_do_not_silently_select_aws_providers() -> None:
    with tempfile.TemporaryDirectory(prefix="minco-local-topology-") as temporary:
        root = Path(temporary)
        plan = write_plan(
            root,
            plugins=["events", "object-storage"],
            aws_services=["ssm", "sts"],
        )

        result = topology("--plan", str(plan), root=root)

    assert result["aws_services"] == ["ssm", "sts"]
    assert result["selected_plugins"] == ["events", "object-storage"]


def test_offline_plan_rejects_unsupported_rustack_services() -> None:
    with tempfile.TemporaryDirectory(prefix="minco-local-topology-") as temporary:
        root = Path(temporary)
        plan = write_plan(root, aws_services=["lambda", "ssm"])
        result = subprocess.run(
            [
                sys.executable,
                "scripts/dev/topology.py",
                "--root",
                str(root),
                "--plan",
                str(plan),
            ],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )

    assert result.returncode != 0
    assert "unsupported Rustack service in deployment plan: lambda" in result.stderr


def test_up_dry_run_reports_the_exact_selected_services() -> None:
    with tempfile.TemporaryDirectory(prefix="minco-local-docker-") as temporary:
        directory = Path(temporary)
        executable = directory / "docker"
        executable.write_text("#!/bin/sh\nprintf 'unexpected docker call: %s\\n' \"$*\"\n")
        executable.chmod(0o755)
        environment = os.environ.copy()
        environment["PATH"] = f"{temporary}:{environment['PATH']}"
        environment["MINCO_DEPLOYMENT_PLAN"] = str(write_plan(directory))
        result = subprocess.run(
            ["bash", "scripts/dev/up.sh", "--dry-run"],
            cwd=ROOT,
            env=environment,
            check=True,
            capture_output=True,
            text=True,
        )

    assert result.stdout.splitlines() == [
        "MINCO_RUSTACK_SERVICES=ssm,sts",
        "docker compose -f infra/local/compose.yaml up -d --wait postgres rustack",
    ]


def test_compose_passes_the_derived_service_set_to_rustack() -> None:
    environment = os.environ.copy()
    environment["MINCO_RUSTACK_SERVICES"] = "ssm,sts"
    environment["MINCO_RUSTACK_PORT"] = "46566"
    result = subprocess.run(
        [
            "docker",
            "compose",
            "-f",
            "infra/local/compose.yaml",
            "config",
            "--format",
            "json",
        ],
        cwd=ROOT,
        env=environment,
        check=True,
        capture_output=True,
        text=True,
    )
    configuration = json.loads(result.stdout)

    assert configuration["services"]["rustack"]["image"] == (
        "ghcr.io/tyrchen/rustack@"
        "sha256:18cd91395e17453e2c34b299e45f4679dc2427473dc1db6541bbe212fd70a104"
    )
    assert configuration["services"]["rustack"]["environment"]["SERVICES"] == "ssm,sts"
    assert configuration["services"]["rustack"]["ports"][0]["published"] == "46566"


def test_reference_graph_exports_standard_local_runtime_configuration() -> None:
    result = topology()

    assert result["environment"] == {
        "AWS_ACCESS_KEY_ID": "test",
        "AWS_DEFAULT_REGION": "ap-southeast-2",
        "AWS_EC2_METADATA_DISABLED": "true",
        "AWS_ENDPOINT_URL": "http://127.0.0.1:4566",
        "AWS_SECRET_ACCESS_KEY": "test",
        "DATABASE_KIND": "postgres",
        "DATABASE_URL": "postgres://minco:minco@127.0.0.1:55432/minco_orders",
        "MIGRATION_DATABASE_URL": "postgres://minco:minco@127.0.0.1:55432/minco_orders",
    }


def test_rustack_port_override_updates_the_sdk_endpoint() -> None:
    environment = os.environ.copy()
    environment["MINCO_RUSTACK_PORT"] = "46566"

    result = topology(environment=environment)

    assert result["environment"]["AWS_ENDPOINT_URL"] == "http://127.0.0.1:46566"


def test_local_database_override_preserves_the_existing_development_database() -> None:
    environment = os.environ.copy()
    isolated_url = "postgres://minco:minco@127.0.0.1:55432/minco_rustack_test"
    environment["MINCO_LOCAL_DATABASE_URL"] = isolated_url

    result = topology(environment=environment)

    assert result["environment"]["DATABASE_URL"] == isolated_url
    assert result["environment"]["MIGRATION_DATABASE_URL"] == isolated_url


def test_run_print_env_exposes_the_graph_derived_runtime_configuration() -> None:
    with tempfile.TemporaryDirectory(prefix="minco-local-cargo-") as temporary:
        directory = Path(temporary)
        executable = directory / "cargo"
        executable.write_text("#!/bin/sh\nprintf 'unexpected cargo call: %s\\n' \"$*\"\n")
        executable.chmod(0o755)
        environment = os.environ.copy()
        environment["PATH"] = f"{temporary}:{environment['PATH']}"
        environment["MINCO_DEPLOYMENT_PLAN"] = str(write_plan(directory))
        result = subprocess.run(
            ["bash", "scripts/dev/run.sh", "--print-env"],
            cwd=ROOT,
            env=environment,
            check=True,
            capture_output=True,
            text=True,
        )

    assert result.stdout.splitlines() == [
        "AWS_ACCESS_KEY_ID=test",
        "AWS_DEFAULT_REGION=ap-southeast-2",
        "AWS_EC2_METADATA_DISABLED=true",
        "AWS_ENDPOINT_URL=http://127.0.0.1:4566",
        "AWS_SECRET_ACCESS_KEY=test",
        "DATABASE_KIND=postgres",
        "DATABASE_URL=postgres://minco:minco@127.0.0.1:55432/minco_orders",
        "MIGRATION_DATABASE_URL=postgres://minco:minco@127.0.0.1:55432/minco_orders",
    ]


def test_migrate_uses_the_same_graph_derived_database_configuration() -> None:
    with tempfile.TemporaryDirectory(prefix="minco-local-migrate-") as temporary:
        directory = Path(temporary)
        log = directory / "calls.log"
        cargo = directory / "cargo"
        cargo.write_text(
            "#!/bin/sh\n"
            "case \"$*\" in\n"
            "  *'db plan'*) printf '{\"digest\":\"test-plan-digest\"}\\n'; exit 0 ;;\n"
            "esac\n"
            "printf 'database=%s migration=%s cargo=%s\\n' "
            "\"$DATABASE_URL\" \"$MIGRATION_DATABASE_URL\" \"$*\" "
            ">> \"$MINCO_TEST_LOG\"\n"
        )
        cargo.chmod(0o755)
        environment = os.environ.copy()
        environment["PATH"] = f"{temporary}:{environment['PATH']}"
        environment["MINCO_TEST_LOG"] = str(log)
        environment["MINCO_DEPLOYMENT_PLAN"] = str(write_plan(directory))
        isolated_url = "postgres://minco:minco@127.0.0.1:55432/minco_migrate_test"
        environment["MINCO_LOCAL_DATABASE_URL"] = isolated_url

        subprocess.run(
            ["bash", "scripts/dev/migrate.sh"],
            cwd=ROOT,
            env=environment,
            check=True,
            capture_output=True,
            text=True,
        )

        calls = log.read_text().splitlines()

    assert len(calls) == 1
    assert calls[0].startswith(
        f"database={isolated_url} migration={isolated_url} "
        "cargo=minco db migrate --set orders-postgres "
    )
    assert "--expected-plan-digest test-plan-digest" in calls[0]
    assert "--receipt target/minco/dev/orders-postgres-" in calls[0]


def test_up_proves_rustack_readiness_through_the_standard_aws_endpoint() -> None:
    with tempfile.TemporaryDirectory(prefix="minco-local-boundaries-") as temporary:
        directory = Path(temporary)
        log = directory / "calls.log"
        docker = directory / "docker"
        docker.write_text(
            "#!/bin/sh\n"
            "printf 'docker %s\\n' \"$*\" >> \"$MINCO_TEST_LOG\"\n"
        )
        docker.chmod(0o755)
        aws = directory / "aws"
        aws.write_text(
            "#!/bin/sh\n"
            "printf 'aws endpoint=%s region=%s credentials=%s/%s %s\\n' "
            "\"$AWS_ENDPOINT_URL\" \"$AWS_DEFAULT_REGION\" "
            "\"$AWS_ACCESS_KEY_ID\" \"$AWS_SECRET_ACCESS_KEY\" \"$*\" "
            ">> \"$MINCO_TEST_LOG\"\n"
        )
        aws.chmod(0o755)
        environment = os.environ.copy()
        environment["PATH"] = f"{temporary}:{environment['PATH']}"
        environment["MINCO_TEST_LOG"] = str(log)
        environment["MINCO_RUSTACK_PORT"] = "46566"
        isolated_url = "postgres://minco:minco@127.0.0.1:55432/minco_up_test"
        environment["MINCO_LOCAL_DATABASE_URL"] = isolated_url
        result = subprocess.run(
            ["bash", "scripts/dev/up.sh"],
            cwd=ROOT,
            env=environment,
            check=True,
            capture_output=True,
            text=True,
        )

        calls = log.read_text().splitlines()

    assert calls == [
        "docker compose -f infra/local/compose.yaml up -d --wait postgres rustack",
        (
            "aws endpoint=http://127.0.0.1:46566 region=ap-southeast-2 "
            "credentials=test/test sts get-caller-identity"
        ),
    ]
    assert result.stdout.splitlines()[0] == f"PostgreSQL: {isolated_url}"
    assert result.stdout.splitlines()[-1] == (
        "Rustack (ssm,sts): http://127.0.0.1:46566"
    )


def main() -> int:
    test_reference_graph_selects_only_declared_local_dependencies()
    test_minco_dev_dry_run_exposes_the_safe_graph_before_startup()
    test_minco_dev_sqlite_profile_and_port_override_remain_local_and_explicit()
    test_provider_neutral_plugins_do_not_silently_select_aws_providers()
    test_offline_plan_rejects_unsupported_rustack_services()
    test_up_dry_run_reports_the_exact_selected_services()
    test_compose_passes_the_derived_service_set_to_rustack()
    test_reference_graph_exports_standard_local_runtime_configuration()
    test_rustack_port_override_updates_the_sdk_endpoint()
    test_local_database_override_preserves_the_existing_development_database()
    test_run_print_env_exposes_the_graph_derived_runtime_configuration()
    test_migrate_uses_the_same_graph_derived_database_configuration()
    test_up_proves_rustack_readiness_through_the_standard_aws_endpoint()
    print("local topology tests: passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
