#!/usr/bin/env bash
set -euo pipefail

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
repo_root="$(minco_repo_root)"
cd "$repo_root"

for command in aws cargo jq python3 shasum tee; do
  require_command "$command"
done

: "${AWS_REGION:=ap-southeast-2}"
: "${MINCO_ROOT_PROFILE:=default}"
: "${MINCO_AWS_RUN_ID:=$(date -u +%Y%m%dt%H%M%Sz)-adapters}"
export AWS_REGION MINCO_AWS_RUN_ID
initialize_cloud_journal

resource_nonce="$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
suffix="$(
  printf '%s:%s' "$MINCO_AWS_RUN_ID" "$resource_nonce" |
    shasum -a 256 |
    cut -c1-12
)"
unset resource_nonce
bucket="minco-adapters-$suffix"
queue_name="minco-adapters-$suffix"
pool_name="minco-adapters-$suffix"
user_name="MincoAdapterBootstrap-$suffix"
role_name="MincoAdapterSmoke-$suffix"
user_policy_name="MincoAdapterAssumeRole"
role_policy_name="MincoAdapterBoundary"
source_profile="minco-adapter-source-$suffix"
deploy_profile="minco-adapter-$suffix"
profile_config="$(mktemp /tmp/minco-adapter-config.XXXXXX)"
source_credentials="$(mktemp /tmp/minco-adapter-source.XXXXXX)"
role_credentials="$(mktemp /tmp/minco-adapter-role.XXXXXX)"
request_directory="$(mktemp -d /tmp/minco-adapter-bootstrap.XXXXXX)"
chmod 600 "$profile_config" "$source_credentials" "$role_credentials"
chmod 700 "$request_directory"
jq -n \
  --arg bucket "$bucket" \
  --arg queue "$queue_name" \
  --arg pool "$pool_name" \
  --arg user "$user_name" \
  --arg role "$role_name" \
  '{
    bucket: $bucket,
    queue: $queue,
    cognito_pool: $pool,
    bootstrap_user: $user,
    adapter_role: $role
  }' >"$MINCO_AWS_EVIDENCE_DIR/resources.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/resources.json"

bucket_owned=false
queue_owned=false
pool_owned=false
user_owned=false
role_owned=false
queue_url=""
user_pool_id=""
account_id=""
cleanup_started=false

root_aws() {
  (
    unset \
      AWS_ACCESS_KEY_ID \
      AWS_CONFIG_FILE \
      AWS_SECRET_ACCESS_KEY \
      AWS_SESSION_TOKEN \
      AWS_SHARED_CREDENTIALS_FILE
    AWS_PROFILE="$MINCO_ROOT_PROFILE" aws_logged "$@"
  )
}

deploy_aws() {
  (
    unset AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN AWS_SHARED_CREDENTIALS_FILE
    AWS_CONFIG_FILE="$profile_config" AWS_PROFILE="$deploy_profile" aws_logged "$@"
  )
}

source_aws() {
  (
    unset AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN AWS_SHARED_CREDENTIALS_FILE
    AWS_CONFIG_FILE="$profile_config" AWS_PROFILE="$source_profile" aws_logged "$@"
  )
}

