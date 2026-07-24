#!/usr/bin/env python3
"""Deterministic static conformance checks for the official Feedback plugin."""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
CONTRACT = ROOT / "plugins/minco-plugin-feedback/openapi/feedback.openapi.yaml"
PLUGIN = ROOT / "plugins/minco-plugin-feedback/src/plugin.rs"
SERVICE = ROOT / "plugins/minco-plugin-feedback/src/service.rs"
WIDGET = ROOT / "plugins/minco-plugin-feedback/assets/widget.js"
POSTGRES = ROOT / "plugins/minco-plugin-feedback/migrations/postgres/0001_feedback.sql"
SQLITE = ROOT / "plugins/minco-plugin-feedback/migrations/sqlite/0001_feedback.sql"


def fail(message: str) -> None:
    raise AssertionError(message)


def contract_operations(document: dict) -> dict[str, tuple[str, str, bool]]:
    result: dict[str, tuple[str, str, bool]] = {}
    for path, item in document.get("paths", {}).items():
        for method in ("get", "post", "put", "patch", "delete", "options", "head"):
            operation = item.get(method)
            if not operation:
                continue
            operation_id = operation.get("operationId")
            if not operation_id:
                fail(f"{method.upper()} {path} has no operationId")
            if operation_id in result:
                fail(f"duplicate OpenAPI operationId {operation_id}")
            public = operation.get("security") == []
            result[operation_id] = (method.upper(), path, public)
    return result


def rust_operations(source: str) -> dict[str, tuple[str, str, bool]]:
    pattern = re.compile(
        r'\(\s*"(?P<id>[A-Za-z][A-Za-z0-9]*)"\s*,\s*'
        r'"(?P<method>GET|POST|PUT|PATCH|DELETE|OPTIONS|HEAD)"\s*,\s*'
        r'"(?P<path>/[^"\\]+)"\s*,\s*(?P<public>true|false)\s*,?\s*\)',
        re.MULTILINE,
    )
    result: dict[str, tuple[str, str, bool]] = {}
    for match in pattern.finditer(source):
        value = (
            match.group("method"),
            match.group("path"),
            match.group("public") == "true",
        )
        operation_id = match.group("id")
        if operation_id in result:
            fail(f"duplicate Rust feedback operation {operation_id}")
        result[operation_id] = value
    return result


def main() -> int:
    document = yaml.safe_load(CONTRACT.read_text())
    openapi = contract_operations(document)
    rust = rust_operations(PLUGIN.read_text())
    if openapi != rust:
        missing = sorted(set(openapi) - set(rust))
        extra = sorted(set(rust) - set(openapi))
        mismatched = sorted(
            key for key in set(openapi) & set(rust) if openapi[key] != rust[key]
        )
        fail(
            f"Feedback operation inventory mismatch: missing={missing}, "
            f"extra={extra}, mismatched={mismatched}"
        )

    widget_schema = document["components"]["schemas"]["FeedbackWidgetConfig"]
    required = set(widget_schema.get("required", []))
    if "token_storage" not in required:
        fail("FeedbackWidgetConfig must require token_storage")
    token_schema = widget_schema["properties"].get("token_storage")
    if token_schema != {"type": "string", "enum": ["session", "local"], "default": "session"}:
        fail("FeedbackWidgetConfig token_storage schema is not the reviewed contract")

    widget = WIDGET.read_text()
    required_fragments = [
        "navigator.mediaDevices.getDisplayMedia",
        "navigator.mediaDevices.getUserMedia",
        "MediaRecorder",
        "sessionStorage",
        "config.token_storage === 'local' ? localStorage : sessionStorage",
        "value.search = ''",
        "X-Minco-Feedback-Token",
    ]
    for fragment in required_fragments:
        if fragment not in widget:
            fail(f"widget is missing reviewed behavior: {fragment}")
    prohibited = ["innerHTML", "outerHTML", "eval(", "document.cookie"]
    for fragment in prohibited:
        if fragment in widget:
            fail(f"widget contains prohibited browser primitive: {fragment}")

    http_source = (ROOT / "plugins/minco-plugin-feedback/src/http.rs").read_text()
    for fragment in ["private, no-store", "nosniff"]:
        if fragment not in http_source:
            fail(f"feedback attachment response is missing security header: {fragment}")

    service = SERVICE.read_text()
    for fragment in [
        "pub enum FeedbackTokenStorage",
        "FeedbackTokenStorage::Session",
        'token_storage: self.token_storage.as_str().into()',
    ]:
        if fragment not in service:
            fail(f"service is missing token-storage behavior: {fragment}")

    for migration in (POSTGRES, SQLITE):
        sql = migration.read_text().lower()
        for fragment in [
            "create table if not exists minco_feedback_threads",
            "client_token_hash",
            "needs_clarification",
            "ready_for_development",
            "revision",
        ]:
            if fragment not in sql:
                fail(f"{migration.relative_to(ROOT)} is missing {fragment}")

    report = {
        "status": "passed",
        "operation_count": len(openapi),
        "operations": sorted(openapi),
        "widget_checks": len(required_fragments) + len(prohibited),
        "attachment_response_checks": 2,
        "database_profiles": ["postgres", "sqlite"],
    }
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    for output in [
        ROOT / "target/minco/feedback-contract.json",
        ROOT / "verification/feedback-contract.json",
    ]:
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(rendered)
    print(rendered, end="")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, KeyError, TypeError, yaml.YAMLError) as error:
        print(f"feedback conformance failed: {error}", file=sys.stderr)
        raise SystemExit(1)
