#!/usr/bin/env bash
set -euo pipefail

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
repo_root="$(minco_repo_root)"
cd "$repo_root"

for command in aws jq psql python3; do
  require_command "$command"
done
: "${AWS_REGION:=ap-southeast-2}"
: "${MINCO_STACK_NAME:?set MINCO_STACK_NAME}"
: "${MINCO_AWS_ARTIFACT_BUCKET:?set MINCO_AWS_ARTIFACT_BUCKET}"
: "${MINCO_DATABASE_URL_PARAMETER:?set MINCO_DATABASE_URL_PARAMETER}"
: "${MINCO_DATABASE_PARAMETER_OWNED:=false}"
: "${MINCO_DATABASE_INSTANCE_OWNED:=false}"
[[ "$MINCO_DATABASE_PARAMETER_OWNED" == "true" || "$MINCO_DATABASE_PARAMETER_OWNED" == "false" ]] || {
  echo "MINCO_DATABASE_PARAMETER_OWNED must equal true or false" >&2
  exit 1
}
[[ "$MINCO_DATABASE_INSTANCE_OWNED" == "true" || "$MINCO_DATABASE_INSTANCE_OWNED" == "false" ]] || {
  echo "MINCO_DATABASE_INSTANCE_OWNED must equal true or false" >&2
  exit 1
}
initialize_cloud_journal
unset MINCO_SMOKE_JWT_TOKEN

failure=0
database_cleanup_complete=true
database_cleanup_boundary_verified=true
set +e

if [[ -f "$MINCO_AWS_EVIDENCE_DIR/order-id.txt" &&
  "$MINCO_DATABASE_INSTANCE_OWNED" == "true" ]]; then
  write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/database-cleanup-delegated.txt" true
elif [[ -f "$MINCO_AWS_EVIDENCE_DIR/order-id.txt" &&
  ! -f "$MINCO_AWS_EVIDENCE_DIR/database-cleanup-complete.txt" ]]; then
  database_cleanup_complete=false
  database_cleanup_boundary_verified=false
  order_id="$(<"$MINCO_AWS_EVIDENCE_DIR/order-id.txt")"
  idempotency_key="$(<"$MINCO_AWS_EVIDENCE_DIR/idempotency-key.txt")"
  customer_reference="$(<"$MINCO_AWS_EVIDENCE_DIR/customer-reference.txt")"
  database_url="$(
    aws_logged ssm get-parameter \
      "read existing database parameter for synthetic row cleanup; value redacted" \
      --name "$MINCO_DATABASE_URL_PARAMETER" \
      --with-decryption \
      --query Parameter.Value \
      --output text
  )"
  if [[ $? -ne 0 || -z "$database_url" || "$database_url" == "None" ]]; then
    echo "could not retrieve the database URL for synthetic row cleanup" >&2
    failure=1
  else
    if ! record_external_database_touch \
      "delete synthetic order" \
      "delete only order $order_id and idempotency key $idempotency_key"; then
      echo "could not journal the synthetic PostgreSQL deletion" >&2
      failure=1
    elif ! PGCONNECT_TIMEOUT=10 psql_with_url "$database_url" \
      --no-psqlrc \
      --quiet \
      --set ON_ERROR_STOP=1 \
      --set "order_id=$order_id" \
      --set "idempotency_key=$idempotency_key" \
      --set "customer_reference=$customer_reference" >/dev/null <<'SQL'
BEGIN;
DELETE FROM order_idempotency
WHERE idempotency_key = :'idempotency_key'
  AND order_id = :'order_id'::uuid;
DELETE FROM orders
WHERE id = :'order_id'::uuid
  AND customer_reference = :'customer_reference';
COMMIT;
SQL
    then
      echo "synthetic PostgreSQL row deletion failed" >&2
      failure=1
    fi
    if ! record_external_database_touch \
      "verify synthetic order cleanup" \
      "confirm order $order_id no longer exists"; then
      echo "could not journal the synthetic PostgreSQL verification" >&2
      failure=1
    elif ! cleanup_count="$(
      PGCONNECT_TIMEOUT=10 psql_with_url "$database_url" \
        --no-psqlrc \
        --quiet \
        --tuples-only \
        --no-align \
        --set ON_ERROR_STOP=1 \
        --set "order_id=$order_id" \
        --command "SELECT count(*) FROM orders WHERE id = :'order_id'::uuid"
    )"; then
      echo "synthetic PostgreSQL row cleanup verification query failed" >&2
      failure=1
    elif [[ "${cleanup_count//[[:space:]]/}" != "0" ]]; then
      echo "synthetic PostgreSQL row cleanup could not be verified" >&2
      failure=1
    else
      write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/database-cleanup-complete.txt" true
      database_cleanup_complete=true
      database_cleanup_boundary_verified=true
    fi
  fi
  unset database_url
