#!/usr/bin/env python3
"""Rehearse bounded migration, backup, restore, and rollback recovery."""

from __future__ import annotations

import argparse
import http.client
import json
import os
import sqlite3
import subprocess
import tempfile
import time
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, Callable

from candidate_qualification import (
    ROOT,
    run_checked,
    safe_output_path,
    validate_recovery_record,
    wait_for_ready,
)


DEFAULT_OUTPUT = ROOT / "target" / "minco" / "candidate-recovery.json"
DEFAULT_TARGET = ROOT / "target" / "minco" / "candidate-recovery" / "cargo"


def database_state(path: Path) -> dict[str, Any]:
    """Read only synthetic counts and SQLite's own integrity result."""

    with sqlite3.connect(path) as connection:
        integrity = connection.execute("PRAGMA integrity_check").fetchone()[0]
        rows = connection.execute("SELECT count(*) FROM orders").fetchone()[0]
        migrations = connection.execute("SELECT count(*) FROM _minco_orders_migrations").fetchone()[0]
    return {"integrity_check": integrity, "rows": rows, "migrations": migrations}


def create_synthetic_order(port: int) -> str:
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=10)
    try:
        connection.request(
            "POST",
            "/orders",
            body=json.dumps(
                {
                    "customerReference": "RECOVERY-SYNTHETIC",
                    "lines": [{"sku": "RECOVERY-1", "quantity": 1}],
                }
            ),
            headers={
                "Content-Type": "application/json",
                "Idempotency-Key": "candidate-recovery-1",
                "X-Minco-Subject": "candidate-recovery",
                "X-Minco-Permissions": "orders.create,orders.read",
            },
        )
        response = connection.getresponse()
        document = json.loads(response.read())
        if response.status != 201:
            raise RuntimeError(f"recovery order returned HTTP {response.status}")
        return str(document["data"]["id"])
    finally:
        connection.close()


def read_synthetic_order(port: int, order_id: str) -> None:
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=10)
    try:
        connection.request(
            "GET",
            f"/orders/{order_id}",
            headers={
                "X-Minco-Subject": "candidate-recovery",
                "X-Minco-Permissions": "orders.read",
            },
        )
        response = connection.getresponse()
        document = json.loads(response.read())
        if response.status != 200:
            raise RuntimeError(f"restored order returned HTTP {response.status}")
        if document.get("data", {}).get("customerReference") != "RECOVERY-SYNTHETIC":
            raise RuntimeError("restored application returned the wrong synthetic record")
    finally:
        connection.close()


def with_api(database: Path, binary: Path, log_path: Path, action: Callable[[int], Any]) -> Any:
    """Run one application-level recovery assertion against a selected database."""

    from candidate_qualification import available_port

    port = available_port()
    environment = os.environ | {
        "APP_ENV": "local",
        "API_HOST": "127.0.0.1",
        "API_PORT": str(port),
        "DATABASE_KIND": "sqlite",
        "SQLITE_PATH": str(database),
        "DATABASE_MAX_CONNECTIONS": "1",
        "ALLOW_DEVELOPMENT_HEADERS": "true",
        "ALLOWED_ORIGINS": "http://127.0.0.1:5173",
    }
    with log_path.open("w") as service_log:
        process = subprocess.Popen(
            [str(binary)],
            cwd=ROOT,
            env=environment,
            stdout=service_log,
            stderr=subprocess.STDOUT,
            text=True,
        )
        try:
            wait_for_ready(port, process, log_path)
            return action(port)
        finally:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)


def migration_command(plan_digest: str, receipt: Path) -> list[str]:
    """Use the catalog-owned public migration lifecycle, never a lower-level binary."""

    if receipt.is_absolute():
        receipt = receipt.relative_to(ROOT)
    return [
        "cargo",
        "minco",
        "db",
        "migrate",
        "--set",
        "orders-sqlite",
        "--database-url-env",
        "MINCO_CANDIDATE_DATABASE_URL",
        "--expected-plan-digest",
        plan_digest,
        "--receipt",
        str(receipt),
        "--allow-destructive",
        "--json",
    ]


def database_environment(database: Path) -> dict[str, str]:
    return os.environ | {
        "CARGO_TARGET_DIR": str(DEFAULT_TARGET),
        "MINCO_CANDIDATE_DATABASE_URL": f"sqlite://{database}",
    }


def receipt_root() -> Path:
    """Return the ignored project-contained boundary required by the CLI."""

    return ROOT / "target" / "minco" / "candidate-recovery" / "receipts"


def migrate(database: Path, plan_digest: str, receipt: Path, log_path: Path) -> None:
    run_checked(
        migration_command(plan_digest, receipt),
        environment=database_environment(database),
        log_path=log_path,
    )


def verify_migration(database: Path, log_path: Path) -> dict[str, Any]:
    output = run_checked(
        [
            "cargo",
            "minco",
            "db",
            "verify",
            "--set",
            "orders-sqlite",
            "--database-url-env",
            "MINCO_CANDIDATE_DATABASE_URL",
            "--json",
        ],
        environment=database_environment(database),
        log_path=log_path,
    )
    return json.loads(output)


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    return parser.parse_args()


