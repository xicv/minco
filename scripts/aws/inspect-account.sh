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
: "${MINCO_DATABASE_URL_PARAMETER:?set MINCO_DATABASE_URL_PARAMETER to the exact reviewed SecureString}"
: "${MINCO_REHEARSAL_AUTHORITY_FILE:?set MINCO_REHEARSAL_AUTHORITY_FILE to the exact reviewed authority document}"
: "${MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST:?set MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST to the reviewed authority SHA-256}"
: "${AWS_PROFILE:?set AWS_PROFILE to the approved non-root profile}"
normalized_ssm_parameter_name "$MINCO_DATABASE_URL_PARAMETER" || {
  echo "MINCO_DATABASE_URL_PARAMETER must be a normalized absolute SSM parameter name" >&2
  exit 1
}

source_revision="$(current_source_revision)"
database_boundary="$(
  jq -cn \
    --arg parameter_name "$MINCO_DATABASE_URL_PARAMETER" \
    '{
      mode: "existing-ssm-secure-string",
      parameter_name: $parameter_name,
      parameter_owned: false,
      instance_owned: false
    }'
)"
scripts/aws/validate-rehearsal-authority.sh \
  "$MINCO_REHEARSAL_AUTHORITY_FILE" \
  "$MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST" \
  "$MINCO_AWS_RUN_ID" \
  "$source_revision" \
  "$AWS_REGION" \
  "$AWS_PROFILE" \
  dev \
  "$database_boundary" \
  bounded-direct-smoke-v1 \
  cleanup-bounded-direct-smoke-v1
initialize_rehearsal_deadline "$MINCO_REHEARSAL_AUTHORITY_FILE"
authority_account_id="$(jq -er '.expected_account_id' "$MINCO_REHEARSAL_AUTHORITY_FILE")"
authority_role_arn="$(jq -er '.expected_role_arn' "$MINCO_REHEARSAL_AUTHORITY_FILE")"
initialize_cloud_journal
write_rehearsal_authority_receipt \
  "$MINCO_REHEARSAL_AUTHORITY_FILE" \
  "$MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST" \
  "$MINCO_AWS_EVIDENCE_DIR/rehearsal-authority-receipt.json"

identity="$(aws_logged sts get-caller-identity \
  "identify the reviewed AWS account before any mutation" \
  --query '{Account:Account,Arn:Arn,UserId:UserId}' \
  --output json)"
account_id="$(jq -er '.Account' <<<"$identity")"
caller_arn="$(jq -er '.Arn' <<<"$identity")"
case "$caller_arn" in
  arn:aws*:iam::"$account_id":role/*)
    caller_role_arn="$caller_arn"
    ;;
  arn:aws*:sts::"$account_id":assumed-role/*/*)
    partition="${caller_arn%%:sts::*}"
    role_session="${caller_arn#*:assumed-role/}"
    role_name="${role_session%%/*}"
    caller_role_arn="${partition}:iam::${account_id}:role/${role_name}"
    ;;
  *)
    echo "account inspection requires an IAM role or assumed-role caller" >&2
    exit 1
    ;;
esac
[[ "$account_id" == "$authority_account_id" && "$caller_role_arn" == "$authority_role_arn" ]] || {
  echo "AWS caller does not match the exact account and role in rehearsal authority" >&2
  exit 1
}
unset account_id authority_account_id authority_role_arn caller_arn caller_role_arn identity

parameter_metadata="$(aws_logged ssm describe-parameters \
  "verify exact approved SecureString metadata without requesting its value" \
  --parameter-filters "Key=Name,Option=Equals,Values=$MINCO_DATABASE_URL_PARAMETER" \
  --query 'Parameters[0].{Type:Type,Tier:Tier,KeyId:KeyId,Version:Version,LastModifiedDate:LastModifiedDate}' \
  --output json)"
jq -e '.Type == "SecureString"' <<<"$parameter_metadata" >/dev/null || {
  echo "the exact approved database parameter is absent or is not SecureString" >&2
  exit 1
}
jq '{
  schema_version: 1,
  caller_account_match: true,
  caller_role_match: true,
  parameter_exists: true,
  parameter_type: .Type,
  parameter_tier: .Tier,
  customer_managed_key: ((.KeyId // "alias/aws/ssm") != "alias/aws/ssm" and .KeyId != "aws/ssm"),
  parameter_version_present: (.Version != null),
  last_modified_present: (.LastModifiedDate != null)
}' <<<"$parameter_metadata" >"$MINCO_AWS_EVIDENCE_DIR/account-inspection.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/account-inspection.json"
unset parameter_metadata
jq . "$MINCO_AWS_EVIDENCE_DIR/account-inspection.json"
