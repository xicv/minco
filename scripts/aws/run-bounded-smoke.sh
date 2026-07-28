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
caller_arn="$(jq -er '.Arn' <<<"$identity")"
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/caller-identity.json" "$identity"
unset identity
[[ "$account_id" =~ ^[0-9]{12}$ ]]
case "$caller_arn" in
  arn:aws*:iam::"$account_id":role/*)
    expected_role_arn="$caller_arn"
    ;;
  arn:aws*:sts::"$account_id":assumed-role/*/*)
    partition="${caller_arn%%:sts::*}"
    role_session="${caller_arn#*:assumed-role/}"
    role_name="${role_session%%/*}"
    expected_role_arn="${partition}:iam::${account_id}:role/${role_name}"
    unset partition role_session role_name
    ;;
  *)
    echo "bounded deployment requires an IAM role or assumed-role caller" >&2
    exit 1
    ;;
esac
unset caller_arn

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

migration_plan="$MINCO_AWS_EVIDENCE_DIR/database-migration-plan.json"
migration_receipt="$MINCO_AWS_EVIDENCE_DIR/database-migration-receipt.json"
[[ -f "$migration_plan" && -f "$migration_receipt" ]] || {
  echo "exact migration plan and successful receipt are required before infrastructure apply" >&2
  exit 1
}

stack_error="$MINCO_AWS_EVIDENCE_DIR/stack-preflight-error.txt"
if aws_logged cloudformation describe-stacks \
  "ensure stack $MINCO_STACK_NAME does not pre-exist" \
  --stack-name "$MINCO_STACK_NAME" >/dev/null 2>"$stack_error"; then
  echo "refusing to mutate pre-existing stack $MINCO_STACK_NAME" >&2
  exit 1
elif ! grep -Eq 'does not exist' "$stack_error"; then
  echo "could not prove that stack $MINCO_STACK_NAME is absent" >&2
  sed -n '1,8p' "$stack_error" >&2
  exit 1
fi
rm -f "$stack_error"
write_evidence_value \
  "$MINCO_AWS_EVIDENCE_DIR/stack-preflight-absent.txt" \
  "$MINCO_STACK_NAME"

bucket_error="$MINCO_AWS_EVIDENCE_DIR/bucket-preflight-error.txt"
if aws_logged s3api head-bucket \
  "ensure artifact bucket $MINCO_AWS_ARTIFACT_BUCKET does not pre-exist" \
  --bucket "$MINCO_AWS_ARTIFACT_BUCKET" >/dev/null 2>"$bucket_error"; then
  echo "refusing to mutate pre-existing bucket $MINCO_AWS_ARTIFACT_BUCKET" >&2
  exit 1
elif ! grep -Eq '404|NoSuchBucket|Not Found' "$bucket_error"; then
  echo "could not prove that artifact bucket is absent" >&2
  sed -n '1,8p' "$bucket_error" >&2
  exit 1
fi
rm -f "$bucket_error"
write_evidence_value \
  "$MINCO_AWS_EVIDENCE_DIR/bucket-preflight-absent.txt" \
  "$MINCO_AWS_ARTIFACT_BUCKET"

bucket_arguments=(--bucket "$MINCO_AWS_ARTIFACT_BUCKET")
bucket_configuration="$(s3_tagged_create_configuration "$AWS_REGION" "$MINCO_AWS_RUN_ID")"
bucket_arguments+=(--create-bucket-configuration "$bucket_configuration")
aws_logged s3api create-bucket \
  "create the bounded SAM artifact bucket" \
  "${bucket_arguments[@]}" >/dev/null
unset bucket_configuration
aws_logged s3api put-public-access-block \
  "block all public access on the bounded artifact bucket" \
  --bucket "$MINCO_AWS_ARTIFACT_BUCKET" \
  --public-access-block-configuration \
  BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true \
  >/dev/null
aws_logged s3api put-bucket-encryption \
  "enable server-side encryption on the bounded artifact bucket" \
  --bucket "$MINCO_AWS_ARTIFACT_BUCKET" \
  --server-side-encryption-configuration \
  '{"Rules":[{"ApplyServerSideEncryptionByDefault":{"SSEAlgorithm":"AES256"},"BucketKeyEnabled":false}]}' \
  >/dev/null