fi

stack_description_error="$MINCO_AWS_EVIDENCE_DIR/stack-describe-error.txt"
if aws_logged cloudformation describe-stacks \
  "check whether bounded stack $MINCO_STACK_NAME requires cleanup" \
  --stack-name "$MINCO_STACK_NAME" \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/stack-before-cleanup.json" \
  2>"$stack_description_error"; then
  stack_cleanup_authorized=false
  stack_cleanup_detail=
  if jq -e \
    --arg stack "$MINCO_STACK_NAME" \
    --arg run_id "$MINCO_AWS_RUN_ID" \
    '.Stacks[0].StackName == $stack
     and (
       .Stacks[0].Tags
       | from_entries
       | .["minco:managed"] == "true"
         and .["minco:purpose"] == "bounded-smoke"
         and .["minco:run-id"] == $run_id
     )' \
    "$MINCO_AWS_EVIDENCE_DIR/stack-before-cleanup.json" >/dev/null; then
    stack_cleanup_authorized=true
    stack_cleanup_detail="delete exact tagged bounded stack $MINCO_STACK_NAME"
  elif [[ -f "$MINCO_AWS_EVIDENCE_DIR/stack-preflight-absent.txt" &&
    "$(<"$MINCO_AWS_EVIDENCE_DIR/stack-preflight-absent.txt")" == "$MINCO_STACK_NAME" ]] &&
    jq -e \
      --arg stack "$MINCO_STACK_NAME" \
      '(.Stacks | type == "array" and length == 1)
       and .Stacks[0].StackName == $stack
       and .Stacks[0].StackStatus == "REVIEW_IN_PROGRESS"
       and ((.Stacks[0].Tags // null) | type == "array" and length == 0)' \
      "$MINCO_AWS_EVIDENCE_DIR/stack-before-cleanup.json" >/dev/null; then
    stack_resources_path="$MINCO_AWS_EVIDENCE_DIR/stack-resources-before-cleanup.json"
    if aws_logged cloudformation list-stack-resources \
      "prove the run-created review stack has no resources before cleanup" \
      --stack-name "$MINCO_STACK_NAME" \
      --output json >"$stack_resources_path" &&
      bounded_review_stack_cleanup_is_authorized \
        "$MINCO_AWS_EVIDENCE_DIR/stack-before-cleanup.json" \
        "$stack_resources_path" \
        "$MINCO_AWS_EVIDENCE_DIR/stack-preflight-absent.txt" \
        "$MINCO_STACK_NAME"; then
      stack_cleanup_authorized=true
      stack_cleanup_detail="delete exact empty run-created review stack $MINCO_STACK_NAME"
    fi
  fi

  if [[ "$stack_cleanup_authorized" == true ]]; then
    aws_logged cloudformation delete-stack \
      "$stack_cleanup_detail" \
      --stack-name "$MINCO_STACK_NAME"
    if ! aws_logged cloudformation wait \
      "wait for bounded stack $MINCO_STACK_NAME deletion" \
      stack-delete-complete \
      --stack-name "$MINCO_STACK_NAME"; then
      failure=1
    fi
  else
    echo "refusing to delete a stack without exact run ownership evidence" >&2
    failure=1
  fi
elif ! grep -Eq 'does not exist' "$stack_description_error"; then
  echo "could not determine bounded stack state" >&2
  sed -n '1,8p' "$stack_description_error" >&2
  failure=1
fi
rm -f "$stack_description_error"

pool_name="minco-smoke-$MINCO_AWS_RUN_ID"
candidate_pool_ids=()
pool_discovery_verified=true
if [[ -f "$MINCO_AWS_EVIDENCE_DIR/cognito-pool-id.txt" ]]; then
  candidate_pool_ids+=("$(<"$MINCO_AWS_EVIDENCE_DIR/cognito-pool-id.txt")")
else
  pool_list_error="$MINCO_AWS_EVIDENCE_DIR/cognito-list-error.txt"
  if pool_listing="$(
    aws_logged cognito-idp list-user-pools \
      "discover a response-lost temporary user pool by exact run-owned name" \
      --max-results 60 \
      --output json 2>"$pool_list_error"
  )"; then
    while IFS= read -r pool_id; do
      [[ -n "$pool_id" ]] && candidate_pool_ids+=("$pool_id")
    done < <(
      jq -r \
        --arg name "$pool_name" \
        '.UserPools[]? | select(.Name == $name) | .Id' \
        <<<"$pool_listing"
    )
  else
    echo "temporary Cognito user pool discovery failed" >&2
    sed -n '1,8p' "$pool_list_error" >&2
    pool_discovery_verified=false
    failure=1
  fi
  rm -f "$pool_list_error"
fi
pool_ids=()
account_id=""
if ((${#candidate_pool_ids[@]} > 0)); then
  if [[ -f "$MINCO_AWS_EVIDENCE_DIR/caller-identity.json" ]]; then
    account_id="$(jq -er '.Account' "$MINCO_AWS_EVIDENCE_DIR/caller-identity.json")"
  else
    account_id="$(
      aws_logged sts get-caller-identity \
        "resolve account for exact temporary Cognito ownership verification" \
        --query Account \
        --output text
    )"
  fi
  if ! [[ "$account_id" =~ ^[0-9]{12}$ ]]; then
    echo "could not resolve the account for temporary Cognito ownership verification" >&2
    pool_discovery_verified=false
    failure=1
  fi
fi
for pool_id in "${candidate_pool_ids[@]}"; do
  pool_tag_error="$MINCO_AWS_EVIDENCE_DIR/cognito-tag-check-error.txt"
  if pool_tags="$(
    aws_logged cognito-idp list-tags-for-resource \
      "prove temporary user pool $pool_id has the exact run ownership tags" \
      --resource-arn "arn:aws:cognito-idp:$AWS_REGION:$account_id:userpool/$pool_id" \
      --output json 2>"$pool_tag_error"
  )"; then
    if jq -e \
      --arg run_id "$MINCO_AWS_RUN_ID" \
      '.Tags["minco:managed"] == "true"
       and .Tags["minco:purpose"] == "bounded-smoke"
       and .Tags["minco:run-id"] == $run_id' \
      <<<"$pool_tags" >/dev/null; then
      pool_ids+=("$pool_id")
    else
      echo "refusing to delete a user pool without exact run ownership tags" >&2
      pool_discovery_verified=false
      failure=1
    fi
  elif ! grep -Eq 'ResourceNotFoundException|User pool .* does not exist' "$pool_tag_error"; then
    echo "could not verify temporary Cognito user pool ownership" >&2
    sed -n '1,8p' "$pool_tag_error" >&2
    pool_discovery_verified=false
    failure=1
  fi
  rm -f "$pool_tag_error"
done

for pool_id in "${pool_ids[@]}"; do
  pool_error="$MINCO_AWS_EVIDENCE_DIR/cognito-delete-error.txt"
  if ! aws_logged cognito-idp delete-user-pool \
    "delete temporary smoke user pool $pool_id and its synthetic user/client" \
    --user-pool-id "$pool_id" 2>"$pool_error"; then
    if ! grep -Eq 'ResourceNotFoundException|User pool .* does not exist' "$pool_error"; then
      echo "temporary Cognito user pool cleanup failed" >&2
      sed -n '1,8p' "$pool_error" >&2
      failure=1
    fi
  fi
  rm -f "$pool_error"
done

bucket_error="$MINCO_AWS_EVIDENCE_DIR/bucket-head-error.txt"
if aws_logged s3api head-bucket \
  "check whether temporary artifact bucket requires cleanup" \
  --bucket "$MINCO_AWS_ARTIFACT_BUCKET" 2>"$bucket_error"; then
  bucket_cleanup_authorized=false
  bucket_tag_error="$MINCO_AWS_EVIDENCE_DIR/bucket-tag-check-error.txt"
  if aws_logged s3api get-bucket-tagging \
    "prove temporary artifact bucket has the exact run ownership tags" \
    --bucket "$MINCO_AWS_ARTIFACT_BUCKET" \
    --output json >"$MINCO_AWS_EVIDENCE_DIR/bucket-tags-before-cleanup.json" \
    2>"$bucket_tag_error"; then
    if jq -e \
      --arg run_id "$MINCO_AWS_RUN_ID" \
      '.TagSet
       | from_entries
       | .["minco:managed"] == "true"
         and .["minco:purpose"] == "bounded-smoke"
         and .["minco:run-id"] == $run_id' \
      "$MINCO_AWS_EVIDENCE_DIR/bucket-tags-before-cleanup.json" >/dev/null; then
      bucket_cleanup_authorized=true
    fi
  fi
  rm -f "$bucket_tag_error"

  if [[ "$bucket_cleanup_authorized" == true ]]; then
    aws_logged s3 rm \
      "remove every temporary SAM artifact from exact owned bucket $MINCO_AWS_ARTIFACT_BUCKET" \
      "s3://$MINCO_AWS_ARTIFACT_BUCKET" --recursive
    if ! aws_logged s3api delete-bucket \
      "delete exact owned temporary artifact bucket $MINCO_AWS_ARTIFACT_BUCKET" \
      --bucket "$MINCO_AWS_ARTIFACT_BUCKET"; then
      failure=1
    fi
  else
    echo "refusing to empty or delete a bucket without exact run ownership proof" >&2
    failure=1
  fi
elif ! grep -Eq '404|NoSuchBucket|Not Found' "$bucket_error"; then
  echo "could not determine temporary artifact bucket state" >&2
  sed -n '1,8p' "$bucket_error" >&2
  failure=1
fi
rm -f "$bucket_error"

if [[ "$MINCO_DATABASE_PARAMETER_OWNED" == "true" &&
  "$database_cleanup_complete" == "true" ]]; then
  parameter_cleanup_authorized=false
  parameter_exists=false
  parameter_tag_error="$MINCO_AWS_EVIDENCE_DIR/parameter-tag-check-error.txt"
  if parameter_count="$(
    aws_logged ssm describe-parameters \
      "check exact run-owned database parameter metadata before deletion; no value requested" \
      --parameter-filters "Key=Name,Option=Equals,Values=$MINCO_DATABASE_URL_PARAMETER" \
      --query 'length(Parameters)' \
      --output text 2>"$parameter_tag_error"
  )"; then
    if [[ "$parameter_count" == "0" ]]; then
      parameter_cleanup_authorized=true
    elif [[ "$parameter_count" == "1" ]]; then
      parameter_exists=true
      if parameter_tags="$(
        aws_logged ssm list-tags-for-resource \
          "prove temporary database parameter has the exact run ownership tags" \
          --resource-type Parameter \
          --resource-id "$MINCO_DATABASE_URL_PARAMETER" \
          --output json 2>>"$parameter_tag_error"
      )" &&
        jq -e \
          --arg run_id "$MINCO_AWS_RUN_ID" \
          '.TagList
           | from_entries
           | .["minco:managed"] == "true"
             and .["minco:purpose"] == "bounded-smoke"
             and .["minco:run-id"] == $run_id' \
          <<<"$parameter_tags" >/dev/null; then
        parameter_cleanup_authorized=true
      fi
    fi
  fi
  if [[ "$parameter_cleanup_authorized" == true && "$parameter_exists" == true ]]; then
    parameter_delete_error="$MINCO_AWS_EVIDENCE_DIR/parameter-delete-error.txt"
    if ! aws_logged ssm delete-parameter \
      "delete exact tagged run-owned database SecureString; value never requested" \
      --name "$MINCO_DATABASE_URL_PARAMETER" 2>"$parameter_delete_error" &&
      ! grep -Eq 'ParameterNotFound' "$parameter_delete_error"; then
      echo "temporary database parameter cleanup failed" >&2
      sed -n '1,8p' "$parameter_delete_error" >&2
      failure=1
    fi
    rm -f "$parameter_delete_error"
  elif [[ "$parameter_cleanup_authorized" != true ]]; then
    echo "refusing to delete a database parameter without exact run ownership tags" >&2
    sed -n '1,8p' "$parameter_tag_error" >&2
    failure=1
  fi
  rm -f "$parameter_tag_error"
