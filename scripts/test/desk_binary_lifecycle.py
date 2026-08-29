#!/usr/bin/env python3
"""Spawned-binary worker lifecycle proof (exact-head review R1).

The shipped ``minco-desk-local`` executable must run its background jobs
worker while serving HTTP, complete a durable notification job without
any manual ``run_once()`` call, and exit cleanly on SIGINT.
"""
from __future__ import annotations

import json
import os
import signal
import socket
import sqlite3
import subprocess
import tempfile
import time
import unittest
import urllib.error
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
BINARY = os.path.join(ROOT, "target", "debug", "minco-desk-local")
AGENT_TOKEN = "binary-proof-agent-token"


def free_port() -> int:
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def request_json(method: str, url: str, token: str, body: dict | None = None,
                 headers: dict[str, str] | None = None) -> tuple[int, dict | str]:
    payload = json.dumps(body).encode() if body is not None else None
    headers = dict(headers or {})
    headers["Authorization"] = f"Bearer {token}"
    if payload is not None:
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=payload, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=10) as response:
            raw = response.read().decode()
            return response.status, (json.loads(raw) if raw else {})
    except urllib.error.HTTPError as error:
        raw = error.read().decode()
        return error.code, (json.loads(raw) if raw else raw)


class DeskBinaryLifecycleTests(unittest.TestCase):
    def test_worker_runs_and_sigint_exits_cleanly(self) -> None:
        self.assertTrue(os.path.exists(BINARY),
                        "run cargo build -p minco-desk-example --bin minco-desk-local first")
        with tempfile.TemporaryDirectory() as directory:
            database = os.path.join(directory, "desk-binary.sqlite")
            port = free_port()
            origin = f"http://127.0.0.1:{port}"
            env = {
                **os.environ,
                "DESK_DATABASE_URL": f"sqlite://{database}?mode=rwc",
                "DESK_HOST": "127.0.0.1",
                "DESK_PORT": str(port),
                "DESK_PROJECT_ID": "desk-proof",
                "DESK_PORTAL_ORIGIN": "http://127.0.0.1:8090",
                "DESK_AGENT_TOKEN": AGENT_TOKEN,
                "DESK_CSRF_SECRET": "binary-proof-csrf-secret-binary-proof-csrf-secret",
                "DESK_ENVIRONMENT": "local",
            }
            with open(os.devnull, "wb") as null:
                child = subprocess.Popen(
                    [BINARY],
                    env=env,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.PIPE,
                    stdin=null,
                )
            try:
                # Readiness: the HTTP surface answers a bearer-authenticated
                # bootstrap within a bounded window.
                deadline = time.monotonic() + 30.0
                bootstrap_status = 0
                while time.monotonic() < deadline:
                    try:
                        bootstrap_status, _ = request_json(
                            "GET", f"{origin}/_minco/ticketing/agent/bootstrap", AGENT_TOKEN)
                        break
                    except (urllib.error.URLError, ConnectionError, OSError):
                        time.sleep(0.2)
                self.assertEqual(bootstrap_status, 200, "the binary never became ready")

                # Real health endpoints execute the registered checks
                # (exact-head review R10).
                for path in ("/live", "/ready"):
                    with self.subTest(path=path):
                        status, _ = request_json("GET", f"{origin}{path}", AGENT_TOKEN)
                        self.assertEqual(status, 200, f"{path} must report healthy")

                status, ticket = request_json("POST", f"{origin}/_minco/ticketing/tickets",
                                              AGENT_TOKEN, body={
                    "project_id": "desk-proof",
                    "subject": "The worker must run",
                    "description": "This ticket proves the shipped worker loop executes.",
                    "requester": {"subject": "user-1"},
                    "channel": "portal",
                })
                self.assertEqual(status, 201, ticket)
                ticket_id = ticket["ticket"]["id"]
                revision = ticket["ticket"]["revision"]
                status, reply = request_json(
                    "POST", f"{origin}/_minco/ticketing/tickets/{ticket_id}/agent-replies",
                    AGENT_TOKEN,
                    body={"body": "Fixed; the background worker delivered this notice."},
                    headers={"If-Match": f'"ticket:{ticket_id}:{revision + 1}"'},
                )
                self.assertEqual(status, 200, reply)

                # Domain events are delivered by the same worker cycle
                # (exact-head review R16): the durable activity intents
                # are published to the in-process outbox. Poll until the
                # cycle following the job completes.
                deadline = time.monotonic() + 10.0
                published = 0
                while time.monotonic() < deadline:
                    with sqlite3.connect(f"file:{database}?mode=ro", uri=True) as conn:
                        published = conn.execute(
                            "SELECT COUNT(*) FROM ticketing_activity_intents"
                            " WHERE published_at IS NOT NULL").fetchone()[0]
                    if published > 0:
                        break
                    time.sleep(0.2)
                self.assertGreater(
                    published, 0,
                    "the worker must deliver domain events, not only jobs")

                # The background worker loop (500 ms interval) must complete
                # the durable notification job on its own.
                deadline = time.monotonic() + 30.0
                while True:
                    with sqlite3.connect(f"file:{database}?mode=ro", uri=True) as conn:
                        rows = conn.execute(
                            "SELECT status FROM minco_jobs"
                            " WHERE worker_profile = 'ticketing-mail'").fetchall()
                    statuses = [row[0] for row in rows]
                    if statuses and all(value == "succeeded" for value in statuses):
                        break
                    self.assertTrue(
                        time.monotonic() < deadline,
                        f"background worker did not finish the job: {statuses}")
                    time.sleep(0.2)

                # SIGINT shuts the process down cleanly.
                os.kill(child.pid, signal.SIGINT)
                exit_code = child.wait(timeout=30)
                self.assertEqual(exit_code, 0,
                                 "the binary must exit cleanly after SIGINT")
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait(timeout=10)