cleanup() {
  local original_status="${1:-0}"
  local cleanup_failure=0
  cleanup_started=true

  if [[ "$pool_owned" == true ]]; then
    if [[ -z "$user_pool_id" ]]; then
      user_pool_id="$(
        root_aws cognito-idp list-user-pools \
          "discover response-lost run-owned user pool by exact deterministic name" \
          --max-results 60 \
          --query "UserPools[?Name=='$pool_name'].Id | [0]" \
          --output text 2>/dev/null || true
      )"
      [[ "$user_pool_id" != "None" ]] || user_pool_id=""
    fi
    if [[ -n "$user_pool_id" ]]; then
      if ! root_aws cognito-idp delete-user-pool \
        "delete exact run-owned Cognito pool" \
        --user-pool-id "$user_pool_id" >/dev/null 2>&1; then
        cleanup_failure=1
      fi
    fi
  fi

  if [[ "$queue_owned" == true ]]; then
    if [[ -z "$queue_url" ]]; then
      queue_url="$(
        root_aws sqs get-queue-url \
          "discover response-lost run-owned queue by exact deterministic name" \
          --queue-name "$queue_name" \
          --query QueueUrl \
          --output text 2>/dev/null || true
      )"
      [[ "$queue_url" != "None" ]] || queue_url=""
    fi
    if [[ -n "$queue_url" ]] &&
      ! root_aws sqs delete-queue \
        "delete exact run-owned adapter queue" \
        --queue-url "$queue_url" >/dev/null 2>&1; then
      cleanup_failure=1
    fi
  fi

  if [[ "$bucket_owned" == true ]]; then
    if ! root_aws s3 rm \
      "remove all objects from exact run-owned adapter bucket" \
      "s3://$bucket" --recursive >/dev/null 2>&1; then
      cleanup_failure=1
    fi
    if ! root_aws s3api delete-bucket \
      "delete exact run-owned adapter bucket" \
      --bucket "$bucket" >/dev/null 2>&1; then
      cleanup_failure=1
    fi
  fi

  if [[ "$user_owned" == true ]]; then
    key_ids="$(
      root_aws iam list-access-keys \
        "list temporary bootstrap-user keys before teardown" \
        --user-name "$user_name" \
        --query 'AccessKeyMetadata[].AccessKeyId' \
        --output text 2>/dev/null || true
    )"
    for key_id in $key_ids; do
      if ! root_aws iam delete-access-key \
        "delete one temporary bootstrap-user access key" \
        --user-name "$user_name" \
        --access-key-id "$key_id" >/dev/null 2>&1; then
        cleanup_failure=1
      fi
    done
    root_aws iam delete-user-policy \
      "remove exact-role assumption policy from temporary user" \
      --user-name "$user_name" \
      --policy-name "$user_policy_name" >/dev/null 2>&1 || true
    if ! root_aws iam delete-user \
      "delete temporary non-root bootstrap user" \
      --user-name "$user_name" >/dev/null 2>&1; then
      cleanup_failure=1
    fi
  fi

  if [[ "$role_owned" == true ]]; then
    root_aws iam delete-role-policy \
      "remove generated adapter policy from temporary role" \
      --role-name "$role_name" \
      --policy-name "$role_policy_name" >/dev/null 2>&1 || true
    if ! root_aws iam delete-role \
      "delete temporary adapter role" \
      --role-name "$role_name" >/dev/null 2>&1; then
      cleanup_failure=1
    fi
  fi

  bucket_absent=false
  queue_absent=false
  pool_absent=false
  user_absent=false
  role_absent=false
  verify_bucket_error="$MINCO_AWS_EVIDENCE_DIR/cleanup-bucket-verify-error.txt"
  for attempt in {1..20}; do
    if ! root_aws s3api head-bucket \
      "verify run-owned bucket is absent; attempt $attempt" \
      --bucket "$bucket" >/dev/null 2>"$verify_bucket_error"; then
      if grep -Eqi '404|NoSuchBucket|Not Found' "$verify_bucket_error"; then
        bucket_absent=true
        break
      fi
      cleanup_failure=1
      break
    fi
    sleep 1
  done
  rm -f "$verify_bucket_error"
  verify_queue_error="$MINCO_AWS_EVIDENCE_DIR/cleanup-queue-verify-error.txt"
  for attempt in {1..20}; do
    if ! root_aws sqs get-queue-url \
      "verify run-owned queue is absent; attempt $attempt" \
      --queue-name "$queue_name" >/dev/null 2>"$verify_queue_error"; then
      if grep -Eqi 'NonExistentQueue|QueueDoesNotExist' "$verify_queue_error"; then
        queue_absent=true
        break
      fi
      cleanup_failure=1
      break
    fi
    sleep 1
  done
  rm -f "$verify_queue_error"
  pool_count="$(
    root_aws cognito-idp list-user-pools \
      "verify run-owned Cognito pool is absent" \
      --max-results 60 \
      --query "length(UserPools[?Name=='$pool_name'])" \
      --output text 2>/dev/null || printf 'unknown'
  )"
  [[ "$pool_count" == "0" ]] && pool_absent=true
  verify_user_error="$MINCO_AWS_EVIDENCE_DIR/cleanup-user-verify-error.txt"
  if ! root_aws iam get-user \
    "verify temporary bootstrap user is absent" \
    --user-name "$user_name" >/dev/null 2>"$verify_user_error"; then
    if grep -Eqi 'NoSuchEntity|cannot be found' "$verify_user_error"; then
      user_absent=true
    else
      cleanup_failure=1
    fi
  fi
  rm -f "$verify_user_error"
  verify_role_error="$MINCO_AWS_EVIDENCE_DIR/cleanup-role-verify-error.txt"
  if ! root_aws iam get-role \
    "verify temporary adapter role is absent" \
    --role-name "$role_name" >/dev/null 2>"$verify_role_error"; then
    if grep -Eqi 'NoSuchEntity|cannot be found' "$verify_role_error"; then
      role_absent=true
    else
      cleanup_failure=1
    fi
  fi
  rm -f "$verify_role_error"

  rm -f \
    "$profile_config" \
    "$source_credentials" \
    "$role_credentials" \
    "$request_directory/access-key.json" \
    "$request_directory/assume-role.json" \
    "$request_directory/bucket-tags.json" \
    "$request_directory/trust.json" \
    "$request_directory/user-policy.json"
  rmdir "$request_directory" >/dev/null 2>&1 || true
  local_credentials_absent=false
  [[ ! -e "$profile_config" && ! -e "$source_credentials" && ! -e "$role_credentials" ]] &&
    local_credentials_absent=true

  jq -n \
    --argjson bucket_absent "$bucket_absent" \
    --argjson queue_absent "$queue_absent" \
    --argjson pool_absent "$pool_absent" \
    --argjson user_absent "$user_absent" \
    --argjson role_absent "$role_absent" \
    --argjson local_credentials_absent "$local_credentials_absent" \
    '{
      bucket_absent: $bucket_absent,
      queue_absent: $queue_absent,
      cognito_pool_absent: $pool_absent,
      bootstrap_user_absent: $user_absent,
      adapter_role_absent: $role_absent,
      local_credentials_absent: $local_credentials_absent
    }' >"$MINCO_AWS_EVIDENCE_DIR/cleanup.json"
  chmod 600 "$MINCO_AWS_EVIDENCE_DIR/cleanup.json"
  if ! jq -e '[.[]] | all' "$MINCO_AWS_EVIDENCE_DIR/cleanup.json" >/dev/null; then
    cleanup_failure=1
  fi
  if ((cleanup_failure != 0)); then
    return 1
  fi
  return "$original_status"
}

