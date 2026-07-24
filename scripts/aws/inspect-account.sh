#!/usr/bin/env bash
set -euo pipefail

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
repo_root="$(minco_repo_root)"
cd "$repo_root"

for command in aws jq; do
  require_command "$command"
done
: "${AWS_REGION:=ap-southeast-2}"
: "${MINCO_AWS_RUN_ID:=inspection-$(date -u +%Y%m%dt%H%M%Sz)-$$}"
initialize_cloud_journal

aws_logged sts get-caller-identity \
  "identify the reviewed AWS account before any mutation" \
  --query '{Account:Account,Arn:Arn,UserId:UserId}' \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/caller-identity.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/caller-identity.json"
aws_logged ssm describe-parameters \
  "list SecureString metadata only to locate the existing Minco dev database parameter" \
  --parameter-filters Key=Type,Values=SecureString \
  --query 'Parameters[].{Name:Name,Tier:Tier,KeyId:KeyId,Version:Version,LastModifiedDate:LastModifiedDate}' \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/secure-parameters.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/secure-parameters.json"

jq '{
  account: input.Account,
  candidate_parameters: [
    .[]
    | select(.Name | test("minco|database|postgres|neon"; "i"))
  ]
}' \
  "$MINCO_AWS_EVIDENCE_DIR/secure-parameters.json" \
  "$MINCO_AWS_EVIDENCE_DIR/caller-identity.json"
