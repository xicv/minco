#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
python3 scripts/validate_static.py
command -v sam >/dev/null || { echo 'SAM CLI is required' >&2; exit 1; }
sam validate --lint --template-file infra/aws/generated/template.yaml
