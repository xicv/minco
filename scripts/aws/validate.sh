#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
# shellcheck source=scripts/aws/lib/common.sh
source scripts/aws/lib/common.sh

command -v jq >/dev/null || { echo 'jq is required' >&2; exit 1; }

dummy_conninfo="$(
  postgres_url_to_conninfo \
    "postgresql://minco%20user:p%27ass%5Cword@db.example:5432/orders%20db?sslmode=verify-full&sslrootcert=%2Ftmp%2Frds%20ca.pem"
)"
[[ "$dummy_conninfo" == \
  "host='db.example' port='5432' user='minco user' password='p\\'ass\\\\word' dbname='orders db' sslmode='verify-full' sslrootcert='/tmp/rds ca.pem'" ]]
unset dummy_conninfo

regional_bucket_configuration="$(
  s3_tagged_create_configuration ap-southeast-2 validation-run
)"
jq -e '
  .LocationConstraint == "ap-southeast-2"
  and (.Tags | from_entries) == {
    "minco:managed": "true",
    "minco:purpose": "bounded-smoke",
    "minco:run-id": "validation-run"
  }
' <<<"$regional_bucket_configuration" >/dev/null
us_east_1_bucket_configuration="$(
  s3_tagged_create_configuration us-east-1 validation-run
)"
jq -e '
  (has("LocationConstraint") | not)
  and (.Tags | from_entries) == {
    "minco:managed": "true",
    "minco:purpose": "bounded-smoke",
    "minco:run-id": "validation-run"
  }
' <<<"$us_east_1_bucket_configuration" >/dev/null
unset regional_bucket_configuration us_east_1_bucket_configuration

bash scripts/aws/test-rehearsal-authority.sh
bash scripts/aws/test-multi-release-rehearsal-authority.sh
bash scripts/aws/test-multi-release-rehearsal-plan.sh
bash scripts/aws/test-multi-release-phase-result.sh
bash scripts/aws/test-bounded-multi-release-smoke.sh
bash scripts/aws/test-smoke-response-contract.sh
bash scripts/aws/test-temp-rds-cleanup.sh
uv run --locked python scripts/validate_static.py
command -v sam >/dev/null || { echo 'SAM CLI is required' >&2; exit 1; }
SAM_CLI_TELEMETRY=0 sam validate --lint --template-file infra/aws/generated/template.yaml
