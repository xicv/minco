#!/usr/bin/env python3
"""Run bounded local load qualification for a Minco release candidate."""

from __future__ import annotations

import argparse
import hashlib
import http.client
import json
import os
import platform
import subprocess
import tempfile
import time
import tomllib
from collections.abc import Mapping
from concurrent.futures import ThreadPoolExecutor
from datetime import UTC, datetime
from math import ceil, isfinite
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUTPUT = ROOT / "target" / "minco" / "candidate-load.json"
DEFAULT_TARGET = ROOT / "target" / "minco" / "candidate-load" / "cargo"
WORKSPACE_VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"][
    "package"
]["version"]
RELEASE_SERIES = ".".join(WORKSPACE_VERSION.split(".")[:2])
CANDIDATE_RECOVERY_RECORD = (
    f"verification/{RELEASE_SERIES}-candidate-recovery.json"
)
CANDIDATE_LOAD_RECORD = f"verification/{RELEASE_SERIES}-candidate-load.json"
MANDATORY_RELEASE_COMMANDS = (
    "uv run --locked python scripts/test/candidate_qualification.py",
    "./scripts/quality.sh",
    "proofs/realtime-appsync/scripts/test-local.sh",
    "npm run --prefix plugins/minco-plugin-feedback test:browser",
    "scripts/test/e2e.sh",
    "scripts/dev/rustack-smoke.sh",
    "scripts/release/publish.sh --skip-quality",
    "scripts/release/package-list.sh",
    f"scripts/release/candidate-recovery.sh --output {CANDIDATE_RECOVERY_RECORD}",
    f"scripts/release/candidate-load.sh --output {CANDIDATE_LOAD_RECORD}",
    "uv run --locked python scripts/source_manifest.py --check",
)
GATE_STATUSES = frozenset({"PASS", "FAIL", "BLOCKED", "NOT RUN"})


def validate_load_record(record: Mapping[str, Any]) -> None:
    """Reject a passing load record that omits a required evidence dimension."""

    if record.get("status") != "PASS":
        return
    if record.get("schema_version") != 2 or record.get("kind") != "minco.candidate-load-qualification.v2":
        raise ValueError("PASS load record requires candidate qualification schema v2")
    if record.get("production_slo") is not False:
        raise ValueError("PASS load record must explicitly set production_slo to false")
    if record.get("provider_contact") is not False:
        raise ValueError("local candidate load qualification must not claim provider contact")
    for section in ("source", "topology", "runner", "environment", "classification"):
        value = record.get(section)
        if not isinstance(value, Mapping) or not value:
            raise ValueError(f"PASS load record requires {section} provenance")
    source_tree_sha256 = record["source"].get("source_tree_sha256")
    if (
        not isinstance(source_tree_sha256, str)
        or len(source_tree_sha256) != 64
        or any(character not in "0123456789abcdef" for character in source_tree_sha256)
        or record["runner"].get("source_tree_sha256") != source_tree_sha256
    ):
        raise ValueError("PASS load record runner source tree must match verified source authority")
    if record["topology"].get("runtime") != "local_native" or record["topology"].get("ingress") != "local_tcp":
        raise ValueError("local candidate load topology must be local_native/local_tcp")
    if record["classification"].get("warm") is not True:
        raise ValueError("candidate load record must classify warm measurements")
    if record["classification"].get("cold_start_measured") is not False:
        raise ValueError("candidate load record must not imply cold-start evidence")
    for section in ("api", "worker", "queue", "cost", "artifacts"):
        value = record.get(section)
        if not isinstance(value, Mapping) or not value:
            raise ValueError(f"PASS load record requires {section} measurements")
    for section in ("api", "worker"):
        if record[section].get("failures") != 0:
            raise ValueError(f"PASS load record requires zero {section} failures")
    api_requests = record["api"].get("requests")
    latency = record["api"].get("latency")
    if not isinstance(api_requests, int) or isinstance(api_requests, bool) or api_requests <= 0:
        raise ValueError("PASS load record requires a positive API request count")
    validate_latency_summary(latency)
    if any(
        not isinstance(size, int) or isinstance(size, bool) or size <= 0
        for size in record["artifacts"].values()
    ):
        raise ValueError("PASS load record requires positive artifact sizes")