elif [[ "$MINCO_DATABASE_PARAMETER_OWNED" == "true" ]]; then
  echo "retaining temporary database parameter because synthetic row cleanup is incomplete" >&2
  failure=1
fi

stack_absent=false
stack_verify_error="$MINCO_AWS_EVIDENCE_DIR/stack-verify-error.txt"
if ! aws_logged cloudformation describe-stacks \
  "verify bounded stack $MINCO_STACK_NAME is absent" \
  --stack-name "$MINCO_STACK_NAME" >/dev/null 2>"$stack_verify_error" &&
  grep -Eq 'does not exist' "$stack_verify_error"; then
  stack_absent=true
fi
rm -f "$stack_verify_error"

bucket_absent=false
bucket_verify_error="$MINCO_AWS_EVIDENCE_DIR/bucket-verify-error.txt"
if ! aws_logged s3api head-bucket \
  "verify temporary artifact bucket is absent" \
  --bucket "$MINCO_AWS_ARTIFACT_BUCKET" 2>"$bucket_verify_error" &&
  grep -Eq '404|NoSuchBucket|Not Found' "$bucket_verify_error"; then
  bucket_absent=true
fi
rm -f "$bucket_verify_error"

pool_absent="$pool_discovery_verified"
for pool_id in "${pool_ids[@]}"; do
  pool_verify_error="$MINCO_AWS_EVIDENCE_DIR/pool-verify-error.txt"
  if ! aws_logged cognito-idp describe-user-pool \
    "verify temporary smoke user pool is absent" \
    --user-pool-id "$pool_id" >/dev/null 2>"$pool_verify_error" &&
    grep -Eq 'ResourceNotFoundException|User pool .* does not exist' "$pool_verify_error"; then
    :
  else
    pool_absent=false
  fi
  rm -f "$pool_verify_error"