on_exit() {
  local status=$?
  trap - EXIT INT TERM
  if [[ "$cleanup_started" == false ]] && ! cleanup "$status"; then
    status=1
  fi
  exit "$status"
}
trap on_exit EXIT INT TERM

root_identity="$(
  root_aws sts get-caller-identity \
    "verify approved account-root bootstrap principal before mutation" \
    --query '{Account:Account,Arn:Arn,UserId:UserId}' \
    --output json
)"
account_id="$(jq -er '.Account' <<<"$root_identity")"
root_arn="$(jq -er '.Arn' <<<"$root_identity")"
[[ "$root_arn" == "arn:aws:iam::$account_id:root" ]] || {
  echo "MINCO_ROOT_PROFILE must resolve to the reviewed account root" >&2
  exit 1
}
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/root-caller-identity.json" "$root_identity"
unset root_identity

bucket_absence_error="$MINCO_AWS_EVIDENCE_DIR/preflight-bucket-error.txt"
if root_aws s3api head-bucket \
  "prove deterministic adapter bucket name is unused before ownership" \
  --bucket "$bucket" >/dev/null 2>"$bucket_absence_error"; then
  echo "adapter bucket already exists; refusing ownership" >&2
  exit 1
elif ! grep -Eqi '404|NoSuchBucket|Not Found' "$bucket_absence_error"; then
  sed -n '1,8p' "$bucket_absence_error" >&2
  echo "could not prove the adapter bucket name is unused" >&2
  exit 1
fi
rm -f "$bucket_absence_error"
queue_absence_error="$MINCO_AWS_EVIDENCE_DIR/preflight-queue-error.txt"
if root_aws sqs get-queue-url \
  "prove deterministic adapter queue name is unused before ownership" \
  --queue-name "$queue_name" >/dev/null 2>"$queue_absence_error"; then
  echo "adapter queue already exists; refusing ownership" >&2
  exit 1
elif ! grep -Eqi 'NonExistentQueue|QueueDoesNotExist' "$queue_absence_error"; then
  sed -n '1,8p' "$queue_absence_error" >&2
  echo "could not prove the adapter queue name is unused" >&2
  exit 1
