#!/usr/bin/env bash
set -euo pipefail

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
repo_root="$(minco_repo_root)"
cd "$repo_root"

for command in aws jq sam; do
  require_command "$command"
done

: "${MINCO_STACK_NAME:?set MINCO_STACK_NAME}"
: "${MINCO_DATABASE_URL_PARAMETER:?set MINCO_DATABASE_URL_PARAMETER}"
: "${MINCO_AWS_ARTIFACT_BUCKET:?set MINCO_AWS_ARTIFACT_BUCKET}"
: "${MINCO_RELEASE_MANIFEST:?set MINCO_RELEASE_MANIFEST}"
: "${MINCO_AWS_EXECUTE_CHANGESET:?set MINCO_AWS_EXECUTE_CHANGESET=yes after approving the bounded run}"
: "${AWS_REGION:=ap-southeast-2}"
[[ "$MINCO_AWS_EXECUTE_CHANGESET" == "yes" ]] || {
  echo "MINCO_AWS_EXECUTE_CHANGESET must equal yes" >&2
  exit 1
}
[[ "$MINCO_DATABASE_URL_PARAMETER" == /* ]] || {
  echo "MINCO_DATABASE_URL_PARAMETER must be an absolute SSM parameter name" >&2
  exit 1
}
if [[ -n "${MINCO_LAMBDA_SUBNET_IDS:-}" || -n "${MINCO_LAMBDA_SECURITY_GROUP_IDS:-}" ]]; then
  [[ -n "${MINCO_LAMBDA_SUBNET_IDS:-}" && -n "${MINCO_LAMBDA_SECURITY_GROUP_IDS:-}" ]] || {
    echo "MINCO_LAMBDA_SUBNET_IDS and MINCO_LAMBDA_SECURITY_GROUP_IDS must be set together" >&2
    exit 1
  }
  [[ "$MINCO_LAMBDA_SUBNET_IDS" =~ ^subnet-[a-z0-9]+(,subnet-[a-z0-9]+)*$ ]] || {
    echo "MINCO_LAMBDA_SUBNET_IDS is invalid" >&2
    exit 1
  }
  [[ "$MINCO_LAMBDA_SECURITY_GROUP_IDS" =~ ^sg-[a-z0-9]+(,sg-[a-z0-9]+)*$ ]] || {
    echo "MINCO_LAMBDA_SECURITY_GROUP_IDS is invalid" >&2
    exit 1
  }
fi
require_safe_name "MINCO_STACK_NAME" "$MINCO_STACK_NAME"
require_safe_name "MINCO_AWS_ARTIFACT_BUCKET" "$MINCO_AWS_ARTIFACT_BUCKET"
initialize_cloud_journal

cargo minco release verify "$MINCO_RELEASE_MANIFEST"
template="$(
  jq -er '.deployment_template.path' "$MINCO_RELEASE_MANIFEST"
)"
artifact="$(
  jq -er '
    [.artifacts[] | select(.function_id == "api")]
    | if length == 1
      then .[0].file.path
      else error("release must contain exactly one api artifact")
      end
  ' "$MINCO_RELEASE_MANIFEST"
)"
plan="$(
  jq -er '.deployment_plan.path' "$MINCO_RELEASE_MANIFEST"
)"
[[ -f "$template" && -f "$artifact" && -f "$plan" ]] || {
  echo "release manifest template, artifact or deployment plan is missing" >&2
  exit 1
}
jq -e '
  .auth.kind != "jwt"
  or (
    (.auth.issuer | startswith("https://"))
    and (.auth.issuer | endswith(".invalid") | not)
    and (.auth.audiences | length > 0)
  )
' "$plan" >/dev/null || {
  echo "refusing to deploy a JWT plan with a placeholder or incomplete issuer" >&2
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
  echo "could not prove that bucket $MINCO_AWS_ARTIFACT_BUCKET is absent" >&2
  sed -n '1,8p' "$bucket_error" >&2
  exit 1
fi
rm -f "$bucket_error"
write_evidence_value \
  "$MINCO_AWS_EVIDENCE_DIR/bucket-preflight-absent.txt" \
  "$MINCO_AWS_ARTIFACT_BUCKET"

function_name="$(jq -er '.application + "-" + .environment + "-api"' "$plan")"
function_error="$MINCO_AWS_EVIDENCE_DIR/function-preflight-error.txt"
if aws_logged lambda get-function \
  "ensure Lambda function $function_name does not pre-exist" \
  --function-name "$function_name" >/dev/null 2>"$function_error"; then
  echo "refusing to mutate pre-existing Lambda function $function_name" >&2
  exit 1
elif ! grep -Eq 'ResourceNotFoundException|Function not found' "$function_error"; then
  echo "could not prove that Lambda function $function_name is absent" >&2
  sed -n '1,8p' "$function_error" >&2
  exit 1
fi
rm -f "$function_error"

bucket_arguments=(--bucket "$MINCO_AWS_ARTIFACT_BUCKET")
bucket_configuration="$(s3_tagged_create_configuration "$AWS_REGION" "$MINCO_AWS_RUN_ID")"
bucket_arguments+=(--create-bucket-configuration "$bucket_configuration")
aws_logged s3api create-bucket \
  "atomically create and tag temporary SAM artifact bucket $MINCO_AWS_ARTIFACT_BUCKET" \
  "${bucket_arguments[@]}" >/dev/null
unset bucket_configuration
aws_logged s3api put-public-access-block \
  "block all public access on temporary artifact bucket" \
  --bucket "$MINCO_AWS_ARTIFACT_BUCKET" \
  --public-access-block-configuration \
  BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true \
  >/dev/null
aws_logged s3api put-bucket-encryption \
  "enable SSE-S3 on temporary artifact bucket" \
  --bucket "$MINCO_AWS_ARTIFACT_BUCKET" \
  --server-side-encryption-configuration \
  '{"Rules":[{"ApplyServerSideEncryptionByDefault":{"SSEAlgorithm":"AES256"},"BucketKeyEnabled":false}]}' \
  >/dev/null
aws_logged s3api put-bucket-lifecycle-configuration \
  "expire temporary artifacts and incomplete uploads after one day if cleanup is interrupted" \
  --bucket "$MINCO_AWS_ARTIFACT_BUCKET" \
  --lifecycle-configuration \
  "{\"Rules\":[{\"ID\":\"minco-bounded-cleanup\",\"Status\":\"Enabled\",\"Filter\":{\"Prefix\":\"$MINCO_AWS_RUN_ID/\"},\"Expiration\":{\"Days\":1},\"AbortIncompleteMultipartUpload\":{\"DaysAfterInitiation\":1}}]}" \
  >/dev/null

parameter_overrides=(
  "DatabaseUrlParameterName=$MINCO_DATABASE_URL_PARAMETER"
)
if [[ -n "${MINCO_DATABASE_KMS_KEY_ARN:-}" ]]; then
  parameter_overrides+=("DatabaseUrlKmsKeyArn=$MINCO_DATABASE_KMS_KEY_ARN")
fi
if [[ -n "${MINCO_LAMBDA_SUBNET_IDS:-}" ]]; then
  parameter_overrides+=(
    "LambdaSubnetIds=$MINCO_LAMBDA_SUBNET_IDS"
    "LambdaSecurityGroupIds=$MINCO_LAMBDA_SECURITY_GROUP_IDS"
  )
fi

sam_logged deploy \
  "upload exact verified release and create unexecuted change set for $MINCO_STACK_NAME" \
  deploy \
  --template-file "$template" \
  --stack-name "$MINCO_STACK_NAME" \
  --region "$AWS_REGION" \
  --capabilities CAPABILITY_IAM \
  --s3-bucket "$MINCO_AWS_ARTIFACT_BUCKET" \
  --s3-prefix "$MINCO_AWS_RUN_ID" \
  --parameter-overrides "${parameter_overrides[@]}" \
  --tags \
  "minco:managed=true" \
  "minco:purpose=bounded-smoke" \
  "minco:run-id=$MINCO_AWS_RUN_ID" \
  --on-failure DELETE \
  --no-execute-changeset \
  --no-fail-on-empty-changeset \
  --no-progressbar

change_set_id="$(
  # shellcheck disable=SC2016
  aws_logged cloudformation list-change-sets \
    "locate generated change set for $MINCO_STACK_NAME" \
    --stack-name "$MINCO_STACK_NAME" \
    --query 'reverse(sort_by(Summaries[?Status==`CREATE_COMPLETE`],&CreationTime))[0].ChangeSetId' \
    --output text
)"
[[ -n "$change_set_id" && "$change_set_id" != "None" ]] || {
  echo "SAM did not leave a reviewable CREATE_COMPLETE change set" >&2
  exit 1
}
aws_logged cloudformation describe-change-set \
  "retain reviewed change-set evidence for $MINCO_STACK_NAME" \
  --change-set-name "$change_set_id" \
  --stack-name "$MINCO_STACK_NAME" \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/change-set.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/change-set.json"

jq -e '
  .Status == "CREATE_COMPLETE"
  and (.Changes | length > 0)
  and ([.Changes[].ResourceChange.Action] | all(. == "Add"))
  and (
    [.Changes[].ResourceChange.ResourceType]
    | all(
        . == "AWS::ApiGatewayV2::Api"
        or . == "AWS::ApiGatewayV2::Stage"
        or . == "AWS::IAM::Role"
        or . == "AWS::Lambda::Function"
        or . == "AWS::Lambda::Permission"
        or . == "AWS::Logs::LogGroup"
      )
  )
' "$MINCO_AWS_EVIDENCE_DIR/change-set.json" >/dev/null || {
  echo "change set was not a create-only deployment of the bounded resource types" >&2
  exit 1
}

aws_logged cloudformation execute-change-set \
  "execute reviewed create-only change set for $MINCO_STACK_NAME" \
  --change-set-name "$change_set_id" \
  --stack-name "$MINCO_STACK_NAME" >/dev/null
if ! aws_logged cloudformation wait \
  "wait for stack $MINCO_STACK_NAME create completion" \
  stack-create-complete \
  --stack-name "$MINCO_STACK_NAME"; then
  aws_logged cloudformation describe-stack-events \
    "retain failure events for stack $MINCO_STACK_NAME before cleanup" \
    --stack-name "$MINCO_STACK_NAME" \
    --output json >"$MINCO_AWS_EVIDENCE_DIR/stack-failure-events.json" || true
  chmod 600 "$MINCO_AWS_EVIDENCE_DIR/stack-failure-events.json" 2>/dev/null || true
  echo "stack creation failed; retained CloudFormation events before cleanup" >&2
  exit 1
fi

aws_logged cloudformation describe-stacks \
  "retain deployed outputs and status for $MINCO_STACK_NAME" \
  --stack-name "$MINCO_STACK_NAME" \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/stack.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/stack.json"
write_evidence_value \
  "$MINCO_AWS_EVIDENCE_DIR/function-name.txt" \
  "$(jq -er '.Stacks[0].Outputs[] | select(.OutputKey=="ApiFunctionName").OutputValue' "$MINCO_AWS_EVIDENCE_DIR/stack.json")"
write_evidence_value \
  "$MINCO_AWS_EVIDENCE_DIR/api-url.txt" \
  "$(jq -er '.Stacks[0].Outputs[] | select(.OutputKey=="ApiUrl").OutputValue' "$MINCO_AWS_EVIDENCE_DIR/stack.json")"
aws_logged cloudformation list-stack-resources \
  "retain physical resource identifiers for independent cleanup verification" \
  --stack-name "$MINCO_STACK_NAME" \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/stack-resources.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/stack-resources.json"
write_evidence_value \
  "$MINCO_AWS_EVIDENCE_DIR/function-role-name.txt" \
  "$(jq -er '.StackResourceSummaries[] | select(.ResourceType=="AWS::IAM::Role").PhysicalResourceId' "$MINCO_AWS_EVIDENCE_DIR/stack-resources.json")"
write_evidence_value \
  "$MINCO_AWS_EVIDENCE_DIR/http-api-id.txt" \
  "$(jq -er '.StackResourceSummaries[] | select(.ResourceType=="AWS::ApiGatewayV2::Api").PhysicalResourceId' "$MINCO_AWS_EVIDENCE_DIR/stack-resources.json")"

printf 'Deployed reviewed release %s to %s\n' \
  "$(jq -r '.release_id' "$MINCO_RELEASE_MANIFEST")" \
  "$MINCO_STACK_NAME"