done

function_absent=true
log_group_absent=true
if [[ -f "$MINCO_AWS_EVIDENCE_DIR/function-name.txt" ]]; then
  function_name="$(<"$MINCO_AWS_EVIDENCE_DIR/function-name.txt")"
  function_absent=false
  function_verify_error="$MINCO_AWS_EVIDENCE_DIR/function-verify-error.txt"
  if ! aws_logged lambda get-function \
    "verify bounded Lambda function $function_name is absent" \
    --function-name "$function_name" >/dev/null 2>"$function_verify_error" &&
    grep -Eq 'ResourceNotFoundException|Function not found' "$function_verify_error"; then
    function_absent=true
  fi
  rm -f "$function_verify_error"
  log_group_absent=false
  log_group_count="$(
    aws_logged logs describe-log-groups \
      "verify bounded Lambda log group is absent" \
      --log-group-name-prefix "/aws/lambda/$function_name" \
      --query "length(logGroups[?logGroupName=='/aws/lambda/$function_name'])" \
      --output text
  )"
  if [[ "$log_group_count" == "0" ]]; then
    log_group_absent=true
  elif [[ "$function_absent" == true ]]; then
    log_group_delete_error="$MINCO_AWS_EVIDENCE_DIR/log-group-delete-error.txt"
    if ! aws_logged logs delete-log-group \
      "delete exact bounded Lambda log group recreated during VPC function teardown" \
      --log-group-name "/aws/lambda/$function_name" 2>"$log_group_delete_error" &&
      ! grep -Eq 'ResourceNotFoundException|does not exist' "$log_group_delete_error"; then
      echo "exact bounded Lambda log group cleanup failed" >&2
      sed -n '1,8p' "$log_group_delete_error" >&2
      failure=1
    fi
    rm -f "$log_group_delete_error"
    for attempt in {1..15}; do
      log_group_count="$(
        aws_logged logs describe-log-groups \
          "verify exact recreated Lambda log group is absent; attempt $attempt" \
          --log-group-name-prefix "/aws/lambda/$function_name" \
          --query "length(logGroups[?logGroupName=='/aws/lambda/$function_name'])" \
          --output text
      )"
      if [[ "$log_group_count" == "0" ]]; then
        log_group_absent=true
        break
      fi
      sleep 2
    done
  fi