fi
rm -f "$queue_absence_error"
pool_count="$(
  root_aws cognito-idp list-user-pools \
    "prove deterministic Cognito pool name is unused before ownership" \
    --max-results 60 \
    --query "length(UserPools[?Name=='$pool_name'])" \
    --output text
)"
[[ "$pool_count" == "0" ]] || {
  echo "adapter Cognito pool already exists; refusing ownership" >&2
  exit 1
}
user_absence_error="$MINCO_AWS_EVIDENCE_DIR/preflight-user-error.txt"
if root_aws iam get-user \
  "prove deterministic bootstrap user name is unused before ownership" \
  --user-name "$user_name" >/dev/null 2>"$user_absence_error"; then
  echo "adapter bootstrap user already exists; refusing ownership" >&2
  exit 1
elif ! grep -Eqi 'NoSuchEntity|cannot be found' "$user_absence_error"; then
  sed -n '1,8p' "$user_absence_error" >&2
  echo "could not prove the adapter bootstrap user name is unused" >&2
  exit 1
fi
rm -f "$user_absence_error"
role_absence_error="$MINCO_AWS_EVIDENCE_DIR/preflight-role-error.txt"
if root_aws iam get-role \
  "prove deterministic adapter role name is unused before ownership" \
  --role-name "$role_name" >/dev/null 2>"$role_absence_error"; then
  echo "adapter role already exists; refusing ownership" >&2
  exit 1
elif ! grep -Eqi 'NoSuchEntity|cannot be found' "$role_absence_error"; then
  sed -n '1,8p' "$role_absence_error" >&2
  echo "could not prove the adapter role name is unused" >&2
  exit 1
fi
rm -f "$role_absence_error"

if [[ "$AWS_REGION" == "us-east-1" ]]; then
  root_aws s3api create-bucket \
    "create exact run-owned adapter bucket" \
    --bucket "$bucket" >/dev/null
else
  root_aws s3api create-bucket \
    "create exact run-owned adapter bucket" \
    --bucket "$bucket" \
    --create-bucket-configuration "LocationConstraint=$AWS_REGION" >/dev/null
fi
bucket_owned=true
jq -n \
  --arg run_id "$MINCO_AWS_RUN_ID" \
  '{TagSet: [
    {Key: "minco:managed", Value: "true"},
    {Key: "minco:purpose", Value: "bounded-adapter-smoke"},
    {Key: "minco:run-id", Value: $run_id}
  ]}' >"$request_directory/bucket-tags.json"
root_aws s3api put-bucket-tagging \
  "tag exact run-owned adapter bucket for cleanup ownership" \
  --bucket "$bucket" \
  --tagging "file://$request_directory/bucket-tags.json"

queue_tags="$(
  jq -cn \
    --arg run_id "$MINCO_AWS_RUN_ID" \
    '{
      "minco:managed": "true",
      "minco:purpose": "bounded-adapter-smoke",
      "minco:run-id": $run_id
    }'
)"
queue_url="$(
  root_aws sqs create-queue \
    "create exact tagged run-owned adapter queue" \
    --queue-name "$queue_name" \
    --tags "$queue_tags" \
    --query QueueUrl \
    --output text
)"
queue_owned=true
unset queue_tags
queue_arn="arn:aws:sqs:$AWS_REGION:$account_id:$queue_name"

pool_tags="$(
  jq -cn \
    --arg run_id "$MINCO_AWS_RUN_ID" \
    '{
      "minco:managed": "true",
      "minco:purpose": "bounded-adapter-smoke",
      "minco:run-id": $run_id
    }'
)"
user_pool_id="$(
  root_aws cognito-idp create-user-pool \
    "create exact tagged run-owned Cognito pool" \
    --pool-name "$pool_name" \
    --user-pool-tags "$pool_tags" \
    --query UserPool.Id \
    --output text
)"
pool_owned=true
unset pool_tags
user_pool_arn="arn:aws:cognito-idp:$AWS_REGION:$account_id:userpool/$user_pool_id"

ses_identities="$(
  root_aws sesv2 list-email-identities \
    "discover a pre-existing verified SES sender without mutation" \
    --output json
)"
ses_sender="$(
  jq -r '
    [.EmailIdentities[]
     | select(.VerifiedForSendingStatus == true)
     | select(.IdentityName | contains("@"))
     | .IdentityName][0] // empty
  ' <<<"$ses_identities"
)"
unset ses_identities
ses_identity_arn=""
if [[ -n "$ses_sender" ]]; then
  ses_identity_arn="arn:aws:ses:$AWS_REGION:$account_id:identity/$ses_sender"
