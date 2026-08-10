#!/usr/bin/env python3
"""Qualify deterministic Minco agent workflows without invoking a model."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "verification/agent-workflows.json"
SKILLS = [
    "minco-diagnose",
    "minco-framework-task",
    "minco-lifecycle",
    "minco-operation",
    "minco-plugin",
    "minco-release",
    "minco-review",
    "minco-web-application",
]
command_count = 0


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Qualify and record deterministic Minco agent workflows."
    )
    destination = parser.add_mutually_exclusive_group()
    destination.add_argument(
        "--output",
        type=Path,
        help="write the canonical receipt to this path",
    )
    destination.add_argument(
        "--check-output",
        type=Path,
        help="fail when this existing receipt differs from current qualification",
    )
    return parser.parse_args(argv)


def confined_evidence_path(path: Path) -> tuple[Path, str]:
    """Resolve one receipt path beneath verification without following symlinks."""
    lexical = path if path.is_absolute() else ROOT / path
    normalized = Path(os.path.abspath(lexical))
    verification_lexical = ROOT / "verification"
    verification = (ROOT / "verification").resolve()
    try:
        relative = normalized.relative_to(verification_lexical)
    except ValueError as error:
        raise ValueError("agent workflow receipt must remain under verification/") from error
    if not relative.parts:
        raise ValueError("agent workflow receipt must name a file under verification/")

    current = verification_lexical
    if current.is_symlink() or not current.is_dir():
        raise ValueError("verification/ must be a real directory")
    for part in relative.parts:
        current = current / part
        if current.is_symlink():
            raise ValueError("agent workflow receipt path must not contain symlinks")
    if current.exists() and not current.is_file():
        raise ValueError("agent workflow receipt must be a regular file")
    try:
        normalized.resolve(strict=False).relative_to(verification)
    except ValueError as error:
        raise ValueError("agent workflow receipt must remain under verification/") from error
    return normalized, f"verification/{relative.as_posix()}"


def cli_binary() -> Path:
    configured = os.environ.get("MINCO_CLI")
    candidates = [Path(configured)] if configured else []
    candidates.extend(
        [
            ROOT / "target/debug/cargo-minco",
        ]
    )
    for candidate in candidates:
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return candidate.resolve()
    raise AssertionError(
        "cargo-minco binary is unavailable; build it or set MINCO_CLI to an exact binary"
    )


def create_project(parent: Path, name: str) -> Path:
    root = parent / name
    root.mkdir()
    (root / "minco.toml").write_text(
        """schema = 1