def finite_number(value: Any) -> bool:
    """Return whether a JSON measurement is a finite number and not a bool."""

    return isinstance(value, (int, float)) and not isinstance(value, bool) and isfinite(value)


def validate_latency_summary(value: Any) -> None:
    """Validate the ordered, finite latency contract used by candidate evidence."""

    if not isinstance(value, Mapping):
        raise ValueError("PASS load record requires an API latency summary")
    keys = ("minimum_ms", "p50_ms", "p95_ms", "p99_ms", "maximum_ms")
    measurements = [value.get(key) for key in keys]
    if any(not finite_number(item) or item < 0 for item in measurements):
        raise ValueError("latency measurements must be finite non-negative numbers")
    if measurements != sorted(measurements):
        raise ValueError("latency percentiles must be monotonic")


def validate_recovery_record(record: Mapping[str, Any]) -> None:
    """Reject a recovery PASS unless restored data and rollback policy were exercised."""

    if record.get("status") != "PASS":
        return
    for section in ("backup", "restore", "migration", "rollback"):
        value = record.get(section)
        if not isinstance(value, Mapping) or not value:
            raise ValueError(f"PASS recovery record requires {section} evidence")
    if record["backup"].get("integrity_check") != "ok":
        raise ValueError("PASS recovery record requires backup integrity")
    if record["restore"].get("integrity_check") != "ok":
        raise ValueError("PASS recovery record requires restore integrity")
    if record["restore"].get("application_read") is not True:
        raise ValueError("PASS recovery record requires an application read")
    if not all(record["migration"].get(key) is True for key in ("first_apply", "repeat_apply", "verify")):
        raise ValueError("PASS recovery record requires complete migration evidence")
    if record["rollback"].get("tests_passed") is not True:
        raise ValueError("PASS recovery record requires rollback tests")
    if record["rollback"].get("reverse_sql") is not False:
        raise ValueError("PASS recovery record must not claim reverse SQL")


def validate_release_gate_record(record: Mapping[str, Any]) -> None:
    """Require explicit status and complete command coverage for a release PASS."""

    commands = record.get("commands")
    if not isinstance(commands, list):
        raise ValueError("release gate record requires commands")
    indexed = {item.get("command"): item for item in commands if isinstance(item, Mapping)}
    missing = [command for command in MANDATORY_RELEASE_COMMANDS if command not in indexed]
    if missing:
        raise ValueError(f"release gate record omits mandatory command: {missing[0]}")
    for command in MANDATORY_RELEASE_COMMANDS:
        status = indexed[command].get("status")
        if status not in GATE_STATUSES:
            raise ValueError(f"release command has invalid status {status!r}: {command}")
        if record.get("status") == "PASS" and status != "PASS":
            raise ValueError(f"release PASS cannot contain {status}: {command}")
        if status == "PASS" and indexed[command].get("exit_code") != 0:
            raise ValueError(f"release command PASS requires exit 0: {command}")


def summarize_latencies(samples: list[float]) -> dict[str, float]:
    """Return deterministic nearest-rank latency measurements in milliseconds."""

    if not samples:
        raise ValueError("latency samples must not be empty")
    if any(not finite_number(sample) or sample < 0 for sample in samples):
        raise ValueError("latency samples must be finite non-negative numbers")
    ordered = sorted(samples)

    def nearest_rank(percentile: float) -> float:
        return ordered[max(0, ceil(percentile * len(ordered)) - 1)]

    return {
        "minimum_ms": round(ordered[0], 3),
        "p50_ms": round(nearest_rank(0.50), 3),
        "p95_ms": round(nearest_rank(0.95), 3),
        "p99_ms": round(nearest_rank(0.99), 3),
        "maximum_ms": round(ordered[-1], 3),
    }