else
  ses_sender=""
fi

export \
  MINCO_AWS_BUCKET_ARN="arn:aws:s3:::$bucket" \
  MINCO_AWS_QUEUE_ARN="$queue_arn" \
  MINCO_AWS_USER_POOL_ARN="$user_pool_arn" \
  MINCO_AWS_CLOUDFRONT_DISTRIBUTION_ARN="arn:aws:cloudfront::$account_id:distribution/E${suffix^^}" \
  MINCO_AWS_POLICY_PATH="$MINCO_AWS_EVIDENCE_DIR/runtime-policy.json" \
  MINCO_AWS_TEMPLATE_PATH="$MINCO_AWS_EVIDENCE_DIR/static-site-template.json" \
  MINCO_AWS_SES_IDENTITY_ARN="$ses_identity_arn"
cargo run -p minco-aws-adapters --all-features --example render_smoke_assets --quiet
chmod 600 "$MINCO_AWS_POLICY_PATH" "$MINCO_AWS_TEMPLATE_PATH"

root_aws accessanalyzer validate-policy \
  "validate generated exact-resource adapter role policy" \
  --policy-document "file://$MINCO_AWS_POLICY_PATH" \
  --policy-type IDENTITY_POLICY \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/runtime-policy-validation.json"
jq -e '[.findings[] | select(.findingType == "ERROR")] | length == 0' \
  "$MINCO_AWS_EVIDENCE_DIR/runtime-policy-validation.json" >/dev/null
root_aws cloudformation validate-template \
  "validate generated private S3 and CloudFront OAC template without deployment" \
  --template-body "file://$MINCO_AWS_TEMPLATE_PATH" \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/static-site-template-validation.json"

bootstrap_user_arn="arn:aws:iam::$account_id:user/$user_name"
role_arn="arn:aws:iam::$account_id:role/$role_name"
jq -n \
  --arg user "$bootstrap_user_arn" \
  '{Version: "2012-10-17", Statement: [{
    Effect: "Allow", Principal: {AWS: $user}, Action: "sts:AssumeRole"
  }]}' >"$request_directory/trust.json"
jq -n \
  --arg role "$role_arn" \
  '{Version: "2012-10-17", Statement: [{
    Effect: "Allow", Action: "sts:AssumeRole", Resource: $role
  }]}' >"$request_directory/user-policy.json"

root_aws iam create-user \
  "create temporary non-root adapter bootstrap user" \
  --user-name "$user_name" \
  --tags \
  Key=minco:managed,Value=true \
  Key=minco:purpose,Value=bounded-adapter-smoke \
  Key=minco:run-id,Value="$MINCO_AWS_RUN_ID" >/dev/null
user_owned=true
for attempt in {1..15}; do
  if root_aws iam create-role \
    "create temporary adapter role trusted only by exact bootstrap user; attempt $attempt" \
    --role-name "$role_name" \
    --assume-role-policy-document "file://$request_directory/trust.json" \
    --max-session-duration 3600 \
    --tags \
    Key=minco:managed,Value=true \
    Key=minco:purpose,Value=bounded-adapter-smoke \
    Key=minco:run-id,Value="$MINCO_AWS_RUN_ID" >/dev/null 2>&1; then
    break
  fi
  [[ "$attempt" != "15" ]] || {
    echo "temporary adapter role did not become creatable" >&2
    exit 1
  }
  sleep 2
done
role_owned=true
root_aws iam put-role-policy \
  "attach generated exact-resource adapter permissions" \
  --role-name "$role_name" \
  --policy-name "$role_policy_name" \
  --policy-document "file://$MINCO_AWS_POLICY_PATH"
root_aws iam put-user-policy \
  "allow bootstrap user to assume only the exact adapter role" \
  --user-name "$user_name" \
  --policy-name "$user_policy_name" \
  --policy-document "file://$request_directory/user-policy.json"

root_aws iam create-access-key \
  "create one temporary bootstrap-user key; secret never journaled" \
  --user-name "$user_name" \
  --query AccessKey \
  --output json >"$request_directory/access-key.json"
jq '{
  Version: 1,
  AccessKeyId: .AccessKeyId,
  SecretAccessKey: .SecretAccessKey
}' "$request_directory/access-key.json" >"$source_credentials"
rm -f "$request_directory/access-key.json"
printf '[profile %s]\nregion = %s\ncredential_process = /bin/cat %s\n' \
  "$source_profile" "$AWS_REGION" "$source_credentials" >"$profile_config"