fi

function_role_absent=true
if [[ -f "$MINCO_AWS_EVIDENCE_DIR/function-role-name.txt" ]]; then
  function_role_name="$(<"$MINCO_AWS_EVIDENCE_DIR/function-role-name.txt")"
  function_role_absent=false
  role_verify_error="$MINCO_AWS_EVIDENCE_DIR/role-verify-error.txt"
  if ! aws_logged iam get-role \
    "verify bounded Lambda execution role $function_role_name is absent" \
    --role-name "$function_role_name" >/dev/null 2>"$role_verify_error" &&
    grep -Eq 'NoSuchEntity|cannot be found' "$role_verify_error"; then
    function_role_absent=true
  fi
  rm -f "$role_verify_error"
fi

http_api_absent=true
if [[ -f "$MINCO_AWS_EVIDENCE_DIR/http-api-id.txt" ]]; then
  http_api_id="$(<"$MINCO_AWS_EVIDENCE_DIR/http-api-id.txt")"
  http_api_absent=false
  api_verify_error="$MINCO_AWS_EVIDENCE_DIR/api-verify-error.txt"
  if ! aws_logged apigatewayv2 get-api \
    "verify bounded HTTP API $http_api_id is absent" \
    --api-id "$http_api_id" >/dev/null 2>"$api_verify_error" &&
    grep -Eq 'NotFoundException|Not Found' "$api_verify_error"; then
    http_api_absent=true
  fi
  rm -f "$api_verify_error"