def environment_record() -> dict[str, Any]:
    """Return sanitized runner provenance and its canonical fingerprint."""

    environment: dict[str, Any] = {
        "os": platform.system().lower(),
        "os_release": platform.release(),
        "architecture": platform.machine().lower(),
        "python": platform.python_version(),
        "github_actions": os.environ.get("GITHUB_ACTIONS") == "true",
    }
    fingerprint = hashlib.sha256(
        json.dumps(environment, separators=(",", ":"), sort_keys=True).encode()
    ).hexdigest()
    return {"fingerprint_sha256": fingerprint, "dimensions": environment}


def runner_record(source_tree_sha256: str) -> dict[str, Any]:
    """Return bounded CI identity without copying arbitrary environment values."""

    hosted = os.environ.get("GITHUB_ACTIONS") == "true"
    record: dict[str, Any] = {
        "scope": "github_hosted" if hosted else "local",
        "source_tree_sha256": source_tree_sha256,
    }
    if hosted:
        record.update(
            {
                "repository": os.environ.get("GITHUB_REPOSITORY", ""),
                "source_sha": os.environ.get("GITHUB_SHA", ""),
                "run_id": os.environ.get("GITHUB_RUN_ID", ""),
                "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT", ""),
                "runner_os": os.environ.get("RUNNER_OS", ""),
                "runner_arch": os.environ.get("RUNNER_ARCH", ""),
                "runner_image": os.environ.get("ImageOS", ""),
            }
        )
    return record


def run_checked(
    command: list[str],
    *,
    log_path: Path,
    environment: Mapping[str, str] | None = None,
    cwd: Path = ROOT,
) -> str:
    """Run a command, retain its private log, and return its standard output."""

    log_path.parent.mkdir(parents=True, exist_ok=True)
    process = subprocess.run(
        command,
        cwd=cwd,
        env=dict(environment) if environment is not None else None,
        capture_output=True,
        text=True,
        check=False,
    )
    log_path.write_text(process.stdout + process.stderr)
    if process.returncode != 0:
        rendered = " ".join(command)
        raise RuntimeError(f"{rendered} failed with exit {process.returncode}; see {log_path}")
    return process.stdout


def external_cargo_environment(target_dir: Path) -> dict[str, str]:
    """Apply the repository toolchain when Cargo runs outside the workspace."""

    toolchain = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    return os.environ | {
        "CARGO_TARGET_DIR": str(target_dir),
        "RUSTUP_TOOLCHAIN": str(toolchain),
    }


def safe_output_path(requested: Path) -> Path:
    """Confine generated evidence to ignored target or checked verification."""

    root = ROOT.resolve()
    candidate = requested if requested.is_absolute() else root / requested
    if candidate.is_symlink():
        raise ValueError("evidence output must not be a symlink")
    resolved = candidate.resolve(strict=False)
    try:
        relative = resolved.relative_to(root)
    except ValueError as error:
        raise ValueError("evidence output must stay under target/minco or verification") from error
    allowed = relative.parts[:2] == ("target", "minco") or relative.parts[:1] == (
        "verification",
    )
    if not allowed:
        raise ValueError("evidence output must stay under target/minco or verification")
    resolved.parent.mkdir(parents=True, exist_ok=True)
    if not resolved.parent.resolve().is_relative_to(root):
        raise ValueError("evidence output parent escapes the project")
    if resolved.exists() and not resolved.is_file():
        raise ValueError("evidence output must be a regular file")
    return resolved


def available_port() -> int:
    """Ask the kernel for a currently available loopback TCP port."""

    import socket

    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def request_order(port: int, index: int) -> tuple[float, int]:
    """Place one synthetic order through a fresh HTTP connection."""

    payload = json.dumps(
        {
            "customerReference": f"LOAD-{index:04d}",
            "lines": [{"sku": "SYNTHETIC-1", "quantity": 1}],
        },
        separators=(",", ":"),
    )
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=15)
    started = time.perf_counter()
    try:
        connection.request(
            "POST",
            "/orders",
            body=payload,
            headers={
                "Content-Type": "application/json",
                "Idempotency-Key": f"candidate-load-{index:04d}",
                "X-Minco-Subject": "candidate-load",
                "X-Minco-Permissions": "orders.create,orders.read",
                "Connection": "close",
            },
        )
        response = connection.getresponse()
        body = response.read()
        status = response.status
        if status != 201:
            raise RuntimeError(f"synthetic order {index} returned HTTP {status}")
        document = json.loads(body)
        if document.get("data", {}).get("customerReference") != f"LOAD-{index:04d}":
            raise RuntimeError(f"synthetic order {index} returned the wrong document")
        return (time.perf_counter() - started) * 1000.0, status
    finally:
        connection.close()


