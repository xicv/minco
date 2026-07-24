#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
uv run --locked python scripts/validate_static.py
command -v sam >/dev/null || { echo 'SAM CLI is required' >&2; exit 1; }
SAM_CLI_TELEMETRY=0 sam validate --lint --template-file infra/aws/generated/template.yaml