target_config="$MINCO_AWS_EVIDENCE_DIR/deployment-targets.toml"
{
  printf 'schema_version = 1\ndefault_environment = "dev"\n\n'
  printf '[environments.dev]\nenabled = true\n'
  printf 'expected_account_id = "%s"\n' "$account_id"
  printf 'expected_region = "%s"\n' "$AWS_REGION"
  printf 'expected_role_arn = "%s"\n' "$expected_role_arn"
  printf 'stack_name = "%s"\n' "$MINCO_STACK_NAME"
  printf 'artifact_bucket = "%s"\n' "$MINCO_AWS_ARTIFACT_BUCKET"
  printf 'database_url_parameter_name = "%s"\n' "$MINCO_DATABASE_URL_PARAMETER"
  if [[ -n "${MINCO_DATABASE_KMS_KEY_ARN:-}" ]]; then
    printf 'database_kms_key_arn = "%s"\n' "$MINCO_DATABASE_KMS_KEY_ARN"
  fi
  if [[ -n "${MINCO_LAMBDA_SUBNET_IDS:-}" ]]; then
    printf 'lambda_subnet_ids = ["%s"]\n' \
      "${MINCO_LAMBDA_SUBNET_IDS//,/\",\"}"
    printf 'lambda_security_group_ids = ["%s"]\n' \
      "${MINCO_LAMBDA_SECURITY_GROUP_IDS//,/\",\"}"
  fi
} >"$target_config"
chmod 600 "$target_config"

release_digest="$(jq -er '.release_digest' "$MINCO_RELEASE_MANIFEST")"
MINCO_DEPLOY_PHASE=changeset \
MINCO_DEPLOY_TARGET_CONFIG="target/minco/aws/$MINCO_AWS_RUN_ID/deployment-targets.toml" \
MINCO_APPROVE_RELEASE_DIGEST="$release_digest" \
  scripts/aws/deploy.sh
change_set_receipt="$MINCO_AWS_EVIDENCE_DIR/change-set-receipt.json"
jq -e '
  .change_set.change_set_type == "create"
  and (.change_set.review.additions | length > 0)
  and (.change_set.review.modifications | length == 0)
  and (.change_set.review.replacements | length == 0)
  and (.change_set.review.deletions | length == 0)
  and (.change_set.review.imports | length == 0)
  and (.change_set.review.indeterminate | length == 0)
  and (.change_set.review.metadata_syncs | length == 0)
  and (
    [.change_set.review.additions[].resource_type]
    | all(
        . == "AWS::ApiGatewayV2::Api"
        or . == "AWS::ApiGatewayV2::Stage"
        or . == "AWS::IAM::Role"
        or . == "AWS::Lambda::Function"
        or . == "AWS::Lambda::Permission"
        or . == "AWS::Logs::LogGroup"
      )
  )
' "$change_set_receipt" >/dev/null || {
  echo "bounded change set is not an approved create-only resource set" >&2
  exit 1
}
change_set_digest="$(jq -er '.receipt_digest' "$change_set_receipt")"
MINCO_DEPLOY_PHASE=apply \
MINCO_DEPLOY_TARGET_CONFIG="target/minco/aws/$MINCO_AWS_RUN_ID/deployment-targets.toml" \
MINCO_CHANGESET_RECEIPT="target/minco/aws/$MINCO_AWS_RUN_ID/change-set-receipt.json" \
MINCO_MIGRATION_PLAN="target/minco/aws/$MINCO_AWS_RUN_ID/database-migration-plan.json" \
MINCO_MIGRATION_RECEIPT="target/minco/aws/$MINCO_AWS_RUN_ID/database-migration-receipt.json" \
MINCO_APPROVE_CHANGESET_DIGEST="$change_set_digest" \
  scripts/aws/deploy.sh
unset change_set_digest release_digest
scripts/aws/smoke.sh

unset MINCO_SMOKE_JWT_TOKEN
scripts/aws/cleanup.sh
cleanup_started=true
trap - EXIT INT TERM

printf 'Bounded real-AWS run passed and cleaned: %s\n' "$MINCO_AWS_RUN_ID"