def main() -> int:
    args = parse_arguments()
    log_dir = ROOT / "target" / "minco" / "candidate-recovery" / "logs"
    run_checked(
        ["uv", "run", "--locked", "python", "scripts/source_manifest.py", "--check"],
        log_path=log_dir / "source-manifest.log",
    )
    environment = os.environ | {"CARGO_TARGET_DIR": str(DEFAULT_TARGET)}
    run_checked(
        [
            "cargo",
            "build",
            "--locked",
            "--release",
            "-p",
            "orders-service",
            "--bin",
            "orders-local",
            "--features",
            "sqlite",
        ],
        environment=environment,
        log_path=log_dir / "orders-build.log",
    )
    run_checked(
        ["cargo", "test", "-p", "minco-deploy-aws", "--test", "rollback", "--locked"],
        environment=environment,
        log_path=log_dir / "rollback-tests.log",
    )
    run_checked(
        ["bash", "scripts/aws/test-multi-release-rehearsal-plan.sh"],
        log_path=log_dir / "multi-release-plan-tests.log",
    )
    local_binary = DEFAULT_TARGET / "release" / "orders-local"
    plan_output = run_checked(
        ["cargo", "minco", "db", "plan", "--set", "orders-sqlite", "--json"],
        environment=environment,
        log_path=log_dir / "migration-plan.log",
    )
    plan_digest = json.loads(plan_output)["digest"]
    recovery_started = time.perf_counter()
    receipt_root().mkdir(parents=True, exist_ok=True)
    with (
        tempfile.TemporaryDirectory(prefix="minco-candidate-recovery-") as temporary,
        tempfile.TemporaryDirectory(prefix="run-", dir=receipt_root()) as receipt_temporary,
    ):
        boundary = Path(temporary)
        receipts = Path(receipt_temporary)
        source = boundary / "source.sqlite"
        backup = boundary / "backup.sqlite"
        restored = boundary / "restored.sqlite"
        migrate(
            source,
            plan_digest,
            receipts / "migration-first-receipt.json",
            log_dir / "migration-first.log",
        )
        first_state = database_state(source)
        migrate(
            source,
            plan_digest,
            receipts / "migration-repeat-receipt.json",
            log_dir / "migration-repeat.log",
        )
        repeat_state = database_state(source)
        source_verification = verify_migration(source, log_dir / "migration-verify-source.log")
        order_id = with_api(
            source,
            local_binary,
            log_dir / "source-api.log",
            create_synthetic_order,
        )
        source_state = database_state(source)
        with sqlite3.connect(source) as source_connection, sqlite3.connect(backup) as backup_connection:
            source_connection.backup(backup_connection)
        backup_state = database_state(backup)

        source.unlink()
        with sqlite3.connect(backup) as backup_connection, sqlite3.connect(restored) as restore_connection:
            backup_connection.backup(restore_connection)
        migrate(
            restored,
            plan_digest,
            receipts / "migration-restored-receipt.json",
            log_dir / "migration-restored.log",
        )
        restored_state = database_state(restored)
        restored_verification = verify_migration(
            restored, log_dir / "migration-verify-restored.log"
        )
        with_api(
            restored,
            local_binary,
            log_dir / "restored-api.log",
            lambda port: read_synthetic_order(port, order_id),
        )
    manifest = json.loads((ROOT / "verification" / "source-manifest.json").read_text())
    record: dict[str, Any] = {
        "schema_version": 1,
        "kind": "minco.candidate-recovery-qualification.v1",
        "status": "PASS",
        "generated_at": datetime.now(UTC).isoformat(),
        "source": {
            "version": manifest["version"],
            "source_tree_sha256": manifest["source_tree_sha256"],
        },
        "data_boundary": "temporary synthetic SQLite only; directory removed by the runner",
        "recovery_time_seconds": round(time.perf_counter() - recovery_started, 3),
        "backup": backup_state,
        "restore": restored_state | {"application_read": True},
        "migration": {
            "first_apply": first_state["migrations"] > 0,
            "repeat_apply": first_state["migrations"] == repeat_state["migrations"],
            "verify": (
                source_verification["target_verified"] is True
                and restored_verification["target_verified"] is True
                and source_state["rows"] == backup_state["rows"] == restored_state["rows"] == 1
            ),
            "migration_count": restored_state["migrations"],
        },
        "rollback": {
            "tests_passed": True,
            "reverse_sql": False,
            "local_contracts": [
                "cargo test -p minco-deploy-aws --test rollback --locked",
                "bash scripts/aws/test-multi-release-rehearsal-plan.sh",
            ],
            "bounded_provider_precedent": {
                "status": "PASS",
                "scope": "M10-T08 exact prior/current/prior deployment and cleanup rehearsal; not rerun by M12-T05",
                "prior_revision": "9cbe8fdb64a6f68363fd1cac949ddfa554106667",
                "current_revision": "4573239d83fff91fffd79ea9bda58afbe217ffe9",
                "hosted_runs": [
                    "https://github.com/xicv/minco/actions/runs/30928588074",
                    "https://github.com/xicv/minco/actions/runs/30931041323",
                ],
                "cleanup_boundaries_absent": 14,
            },
        },
        "limitations": [
            "The restore gate proves a repeatable application-level SQLite path, not managed PostgreSQL point-in-time recovery or a product RPO/RTO.",
            "The provider rehearsal is retained historical evidence bound to its two exact revisions; this gate made no AWS call and did not redeploy the current documentation/local-tooling candidate.",
            "Database rollback still means forward-compatible application release recovery; Minco does not reverse SQL or repair application data automatically.",
        ],
    }
    validate_recovery_record(record)
    output = safe_output_path(args.output)
    output.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    print(f"Candidate recovery qualification PASS: {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