class NonLocalFailClosedTests(unittest.TestCase):
    """Exact-head review R19: trivial secrets, memory/read-only databases
    and inherited local defaults must refuse non-local startup."""

    def _run(self, env_overrides: dict) -> subprocess.CompletedProcess:
        env = {
            **os.environ,
            "DESK_ENVIRONMENT": "production",
            "DESK_DATABASE_URL": "sqlite:///tmp/nonlocal.sqlite?mode=rwc",
            "DESK_AGENT_TOKEN": "aB3dEf7hIj9lMn2pQr5tUv8wXy4zC1bE6gK0dJ3fH",
            "DESK_CSRF_SECRET": "zY4xW7vUtSrQpOnMlKjIhGfEdCbAzXwV5tSr2qP",
            "DESK_PORTAL_ORIGIN": "https://desk.example.test",
            "DESK_ALLOWED_ORIGINS": "https://desk.example.test",
        }
        env.update(env_overrides)
        return subprocess.run(
            [BINARY],
            env=env,
            capture_output=True,
            text=True,
            timeout=30,
        )

    def test_trivial_secret_is_rejected(self) -> None:
        result = self._run({"DESK_AGENT_TOKEN": "x"})
        self.assertNotEqual(result.returncode, 0, "a 1-char token must fail closed")
        self.assertIn("at least 32 characters", result.stderr)

    def test_memory_database_is_rejected(self) -> None:
        result = self._run({"DESK_DATABASE_URL": "sqlite://:memory:?mode=memory"})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("read-write", result.stderr)

    def test_read_only_database_is_rejected(self) -> None:
        result = self._run({"DESK_DATABASE_URL": "sqlite:///tmp/ro.sqlite?mode=ro"})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("read-write", result.stderr)

    def test_low_entropy_secret_is_rejected(self) -> None:
        result = self._run({"DESK_AGENT_TOKEN": "a" * 40})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("real entropy", result.stderr)

    def test_http_portal_origin_is_rejected(self) -> None:
        result = self._run({"DESK_PORTAL_ORIGIN": "http://desk.example.test"})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("HTTPS", result.stderr)

    def test_wildcard_origin_is_rejected(self) -> None:
        result = self._run({"DESK_PORTAL_ORIGIN": "https://*.example.test"})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("HTTPS", result.stderr)

    def test_inherited_return_paths_are_rejected(self) -> None:
        env = {
            **os.environ,
            "DESK_ENVIRONMENT": "production",
            "DESK_DATABASE_URL": "sqlite:///tmp/nonlocal3.sqlite?mode=rwc",
            "DESK_AGENT_TOKEN": "aB3dEf7hIj9lMn2pQr5tUv8wXy4zC1bE6gK0dJ3fH",
            "DESK_CSRF_SECRET": "zY4xW7vUtSrQpOnMlKjIhGfEdCbAzXwV5tSr2qP",
            "DESK_PORTAL_ORIGIN": "https://desk.example.test",
            "DESK_ALLOWED_ORIGINS": "https://desk.example.test",
        }
        env.pop("DESK_ALLOWED_RETURN_PATHS", None)
        result = subprocess.run([BINARY], env=env, capture_output=True, text=True, timeout=30)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("DESK_ALLOWED_RETURN_PATHS", result.stderr)

    def test_http_return_paths_are_rejected(self) -> None:
        result = self._run({
            "DESK_ALLOWED_RETURN_PATHS":
                "http://app.example.test=/orders",
        })
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("HTTPS origins", result.stderr)

    def test_inherited_portal_origin_is_rejected(self) -> None:
        env = {
            **os.environ,
            "DESK_ENVIRONMENT": "production",
            "DESK_DATABASE_URL": "sqlite:///tmp/nonlocal2.sqlite?mode=rwc",
            "DESK_AGENT_TOKEN": "aB3dEf7hIj9lMn2pQr5tUv8wXy4zC1bE6gK0dJ3fH",
            "DESK_CSRF_SECRET": "zY4xW7vUtSrQpOnMlKjIhGfEdCbAzXwV5tSr2qP",
        }
        env.pop("DESK_PORTAL_ORIGIN", None)
        env.pop("DESK_ALLOWED_ORIGINS", None)
        result = subprocess.run([BINARY], env=env, capture_output=True, text=True, timeout=30)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("DESK_PORTAL_ORIGIN", result.stderr)


if __name__ == "__main__":
    unittest.main()