name = "agent-evaluation"
contract = "openapi.yaml"
generated = "generated"
deployment_config = "deploy.toml"
roadmap = "roadmap.yaml"
tasks = "tasks"
plugin_catalog = "plugins.toml"
quality = "quality.toml"
"""
    )
    (root / "AGENTS.md").write_text("# Application agent instructions\n")
    return root


def invoke(
    root: Path, arguments: list[str], *, success: bool = True
) -> subprocess.CompletedProcess[str]:
    global command_count
    command_count += 1
    environment = os.environ.copy()
    environment["RUST_BACKTRACE"] = "0"
    completed = subprocess.run(
        [str(cli_binary()), "--root", str(root), "--json", "agent", *arguments],
        cwd=root,
        env=environment,
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )
    if (completed.returncode == 0) != success:
        raise AssertionError(
            f"unexpected agent {' '.join(arguments)} result: "
            f"stdout={completed.stdout!r} stderr={completed.stderr!r}"
        )
    return completed


def invoke_json(root: Path, arguments: list[str]) -> dict[str, Any]:
    completed = invoke(root, arguments)
    return json.loads(completed.stdout)


def snapshot(root: Path) -> str:
    value = hashlib.sha256()
    for path in sorted(path for path in root.rglob("*") if path.is_file()):
        relative = path.relative_to(root).as_posix().encode()
        contents = path.read_bytes()
        value.update(len(relative).to_bytes(8, "big"))
        value.update(relative)
        value.update(len(contents).to_bytes(8, "big"))
        value.update(contents)
    return value.hexdigest()


def install(root: Path, target: str) -> dict[str, Any]:
    plan = invoke_json(root, ["plan", "--target", target])
    assert plan["safe"] is True
    report = invoke_json(
        root,
        [
            "sync",
            "--target",
            target,
            "--expect-plan-digest",
            plan["plan_digest"],
        ],
    )
    assert report["applied"] is True
    return plan


def projected_skill_bytes(root: Path, client: str) -> dict[str, bytes]:
    projection = ".agents/skills" if client == "codex" else ".claude/skills"
    base = root / projection
    return {
        path.relative_to(base).as_posix(): path.read_bytes()
        for path in sorted(path for path in base.rglob("*") if path.is_file())
    }


def canonical_skill_bytes() -> dict[str, bytes]:
    base = ROOT / "crates/minco-cli/assets/agent/skills"
    return {
        path.relative_to(base).as_posix(): path.read_bytes()
        for path in sorted(path for path in base.rglob("*") if path.is_file())
    }


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        receipt_path, receipt_relative = confined_evidence_path(
            args.check_output or args.output or OUTPUT
        )
    except ValueError as error:
        print(str(error), file=sys.stderr)
        return 2
    scenario_source = json.loads(
        (ROOT / "crates/minco-cli/assets/agent/evals/scenarios.json").read_text()
    )
    scenarios_by_id = {
        scenario["id"]: scenario for scenario in scenario_source["scenarios"]
    }
    framework_boundary = scenarios_by_id["framework-generated-application"]
    review_boundary = scenarios_by_id["review-local-green"]
    release_boundary = scenarios_by_id["release-feature-complete"]
    lifecycle_boundary = scenarios_by_id["lifecycle-plan-not-apply"]
    framework_application_mode_separated = (
        "application mode" in framework_boundary["required_concepts"]
        and "run framework task workflow" in framework_boundary["forbidden_actions"]
    )
    evidence_lane_upgrade_forbidden = (
        "local evidence only" in review_boundary["required_concepts"]
        and "claim deployment" in review_boundary["forbidden_actions"]
    )
    implicit_side_effects_forbidden = (
        "create release" in release_boundary["forbidden_actions"]
        and "deploy" in lifecycle_boundary["forbidden_actions"]
    )
    assert framework_application_mode_separated
    assert evidence_lane_upgrade_forbidden
    assert implicit_side_effects_forbidden

    with tempfile.TemporaryDirectory(prefix="minco-agent-eval-") as temporary:
        parent = Path(temporary)
        installed = create_project(parent, "installed")
        install_plan = install(installed, "all")
        before_eval = snapshot(installed)
        reports = {
            target: invoke_json(installed, ["eval", "--target", target])
            for target in ["codex", "claude", "all"]
        }
        after_eval = snapshot(installed)
        assert before_eval == after_eval
        assert all(report["status"] == "passed" for report in reports.values())
        assert all(
            report["forward_model"]["status"] == "not_run"
            and report["bounds"]
            == {
                "writes": 0,
                "commands_executed": 0,
                "network_requests": 0,
                "model_invocations": 0,
            }
            for report in reports.values()
        )
        codex_bytes = projected_skill_bytes(installed, "codex")
        claude_bytes = projected_skill_bytes(installed, "claude")
        assert codex_bytes == claude_bytes == canonical_skill_bytes()
        assert len(codex_bytes) == 24

        stale = create_project(parent, "stale")
        stale_plan = invoke_json(stale, ["plan", "--target", "codex"])
        stale_destination = stale / ".agents/skills/minco-operation/SKILL.md"
        stale_destination.parent.mkdir(parents=True)
        stale_destination.write_text("user-owned instructions\n")
        stale_sync = invoke(
            stale,
            [
                "sync",
                "--target",
                "codex",
                "--expect-plan-digest",
                stale_plan["plan_digest"],
            ],
            success=False,
        )
        stale_plan_rejected = "stale agent plan digest" in stale_sync.stderr
        stale_destination_preserved = (
            stale_destination.read_text() == "user-owned instructions\n"
        )
        assert stale_plan_rejected
        assert stale_destination_preserved

        user_owned = create_project(parent, "user-owned")
        user_claude = user_owned / "CLAUDE.md"
        user_claude.write_text("# Existing Claude instructions\n")
        user_plan = invoke_json(user_owned, ["plan", "--target", "all"])
        assert user_plan["safe"] is True
        assert any(
            action["code"] == "claude_project_instructions"
            for action in user_plan["manual_actions"]
        )
        invoke_json(
            user_owned,
            [
                "sync",
                "--target",
                "all",
                "--expect-plan-digest",
                user_plan["plan_digest"],
            ],
        )
        user_owned_claude_preserved = (
            user_claude.read_text() == "# Existing Claude instructions\n"
        )
        assert user_owned_claude_preserved

    all_report = reports["all"]
    scenario_results = all_report["scenarios"]["results"]
    assert {result["skill"] for result in scenario_results} == set(SKILLS)
    assert all_report["scenario_suite_digest"] == hashlib.sha256(
        (ROOT / "crates/minco-cli/assets/agent/evals/scenarios.json").read_bytes()
    ).hexdigest()
    report = {
        "schema_version": 1,
        "status": "ok",
        "operation": "deterministic_agent_workflow_qualification",
        "minco_version": all_report["minco_version"],
        "bundle_digest": all_report["bundle_digest"],
        "scenario_suite_digest": all_report["scenario_suite_digest"],
        "targets": {
            target: {
                "status": result["status"],
                "projection": result["projection"]["status"],
                "scenarios": result["scenarios"]["status"],
            }
            for target, result in reports.items()
        },
        "projection": {
            "canonical_files_per_client": 24,
            "codex_path": ".agents/skills",
            "claude_path": ".claude/skills",
            "byte_parity": True,
            "read_only_eval": before_eval == after_eval,
            "initial_plan_digest": install_plan["plan_digest"],
        },
        "scenarios": {
            "total": all_report["scenarios"]["total"],
            "trigger": all_report["scenarios"]["trigger"],
            "boundary": all_report["scenarios"]["boundary"],
            "skills_covered": all_report["scenarios"]["skills_covered"],
            "contracts_valid": all(
                result["status"] == "passed" for result in scenario_results
            ),
        },
        "negative_cases": {
            "stale_plan_rejected": stale_plan_rejected,
            "stale_destination_preserved": stale_destination_preserved,
            "user_owned_claude_preserved": user_owned_claude_preserved,
            "framework_application_mode_separated": framework_application_mode_separated,
            "evidence_lane_upgrade_forbidden": evidence_lane_upgrade_forbidden,
            "implicit_side_effects_forbidden": implicit_side_effects_forbidden,
        },
        "harness": {
            "commands_executed": command_count,
            "network_requests": 0,
            "model_invocations": 0,
            "permitted_persistent_output": receipt_relative,
        },
        "forward_model": {
            "status": "not_run",
            "clients": ["codex", "claude"],
            "detail": "No model-driven result is represented by this deterministic evidence.",
        },
        "evidence_lanes": {
            "source": "checked-in assets and scenario contracts",
            "local": "deterministic CLI and filesystem boundary checks",
            "hosted": "absent",
            "deployment": "absent",
            "runtime": "absent",
            "review": "absent",
        },
        "limitations": [
            "Scenario contract validation is not a model quality score.",
            "Client discovery is proven through documented project layouts and byte parity, not interactive client UI automation.",
            "No hosted, deployment, runtime, production, or acceptance evidence was produced.",
        ],
    }
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.check_output is not None:
        try:
            current = receipt_path.read_text()
        except OSError as error:
            print(
                f"agent workflow receipt is unavailable: {receipt_relative}: {error}",
                file=sys.stderr,
            )
            return 1
        if current != rendered:
            print(
                f"agent workflow receipt is stale: {receipt_relative}",
                file=sys.stderr,
            )
            return 1
    else:
        receipt_path.parent.mkdir(parents=True, exist_ok=True)
        receipt_path.write_text(rendered)
    print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
