#!/usr/bin/env bash
set -euo pipefail

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
repo_root="$(minco_repo_root)"
cd "$repo_root"

for command in aws cargo curl jq psql sam shasum uv; do
  require_command "$command"
done
: "${MINCO_DATABASE_URL_PARAMETER:?set MINCO_DATABASE_URL_PARAMETER to an existing dev SecureString}"
: "${AWS_REGION:=ap-southeast-2}"
: "${MINCO_DATABASE_PARAMETER_OWNED:=false}"
: "${MINCO_DATABASE_INSTANCE_OWNED:=false}"
: "${MINCO_DATABASE_MIGRATION_COMPLETE:=false}"
[[ "$MINCO_DATABASE_URL_PARAMETER" == /* ]] || {
  echo "MINCO_DATABASE_URL_PARAMETER must be an absolute SSM parameter name" >&2
  exit 1
}
[[ "$MINCO_DATABASE_PARAMETER_OWNED" == "true" || "$MINCO_DATABASE_PARAMETER_OWNED" == "false" ]] || {
  echo "MINCO_DATABASE_PARAMETER_OWNED must equal true or false" >&2
  exit 1
}
[[ "$MINCO_DATABASE_INSTANCE_OWNED" == "true" || "$MINCO_DATABASE_INSTANCE_OWNED" == "false" ]] || {
  echo "MINCO_DATABASE_INSTANCE_OWNED must equal true or false" >&2
  exit 1
}
[[ "$MINCO_DATABASE_MIGRATION_COMPLETE" == "true" || "$MINCO_DATABASE_MIGRATION_COMPLETE" == "false" ]] || {
  echo "MINCO_DATABASE_MIGRATION_COMPLETE must equal true or false" >&2
  exit 1
}

: "${MINCO_AWS_RUN_ID:=$(date -u +%Y%m%dt%H%M%Sz)-$$}"
initialize_cloud_journal
run_suffix="$(printf '%s' "$MINCO_AWS_RUN_ID" | shasum -a 256 | cut -c1-12)"
: "${MINCO_STACK_NAME:=minco-smoke-$run_suffix}"
: "${MINCO_AWS_ARTIFACT_BUCKET:=minco-smoke-$run_suffix}"
: "${MINCO_SMOKE_APPLICATION:=minco-$run_suffix}"
MINCO_AWS_ARTIFACT_BUCKET="${MINCO_AWS_ARTIFACT_BUCKET,,}"
MINCO_AWS_ARTIFACT_BUCKET="${MINCO_AWS_ARTIFACT_BUCKET:0:63}"
export \
  AWS_REGION \
  MINCO_STACK_NAME \
  MINCO_AWS_ARTIFACT_BUCKET \
  MINCO_DATABASE_INSTANCE_OWNED \
  MINCO_DATABASE_MIGRATION_COMPLETE \
  MINCO_DATABASE_PARAMETER_OWNED \
  MINCO_AWS_EVIDENCE_DIR \
  MINCO_AWS_RUN_ID \
  MINCO_SMOKE_APPLICATION \
  MINCO_AWS_TOUCH_LOG

# Build the native ZIP before creating billable or persistent cloud resources.
scripts/aws/build-lambda.sh

identity="$(
  aws_logged sts get-caller-identity \
    "confirm reviewed AWS account and principal before mutation" \
    --query '{Account:Account,Arn:Arn,UserId:UserId}' \
    --output json
)"
account_id="$(jq -er '.Account' <<<"$identity")"
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/caller-identity.json" "$identity"
unset identity
[[ "$account_id" =~ ^[0-9]{12}$ ]]

aws_logged ssm describe-parameters \
  "capture existing database parameter metadata before use; no value requested" \
  --parameter-filters "Key=Name,Option=Equals,Values=$MINCO_DATABASE_URL_PARAMETER" \
  --query 'Parameters[0].{Name:Name,Type:Type,Tier:Tier,DataType:DataType,KeyId:KeyId,Version:Version,LastModifiedDate:LastModifiedDate}' \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/database-parameter-before.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/database-parameter-before.json"
jq -e \
  --arg name "$MINCO_DATABASE_URL_PARAMETER" \
  '.Name == $name and .Type == "SecureString"' \
  "$MINCO_AWS_EVIDENCE_DIR/database-parameter-before.json" >/dev/null || {
  echo "the selected database parameter is absent or is not SecureString" >&2
  exit 1
}

key_id="$(jq -r '.KeyId // "alias/aws/ssm"' "$MINCO_AWS_EVIDENCE_DIR/database-parameter-before.json")"
if [[ "$key_id" != "alias/aws/ssm" && "$key_id" != "aws/ssm" ]]; then
  MINCO_DATABASE_KMS_KEY_ARN="$(
    aws_logged kms describe-key \
      "resolve customer-managed key ARN for least-privilege Lambda policy" \
      --key-id "$key_id" \
      --query KeyMetadata.Arn \
      --output text
  )"
  export MINCO_DATABASE_KMS_KEY_ARN
fi

if [[ "$MINCO_DATABASE_MIGRATION_COMPLETE" == "true" ]]; then
  if [[ ! -f "$MINCO_AWS_EVIDENCE_DIR/database-migration-complete.txt" ]] ||
    ! grep -qx true "$MINCO_AWS_EVIDENCE_DIR/database-migration-complete.txt"; then
    echo "pre-completed migration evidence is missing" >&2
    exit 1
  fi
else
  database_url="$(
    aws_logged ssm get-parameter \
      "read existing database parameter for explicit migration; value redacted" \
      --name "$MINCO_DATABASE_URL_PARAMETER" \
      --with-decryption \
      --query Parameter.Value \
      --output text
  )"
  record_external_database_touch \
    "explicit migration" \
    "apply release migrations before Lambda deployment; database URL redacted"
  migration_plan="$MINCO_AWS_EVIDENCE_DIR/database-migration-plan.json"
  cargo minco db plan --set orders-postgres --json >"$migration_plan"
  migration_digest="$(jq -er '.digest' "$migration_plan")"
  MIGRATION_DATABASE_URL="$database_url" \
    cargo minco db migrate \
      --set orders-postgres \
      --database-url-env MIGRATION_DATABASE_URL \
      --expected-plan-digest "$migration_digest" \
      --receipt "target/minco/aws/$MINCO_AWS_RUN_ID/database-migration-receipt.json" \
      --json >"$MINCO_AWS_EVIDENCE_DIR/database-migration-output.json"
  MIGRATION_DATABASE_URL="$database_url" \
    cargo minco db verify \
      --set orders-postgres \
      --database-url-env MIGRATION_DATABASE_URL \
      --json >"$MINCO_AWS_EVIDENCE_DIR/database-migration-verification.json"
  unset database_url
  unset migration_digest
  write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/database-migration-complete.txt" true
fi

cleanup_started=false
cleanup_on_exit() {
  local status=$?
  trap - EXIT INT TERM
  if [[ "$cleanup_started" == false ]]; then
    cleanup_started=true
    if ! scripts/aws/cleanup.sh; then
      status=1
    fi
  fi
  exit "$status"
}
trap cleanup_on_exit EXIT INT TERM

MINCO_SMOKE_JWT_TOKEN="$(scripts/aws/create-smoke-identity.sh)"
export MINCO_SMOKE_JWT_TOKEN
issuer="$(<"$MINCO_AWS_EVIDENCE_DIR/jwt-issuer.txt")"
client_id="$(<"$MINCO_AWS_EVIDENCE_DIR/cognito-client-id.txt")"

build_directory="target/lambda/orders-lambda"
smoke_config="$build_directory/minco.smoke.toml"
awk \
  -v issuer="$issuer" \
  -v audience="$client_id" \
  -v application="$MINCO_SMOKE_APPLICATION" \
  '
    /^application = / { print "application = \"" application "\""; next }
    /^issuer = / { print "issuer = \"" issuer "\""; next }
    /^audiences = / { print "audiences = [\"" audience "\"]"; next }
    /^artifact_path = / {
      print "artifact_path = \"target/lambda/orders-lambda/bootstrap.zip\""
      next
    }
    { print }
  ' examples/orders/config/minco.dev.toml >"$smoke_config"

plan="$build_directory/plan.json"
template="$build_directory/template.yaml"
MINCO_RELEASE_MANIFEST="$MINCO_AWS_EVIDENCE_DIR/release.json"
export MINCO_RELEASE_MANIFEST
cargo minco deploy plan --config "$smoke_config" --output "$plan"
cargo minco deploy render-sam --config "$smoke_config" --output "$template"
uv run --locked python scripts/validate_static.py
SAM_CLI_TELEMETRY=0 sam validate --lint --template-file "$template"
MINCO_CONFIG__APPLICATION__NAME="$MINCO_SMOKE_APPLICATION" cargo minco release create \
  --artifact "$build_directory/bootstrap.zip" \
  --plan "$plan" \
  --template "$template" \
  --output "$MINCO_RELEASE_MANIFEST"
cargo minco release verify "$MINCO_RELEASE_MANIFEST"

MINCO_AWS_EXECUTE_CHANGESET=yes scripts/aws/deploy.sh
scripts/aws/smoke.sh

unset MINCO_SMOKE_JWT_TOKEN
scripts/aws/cleanup.sh
cleanup_started=true
trap - EXIT INT TERM

printf 'Bounded real-AWS run passed and cleaned: %s\n' "$MINCO_AWS_RUN_ID"