fi

parameter_cleanup_verified=false
if [[ "$MINCO_DATABASE_PARAMETER_OWNED" == "true" ]]; then
  for attempt in {1..15}; do
    parameter_count="$(
      aws_logged ssm describe-parameters \
        "verify run-owned temporary database parameter is absent without requesting its value; attempt $attempt" \
        --parameter-filters "Key=Name,Option=Equals,Values=$MINCO_DATABASE_URL_PARAMETER" \
        --query 'length(Parameters)' \
        --output text
    )"
    if [[ "$parameter_count" == "0" ]]; then
      parameter_cleanup_verified=true
      printf 'null\n' >"$MINCO_AWS_EVIDENCE_DIR/database-parameter-after.json"
      break
    fi
    sleep 2
  done
else
  aws_logged ssm describe-parameters \
    "capture external database parameter metadata after cleanup; no value requested" \
    --parameter-filters "Key=Name,Option=Equals,Values=$MINCO_DATABASE_URL_PARAMETER" \
    --query 'Parameters[0].{Name:Name,Type:Type,Tier:Tier,DataType:DataType,KeyId:KeyId,Version:Version,LastModifiedDate:LastModifiedDate}' \
    --output json >"$MINCO_AWS_EVIDENCE_DIR/database-parameter-after.json"
  if [[ -f "$MINCO_AWS_EVIDENCE_DIR/database-parameter-before.json" ]] &&
    cmp -s \
      "$MINCO_AWS_EVIDENCE_DIR/database-parameter-before.json" \
      "$MINCO_AWS_EVIDENCE_DIR/database-parameter-after.json"; then
    parameter_cleanup_verified=true
  fi
fi
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/database-parameter-after.json"

jq -n \
  --argjson stack_absent "$stack_absent" \
  --argjson bucket_absent "$bucket_absent" \
  --argjson pool_absent "$pool_absent" \
  --argjson function_absent "$function_absent" \
  --argjson function_role_absent "$function_role_absent" \
  --argjson http_api_absent "$http_api_absent" \
  --argjson log_group_absent "$log_group_absent" \
  --argjson database_cleanup_boundary_verified "$database_cleanup_boundary_verified" \
  --argjson parameter_cleanup_verified "$parameter_cleanup_verified" \
  '{
    stack_absent: $stack_absent,
    artifact_bucket_absent: $bucket_absent,
    cognito_pool_absent: $pool_absent,
    lambda_function_absent: $function_absent,
    lambda_execution_role_absent: $function_role_absent,
    http_api_absent: $http_api_absent,
    lambda_log_group_absent: $log_group_absent,
    synthetic_database_cleanup_boundary_verified: $database_cleanup_boundary_verified,
    database_parameter_cleanup_verified: $parameter_cleanup_verified
  }' >"$MINCO_AWS_EVIDENCE_DIR/cleanup.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/cleanup.json"

if ! jq -e '[.[]] | all' "$MINCO_AWS_EVIDENCE_DIR/cleanup.json" >/dev/null; then
  jq . "$MINCO_AWS_EVIDENCE_DIR/cleanup.json" >&2
  failure=1
fi
((failure == 0)) || exit 1
printf 'Verified AWS cleanup for run %s\n' "$MINCO_AWS_RUN_ID"