def wait_for_ready(port: int, process: subprocess.Popen[str], log_path: Path) -> None:
    """Wait for the local API or fail with its retained log."""

    for _ in range(160):
        if process.poll() is not None:
            raise RuntimeError(f"orders-local exited early; see {log_path}")
        connection = http.client.HTTPConnection("127.0.0.1", port, timeout=1)
        try:
            connection.request("GET", "/health/ready")
            response = connection.getresponse()
            response.read()
            if response.status == 200:
                return
        except OSError:
            pass
        finally:
            connection.close()
        time.sleep(0.125)
    raise RuntimeError(f"orders-local did not become ready; see {log_path}")


def benchmark_api(target_dir: Path, log_dir: Path, requests: int, concurrency: int) -> dict[str, Any]:
    """Exercise the real local Axum/SQLite application with bounded concurrency."""

    binary = target_dir / "release" / "orders-local"
    build_environment = os.environ | {"CARGO_TARGET_DIR": str(target_dir)}
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
        environment=build_environment,
        log_path=log_dir / "orders-build.log",
    )
    port = available_port()
    with tempfile.TemporaryDirectory(prefix="minco-candidate-api-") as temporary:
        database = Path(temporary) / "orders.sqlite"
        service_environment = os.environ | {
            "APP_ENV": "local",
            "API_HOST": "127.0.0.1",
            "API_PORT": str(port),
            "DATABASE_KIND": "sqlite",
            "SQLITE_PATH": str(database),
            "DATABASE_MAX_CONNECTIONS": "4",
            "ALLOW_DEVELOPMENT_HEADERS": "true",
            "ALLOWED_ORIGINS": "http://127.0.0.1:5173",
        }
        service_log_path = log_dir / "orders-api.log"
        with service_log_path.open("w") as service_log:
            process = subprocess.Popen(
                [str(binary)],
                cwd=ROOT,
                env=service_environment,
                stdout=service_log,
                stderr=subprocess.STDOUT,
                text=True,
            )
            try:
                wait_for_ready(port, process, service_log_path)
                wall_started = time.perf_counter()
                with ThreadPoolExecutor(max_workers=concurrency) as executor:
                    results = list(executor.map(lambda index: request_order(port, index), range(requests)))
                wall_seconds = time.perf_counter() - wall_started
            finally:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
    latencies = [latency for latency, _ in results]
    failures = sum(1 for _, status in results if status != 201)
    return {
        "scope": "loopback Axum with file-backed SQLite and synthetic orders",
        "requests": requests,
        "concurrency": concurrency,
        "database_max_connections": 4,
        "fresh_tcp_connections": requests,
        "failures": failures,
        "throughput_requests_per_second": round(requests / wall_seconds, 3),
        "latency": summarize_latencies(latencies),
        "wall_seconds": round(wall_seconds, 3),
        "artifact_path": str(binary.relative_to(ROOT)),
    }


