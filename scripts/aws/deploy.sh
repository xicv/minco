#!/usr/bin/env bash
set -euo pipefail

# Thin compatibility wrapper around the fail-closed deployment controller.
# Artifact buckets and deployment targets are provisioned and reviewed
# separately; this script never rebuilds, replans, or combines review with
# apply.

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
repo_root="$(minco_repo_root)"
cd "$repo_root"

for command in aws cargo jq sam; do
  require_command "$command"
done

: "${MINCO_DEPLOY_PHASE:?set MINCO_DEPLOY_PHASE to changeset or apply}"
: "${MINCO_RELEASE_MANIFEST:?set MINCO_RELEASE_MANIFEST}"
: "${MINCO_DEPLOY_TARGET_CONFIG:?set MINCO_DEPLOY_TARGET_CONFIG to a reviewed target catalog}"
: "${MINCO_AWS_RUN_ID:?set MINCO_AWS_RUN_ID}"
[[ "$MINCO_DEPLOY_PHASE" == "changeset" || "$MINCO_DEPLOY_PHASE" == "apply" ]] || {
  echo "MINCO_DEPLOY_PHASE must equal changeset or apply" >&2
  exit 1
}

initialize_cloud_journal
cargo minco release verify "$MINCO_RELEASE_MANIFEST"

change_set_receipt="$MINCO_AWS_EVIDENCE_DIR/change-set-receipt.json"
deployment_receipt="$MINCO_AWS_EVIDENCE_DIR/deployment-receipt.json"

if [[ "$MINCO_DEPLOY_PHASE" == "changeset" ]]; then
  : "${MINCO_APPROVE_RELEASE_DIGEST:?set MINCO_APPROVE_RELEASE_DIGEST to the reviewed release digest}"
  record_cloud_touch \
    "aws:cloudformation" \
    "guarded-changeset" \
    "package exact release and create an unexecuted change set"
  cargo minco deploy changeset \
    --target-config "$MINCO_DEPLOY_TARGET_CONFIG" \
    --manifest "$MINCO_RELEASE_MANIFEST" \
    --output "$MINCO_AWS_EVIDENCE_RELATIVE/change-set-receipt.json" \
    --approve-release-digest "$MINCO_APPROVE_RELEASE_DIGEST" \
    --json >"$MINCO_AWS_EVIDENCE_DIR/change-set-output.json"
  chmod 600 \
    "$change_set_receipt" \
    "$MINCO_AWS_EVIDENCE_DIR/change-set-output.json"
  jq -e '
    .change_set.status == "create_complete"
    and .change_set.execution_status == "available"
  ' "$change_set_receipt" >/dev/null
  printf 'Created unexecuted change set; approve receipt digest %s before apply\n' \
    "$(jq -er '.receipt_digest' "$change_set_receipt")"
  exit 0
fi

: "${MINCO_CHANGESET_RECEIPT:=$change_set_receipt}"
: "${MINCO_MIGRATION_PLAN:?set MINCO_MIGRATION_PLAN to the reviewed JSON plan}"
: "${MINCO_MIGRATION_RECEIPT:?set MINCO_MIGRATION_RECEIPT to the successful migration receipt}"
: "${MINCO_APPROVE_CHANGESET_DIGEST:?set MINCO_APPROVE_CHANGESET_DIGEST to the reviewed change-set receipt digest}"

record_cloud_touch \
  "aws:cloudformation" \
  "guarded-apply" \
  "execute the exact approved change set after migration evidence verification"
cargo minco deploy apply \
  --changeset "$MINCO_CHANGESET_RECEIPT" \
  --migration-plan "$MINCO_MIGRATION_PLAN" \
  --migration-receipt "$MINCO_MIGRATION_RECEIPT" \
  --receipt "$MINCO_AWS_EVIDENCE_RELATIVE/deployment-receipt.json" \
  --approve-changeset-digest "$MINCO_APPROVE_CHANGESET_DIGEST" \
  --json >"$MINCO_AWS_EVIDENCE_DIR/deploy-apply-output.json"
chmod 600 \
  "$deployment_receipt" \
  "$MINCO_AWS_EVIDENCE_DIR/deploy-apply-output.json"

stack_name="$(
  jq -er '.change_set.stack_name' "$MINCO_CHANGESET_RECEIPT"
)"
aws_logged cloudformation describe-stacks \
  "retain deployed outputs and status for $stack_name" \
  --stack-name "$stack_name" \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/stack.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/stack.json"
write_evidence_value \
  "$MINCO_AWS_EVIDENCE_DIR/function-name.txt" \
  "$(jq -er '.Stacks[0].Outputs[] | select(.OutputKey=="ApiFunctionName").OutputValue' "$MINCO_AWS_EVIDENCE_DIR/stack.json")"
write_evidence_value \
  "$MINCO_AWS_EVIDENCE_DIR/api-url.txt" \
  "$(jq -er '.Stacks[0].Outputs[] | select(.OutputKey=="CandidateApiUrl").OutputValue' "$MINCO_AWS_EVIDENCE_DIR/stack.json")"
write_evidence_value \
  "$MINCO_AWS_EVIDENCE_DIR/live-api-url.txt" \
  "$(jq -er '.Stacks[0].Outputs[] | select(.OutputKey=="ApiUrl").OutputValue' "$MINCO_AWS_EVIDENCE_DIR/stack.json")"
aws_logged cloudformation list-stack-resources \
  "retain physical resource identifiers for independent cleanup verification" \
  --stack-name "$stack_name" \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/stack-resources.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/stack-resources.json"
write_evidence_value \
  "$MINCO_AWS_EVIDENCE_DIR/function-role-name.txt" \
  "$(jq -er '.StackResourceSummaries[] | select(.ResourceType=="AWS::IAM::Role").PhysicalResourceId' "$MINCO_AWS_EVIDENCE_DIR/stack-resources.json")"
write_evidence_value \
  "$MINCO_AWS_EVIDENCE_DIR/http-api-id.txt" \
  "$(jq -er '.StackResourceSummaries[] | select(.ResourceType=="AWS::ApiGatewayV2::Api").PhysicalResourceId' "$MINCO_AWS_EVIDENCE_DIR/stack-resources.json")"

printf 'Applied reviewed release %s to %s; hosted verification remains pending\n' \
  "$(jq -r '.release_id' "$MINCO_RELEASE_MANIFEST")" \
  "$stack_name"
