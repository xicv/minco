#!/usr/bin/env bash
set -euo pipefail

proof_root="$(cd "$(dirname "$0")/.." && pwd)"
repository_root="$(cd "$proof_root/../.." && pwd)"
template="$proof_root/aws/template.yaml"

cd "$repository_root"
uv run --locked python "$proof_root/scripts/check_aws_template.py" "$template"
sam validate --lint --region ap-southeast-2 --template-file "$template"