def worker_benchmark_sources(project: Path) -> None:
    """Create a disposable external consumer that drives the public worker API."""

    manifest = f'''[package]
name = "minco-candidate-worker-load"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[dependencies]
async-trait = "=0.1.91"
aws_lambda_events = {{ version = "=1.2.0", default-features = false, features = ["sqs"] }}
minco-aws-worker = {{ path = {json.dumps(str(ROOT / "extensions" / "minco-aws-worker"))} }}
tokio = {{ version = "=1.52.0", features = ["macros", "rt-multi-thread", "sync", "time"] }}
'''
    source = r'''use async_trait::async_trait;
use aws_lambda_events::event::sqs::{SqsEvent, SqsMessage};
use minco_aws_worker::{process_sqs_event, MessageHandler, WorkerConfig, WorkerFailure, WorkerMessage};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::time::{Duration, Instant};

#[derive(Debug, Default)]
struct Handler {
    processed: AtomicUsize,
    active: AtomicUsize,
    maximum_active: AtomicUsize,
}

#[async_trait]
impl MessageHandler for Handler {
    async fn handle(&self, _message: WorkerMessage) -> Result<(), WorkerFailure> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum_active.fetch_max(active, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(1)).await;
        self.processed.fetch_add(1, Ordering::SeqCst);
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let batches = 100usize;
    let batch_size = 10usize;
    let max_concurrency = 4usize;
    let handler = Arc::new(Handler::default());
    let started = Instant::now();
    let mut failures = 0usize;
    for batch in 0..batches {
        let mut event = SqsEvent::default();
        event.records = (0..batch_size)
            .map(|item| {
                let mut message = SqsMessage::default();
                message.message_id = Some(format!("synthetic-{batch}-{item}"));
                message.body = Some("synthetic-worker-payload".to_owned());
                message
            })
            .collect();
        let response = process_sqs_event(
            event,
            Arc::clone(&handler),
            WorkerConfig {
                max_batch_size: batch_size,
                max_message_bytes: 256,
                max_concurrency,
            },
        )
        .await
        .expect("bounded synthetic batch");
        failures += response.batch_item_failures.len();
    }
    println!(
        "{{\"batches\":{batches},\"batch_size\":{batch_size},\"messages\":{},\"failures\":{failures},\"configured_max_concurrency\":{max_concurrency},\"observed_max_concurrency\":{},\"wall_seconds\":{:.6}}}",
        handler.processed.load(Ordering::SeqCst),
        handler.maximum_active.load(Ordering::SeqCst),
        started.elapsed().as_secs_f64(),
    );
}
'''
    (project / "src").mkdir(parents=True)
    (project / "Cargo.toml").write_text(manifest)
    (project / "src" / "main.rs").write_text(source)


def benchmark_worker(target_dir: Path, log_dir: Path) -> dict[str, Any]:
    """Drive the public SQS worker boundary from a disposable external crate."""

    with tempfile.TemporaryDirectory(prefix="minco-candidate-worker-") as temporary:
        project = Path(temporary)
        worker_benchmark_sources(project)
        environment = external_cargo_environment(target_dir)
        run_checked(
            ["cargo", "generate-lockfile", "--manifest-path", str(project / "Cargo.toml")],
            environment=environment,
            cwd=project,
            log_path=log_dir / "worker-lockfile.log",
        )
        output = run_checked(
            [
                "cargo",
                "run",
                "--quiet",
                "--release",
                "--locked",
                "--manifest-path",
                str(project / "Cargo.toml"),
            ],
            environment=environment,
            cwd=project,
            log_path=log_dir / "worker-load.log",
        )
    record = json.loads(output.strip().splitlines()[-1])
    if record["messages"] != record["batches"] * record["batch_size"]:
        raise RuntimeError("worker benchmark did not process the complete queue projection")
    if record["observed_max_concurrency"] > record["configured_max_concurrency"]:
        raise RuntimeError("worker exceeded configured concurrency")
    record["scope"] = "public process_sqs_event API with synthetic standard-queue batches"
    record["throughput_messages_per_second"] = round(record["messages"] / record["wall_seconds"], 3)
    return record


def queue_measurements() -> dict[str, Any]:
    """Read the reviewed worker/queue limits from the schema-2 fixture."""

    fixture = ROOT / "crates" / "minco-plan" / "tests" / "fixtures" / "api_worker_standard_v2.toml"
    document = tomllib.loads(fixture.read_text())
    worker = next(function for function in document["functions"] if function["role"] == "worker")
    queue = document["queues"][0]
    trigger = next(trigger for trigger in document["triggers"] if trigger["kind"] == "sqs")
    return {
        "fixture": str(fixture.relative_to(ROOT)),
        "batch_size": trigger["batch_size"],
        "batching_window_seconds": trigger["batching_window_seconds"],
        "maximum_concurrency": trigger["maximum_concurrency"],
        "reserved_concurrency": worker["reserved_concurrency"],
        "database_connections_per_instance": worker["database_connections_per_instance"],
        "visibility_timeout_seconds": queue["visibility_timeout_seconds"],
        "function_timeout_seconds": worker["timeout_seconds"],
        "visibility_to_function_timeout_ratio": queue["visibility_timeout_seconds"]
        / worker["timeout_seconds"],
        "retention_seconds": queue["retention_seconds"],
        "report_batch_item_failures": trigger["report_batch_item_failures"],
    }