assume_role_error="$MINCO_AWS_EVIDENCE_DIR/assume-role-error.txt"
assumed=false
for attempt in {1..15}; do
  if source_aws sts assume-role \
    "issue one-hour exact-role session; attempt $attempt; credentials never journaled" \
    --role-arn "$role_arn" \
    --role-session-name "minco-$suffix" \
    --duration-seconds 3600 \
    --query Credentials \
    --output json >"$request_directory/assume-role.json" 2>"$assume_role_error"; then
    assumed=true
    break
  fi
  if ! grep -Eqi 'AccessDenied|not authorized|InvalidClientTokenId|security token' \
    "$assume_role_error"; then
    sed -n '1,8p' "$assume_role_error" >&2
    exit 1
  fi
  sleep 2
done
[[ "$assumed" == true ]] || {
  sed -n '1,8p' "$assume_role_error" >&2
  echo "temporary bootstrap user could not assume the exact adapter role" >&2
  exit 1
}
rm -f "$assume_role_error"
jq '{
  Version: 1,
  AccessKeyId: .AccessKeyId,
  SecretAccessKey: .SecretAccessKey,
  SessionToken: .SessionToken,
  Expiration: .Expiration
}' "$request_directory/assume-role.json" >"$role_credentials"
rm -f "$request_directory/assume-role.json"
printf '\n[profile %s]\nregion = %s\ncredential_process = /bin/cat %s\n' \
  "$deploy_profile" "$AWS_REGION" "$role_credentials" >>"$profile_config"

deploy_identity="$(
  deploy_aws sts get-caller-identity \
    "verify isolated non-root adapter role profile before provider calls" \
    --query '{Account:Account,Arn:Arn,UserId:UserId}' \
    --output json
)"
jq -e \
  --arg account "$account_id" \
  --arg role "$role_name" \
  '.Account == $account and (.Arn | contains(":assumed-role/" + $role + "/"))' \
  <<<"$deploy_identity" >/dev/null
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/deploy-caller-identity.json" "$deploy_identity"
unset deploy_identity

export \
  AWS_CONFIG_FILE="$profile_config" \
  AWS_PROFILE="$deploy_profile" \
  AWS_REGION \
  MINCO_AWS_BUCKET="$bucket" \
  MINCO_AWS_QUEUE_URL="$queue_url" \
  MINCO_AWS_USER_POOL_ID="$user_pool_id"
if [[ -n "$ses_sender" ]]; then
  export MINCO_AWS_SES_SENDER="$ses_sender"
else
  unset MINCO_AWS_SES_SENDER
fi
RUST_BACKTRACE=0 cargo test \
  -p minco-aws-adapters \
  --features s3 \
  --test real_aws_s3 \
  --locked \
  managed_uploads_conform_on_bounded_real_s3 \
  -- --ignored --exact --nocapture \
  2>&1 | tee "$MINCO_AWS_EVIDENCE_DIR/managed-s3-test.log"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/managed-s3-test.log"
RUST_BACKTRACE=0 cargo test \
  -p minco-aws-adapters \
  --all-features \
  --test real_aws \
  --locked \
  adapters_conform_on_bounded_real_aws \
  -- --ignored --exact --nocapture \
  2>&1 | tee "$MINCO_AWS_EVIDENCE_DIR/adapter-test.log"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/adapter-test.log"

message_body=""
for attempt in {1..10}; do
  message_body="$(
    root_aws sqs receive-message \
      "verify the non-root SQS adapter published one domain event; attempt $attempt" \
      --queue-url "$queue_url" \
      --max-number-of-messages 10 \
      --wait-time-seconds 1 \
      --query 'Messages[0].Body' \
      --output text
  )"
  [[ "$message_body" != "None" && -n "$message_body" ]] && break
done
[[ "$message_body" != "None" && -n "$message_body" ]] || {
  echo "the bounded SQS adapter message was not observable" >&2
  exit 1
}
jq -e --arg run_id "$MINCO_AWS_RUN_ID" \
  '.event_type == "feedback.created" and .aggregate_id == $run_id' \
  <<<"$message_body" >/dev/null

cleanup 0
trap - EXIT INT TERM
printf 'Bounded real-AWS adapter smoke and cleanup passed: %s\n' "$MINCO_AWS_RUN_ID"