def artifact_measurements(target_dir: Path, api: Mapping[str, Any]) -> dict[str, int]:
    """Measure the actual local release artifacts used by this gate."""

    orders_binary = ROOT / str(api["artifact_path"])
    worker_archives = sorted((target_dir / "release" / "deps").glob("libminco_aws_worker-*.rlib"))
    if not worker_archives:
        raise RuntimeError("worker benchmark did not produce a release rlib")
    return {
        "orders_local_bytes": orders_binary.stat().st_size,
        "worker_crate_bytes": max(path.stat().st_size for path in worker_archives),
        "source_manifest_bytes": (ROOT / "verification" / "source-manifest.json").stat().st_size,
    }


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--api-requests", type=int, default=80)
    parser.add_argument("--api-concurrency", type=int, default=8)
    return parser.parse_args()


def main() -> int:
    args = parse_arguments()
    if not 20 <= args.api_requests <= 500:
        raise SystemExit("--api-requests must be between 20 and 500")
    if not 1 <= args.api_concurrency <= 32:
        raise SystemExit("--api-concurrency must be between 1 and 32")
    log_dir = ROOT / "target" / "minco" / "candidate-load" / "logs"
    run_checked(
        ["uv", "run", "--locked", "python", "scripts/source_manifest.py", "--check"],
        log_path=log_dir / "source-manifest.log",
    )
    manifest = json.loads((ROOT / "verification" / "source-manifest.json").read_text())
    api = benchmark_api(DEFAULT_TARGET, log_dir, args.api_requests, args.api_concurrency)
    worker = benchmark_worker(DEFAULT_TARGET, log_dir)
    queue = queue_measurements()
    modeled_invocations = ceil(worker["messages"] / queue["batch_size"])
    record: dict[str, Any] = {
        "schema_version": 2,
        "kind": "minco.candidate-load-qualification.v2",
        "status": "PASS",
        "generated_at": datetime.now(UTC).isoformat(),
        "production_slo": False,
        "provider_contact": False,
        "source": {
            "version": manifest["version"],
            "source_tree_sha256": manifest["source_tree_sha256"],
        },
        "scope": "bounded local synthetic load; no AWS contact and no production SLO claim",
        "topology": {"runtime": "local_native", "ingress": "local_tcp"},
        "runner": runner_record(manifest["source_tree_sha256"]),
        "environment": environment_record(),
        "classification": {"warm": True, "cold_start_measured": False},
        "tools": {
            "python": platform.python_version(),
            "cargo": run_checked(["cargo", "--version"], log_path=log_dir / "cargo-version.log").strip(),
        },
        "api": api,
        "worker": worker,
        "queue": queue,
        "cost": {
            "modeled_lambda_invocations": modeled_invocations,
            "synthetic_api_requests": api["requests"],
            "synthetic_worker_messages": worker["messages"],
            "maximum_database_connections": api["database_max_connections"]
            + queue["maximum_concurrency"] * queue["database_connections_per_instance"],
            "pricing_claim": "none",
            "note": "Counts expose billable dimensions; current provider prices and a dollar estimate are intentionally not inferred from local execution.",
        },
        "artifacts": artifact_measurements(DEFAULT_TARGET, api),
        "limitations": [
            "Loopback timings are machine-specific measurements, not an AWS or production performance SLO.",
            "The worker gate invokes the real public batch processor but does not emulate Lambda poller scaling, SQS retries, throttling or network latency.",
            "The reviewed queue limits model connection and cost pressure; no provider request or charge occurred.",
        ],
    }
    validate_load_record(record)
    output = safe_output_path(args.output)
    output.write_text(json.dumps(record, allow_nan=False, indent=2, sort_keys=True) + "\n")
    print(f"Candidate load qualification PASS: {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
