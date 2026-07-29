#!/usr/bin/env bash
set -euo pipefail

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
repo_root="$(minco_repo_root)"
cd "$repo_root"

for command in aws jq psql python3 shasum stat; do
  require_command "$command"
done

: "${AWS_REGION:=ap-southeast-2}"
: "${MINCO_ROOT_PROFILE:=default}"
: "${MINCO_AWS_RUN_ID:=$(date -u +%Y%m%dt%H%M%Sz)-approved}"
: "${MINCO_CREATE_TEMP_RDS:=false}"
initialize_cloud_journal

[[ "$MINCO_CREATE_TEMP_RDS" == "true" || "$MINCO_CREATE_TEMP_RDS" == "false" ]] || {
  echo "MINCO_CREATE_TEMP_RDS must equal true or false" >&2
  exit 1
}
database_source_count=0
[[ -n "${MINCO_DATABASE_URL_SOURCE_PARAMETER:-}" ]] && ((database_source_count += 1))
[[ -n "${MINCO_DATABASE_URL_FILE:-}" ]] && ((database_source_count += 1))
[[ -n "${MINCO_DATABASE_URL:-}" ]] && ((database_source_count += 1))
[[ "$MINCO_CREATE_TEMP_RDS" == "true" ]] && ((database_source_count += 1))
((database_source_count == 1)) || {
  echo "set exactly one database source or MINCO_CREATE_TEMP_RDS=true" >&2
  exit 1
}
if [[ -n "${MINCO_DATABASE_URL_SOURCE_PARAMETER:-}" &&
  "$MINCO_DATABASE_URL_SOURCE_PARAMETER" != /minco/* ]]; then
  echo "MINCO_DATABASE_URL_SOURCE_PARAMETER must be a Minco-owned /minco/... parameter" >&2
  exit 1
fi
if [[ -n "${MINCO_DATABASE_URL_SOURCE_PARAMETER:-}" ]] &&
  ! normalized_ssm_parameter_name "$MINCO_DATABASE_URL_SOURCE_PARAMETER"; then
  echo "MINCO_DATABASE_URL_SOURCE_PARAMETER must be a normalized SSM parameter name" >&2
  exit 1
fi
if [[ -n "${MINCO_DATABASE_URL_FILE:-}" ]]; then
  [[ -f "$MINCO_DATABASE_URL_FILE" && ! -L "$MINCO_DATABASE_URL_FILE" ]] || {
    echo "MINCO_DATABASE_URL_FILE must be a regular non-symlink file" >&2
    exit 1
  }
  file_mode="$(stat -f '%Lp' "$MINCO_DATABASE_URL_FILE")"
  (((8#$file_mode & 8#077) == 0)) || {
    echo "MINCO_DATABASE_URL_FILE must not be group/world accessible" >&2
    exit 1
  }
fi

run_suffix="$(printf '%s' "$MINCO_AWS_RUN_ID" | shasum -a 256 | cut -c1-12)"
role_name="MincoSmoke-$run_suffix"
role_policy_name="MincoSmokeBoundary"
user_name="MincoSmokeBootstrap-$run_suffix"
user_policy_name="MincoSmokeAssumeRole"
source_profile="minco-smoke-source-$run_suffix"
deploy_profile="minco-smoke-$run_suffix"
profile_config="$(mktemp /tmp/minco-aws-config.XXXXXX)"
source_credentials="$(mktemp /tmp/minco-aws-user-credentials.XXXXXX)"
role_credentials="$(mktemp /tmp/minco-aws-role-credentials.XXXXXX)"
request_directory="$(mktemp -d /tmp/minco-aws-bootstrap.XXXXXX)"
chmod 600 "$profile_config"
chmod 600 "$source_credentials" "$role_credentials"
chmod 700 "$request_directory"

export MINCO_STACK_NAME="${MINCO_STACK_NAME:-minco-smoke-$run_suffix}"
export MINCO_AWS_ARTIFACT_BUCKET="${MINCO_AWS_ARTIFACT_BUCKET:-minco-smoke-$run_suffix}"
export MINCO_SMOKE_APPLICATION="${MINCO_SMOKE_APPLICATION:-minco-$run_suffix}"
export MINCO_DATABASE_URL_PARAMETER="${MINCO_DATABASE_URL_PARAMETER:-/minco/smoke/$run_suffix/database-url}"
export MINCO_RDS_STACK_NAME="${MINCO_RDS_STACK_NAME:-minco-rds-$run_suffix}"
export MINCO_RDS_INSTANCE_ID="${MINCO_RDS_INSTANCE_ID:-minco-$run_suffix}"
export MINCO_DATABASE_PARAMETER_OWNED=true
export AWS_REGION MINCO_AWS_RUN_ID MINCO_CREATE_TEMP_RDS

[[ "$AWS_REGION" =~ ^[a-z]{2}(-gov)?-[a-z]+-[0-9]+$ ]] || {
  echo "AWS_REGION is not a supported region identifier" >&2
  exit 1
}
[[ "$MINCO_STACK_NAME" =~ ^[A-Za-z][A-Za-z0-9-]{0,47}$ ]] || {
  echo "MINCO_STACK_NAME must be a bounded CloudFormation stack name" >&2
  exit 1
}
[[ "$MINCO_RDS_STACK_NAME" =~ ^[A-Za-z][A-Za-z0-9-]{0,47}$ ]] || {
  echo "MINCO_RDS_STACK_NAME must be a bounded CloudFormation stack name" >&2
  exit 1
}
[[ "$MINCO_RDS_INSTANCE_ID" =~ ^[a-z][a-z0-9-]{0,47}$ ]] || {
  echo "MINCO_RDS_INSTANCE_ID must be a bounded RDS identifier" >&2
  exit 1
}
[[ "$MINCO_SMOKE_APPLICATION" =~ ^[A-Za-z0-9][A-Za-z0-9_-]{0,47}$ ]] || {
  echo "MINCO_SMOKE_APPLICATION must be a bounded Lambda name component" >&2
  exit 1
}
normalized_ssm_parameter_name "$MINCO_DATABASE_URL_PARAMETER" || {
  echo "MINCO_DATABASE_URL_PARAMETER must be a normalized absolute SSM parameter name" >&2
  exit 1
}

role_created=false
user_created=false
parameter_created=false
rds_created=false
bootstrap_cleanup_started=false

root_aws_logged() {
  AWS_PROFILE="$MINCO_ROOT_PROFILE" aws_logged "$@"
}

deploy_aws_logged() {
  AWS_CONFIG_FILE="$profile_config" AWS_PROFILE="$deploy_profile" aws_logged "$@"
}

source_aws_logged() {
  AWS_CONFIG_FILE="$profile_config" AWS_PROFILE="$source_profile" aws_logged "$@"
}

remove_request_files() {
  rm -f \
    "$request_directory/trust-policy.json" \
    "$request_directory/role-policy.json" \
    "$request_directory/user-policy.json" \
    "$request_directory/parameter.json" \
    "$request_directory/user-access-key.json" \
    "$request_directory/role-session.json"
  rmdir "$request_directory" >/dev/null 2>&1 || true
}

cleanup_bootstrap() {
  local status="${1:-0}"
  local cleanup_failure=0
  bootstrap_cleanup_started=true

  application_cleanup=false
  if [[ -f "$MINCO_AWS_EVIDENCE_DIR/cleanup.json" ]] &&
    jq -e '[.[]] | all' "$MINCO_AWS_EVIDENCE_DIR/cleanup.json" >/dev/null; then
    application_cleanup=true
    parameter_created=false
  fi

  parameter_safe_to_delete=true
  if [[ -f "$MINCO_AWS_EVIDENCE_DIR/order-id.txt" &&
    ! -f "$MINCO_AWS_EVIDENCE_DIR/database-cleanup-complete.txt" &&
    ! -f "$MINCO_AWS_EVIDENCE_DIR/database-cleanup-delegated.txt" ]]; then
    parameter_safe_to_delete=false
  fi
  if [[ "$parameter_created" == false &&
    "$application_cleanup" == false &&
    -s "$role_credentials" ]]; then
    parameter_discovery_error="$MINCO_AWS_EVIDENCE_DIR/bootstrap-parameter-discovery-error.txt"
    if parameter_count="$(
      deploy_aws_logged ssm describe-parameters \
        "discover a response-lost run-owned database parameter by exact name" \
        --parameter-filters "Key=Name,Option=Equals,Values=$MINCO_DATABASE_URL_PARAMETER" \
        --query 'length(Parameters)' \
        --output text 2>"$parameter_discovery_error"
    )"; then
      if [[ "$parameter_count" == "1" ]]; then
        if parameter_tags="$(
          deploy_aws_logged ssm list-tags-for-resource \
            "prove the response-lost database parameter has exact run ownership tags" \
            --resource-type Parameter \
            --resource-id "$MINCO_DATABASE_URL_PARAMETER" \
            --output json 2>>"$parameter_discovery_error"
        )" &&
          jq -e \
            --arg run_id "$MINCO_AWS_RUN_ID" \
            '.TagList
             | from_entries
             | .["minco:managed"] == "true"
               and .["minco:purpose"] == "bounded-smoke"
               and .["minco:run-id"] == $run_id' \
            <<<"$parameter_tags" >/dev/null; then
          parameter_created=true
        else
          echo "refusing to delete a database parameter without exact run ownership tags" >&2
          cleanup_failure=1
        fi
      elif [[ "$parameter_count" != "0" ]]; then
        echo "database parameter discovery returned an ambiguous result" >&2
        cleanup_failure=1
      fi
    else
      echo "could not determine whether the database parameter requires recovery cleanup" >&2
      cleanup_failure=1
    fi
    rm -f "$parameter_discovery_error"
  fi
  if [[ "$parameter_created" == true && "$parameter_safe_to_delete" == true ]]; then
    parameter_error="$MINCO_AWS_EVIDENCE_DIR/bootstrap-parameter-delete-error.txt"
    if ! deploy_aws_logged ssm delete-parameter \
      "bootstrap fallback deletes run-owned database SecureString; value never requested" \
      --name "$MINCO_DATABASE_URL_PARAMETER" 2>"$parameter_error"; then
      if ! grep -Eq 'ParameterNotFound' "$parameter_error"; then
        cleanup_failure=1
      fi
    fi
    rm -f "$parameter_error"
  elif [[ "$parameter_created" == true ]]; then
    cleanup_failure=1
  fi

  temporary_database_cleanup_verified=true
  prior_temporary_database_cleanup=false
  if [[ -f "$MINCO_AWS_EVIDENCE_DIR/rds-cleanup.json" ]] &&
    jq -e '[.[]] | all' "$MINCO_AWS_EVIDENCE_DIR/rds-cleanup.json" >/dev/null; then
    prior_temporary_database_cleanup=true
  fi
  if [[ "$rds_created" == true ||
    ( -f "$MINCO_AWS_EVIDENCE_DIR/rds-stack-created.txt" &&
      "$prior_temporary_database_cleanup" == false) ]]; then
    temporary_database_cleanup_verified=false
    if AWS_CONFIG_FILE="$profile_config" \
      AWS_PROFILE="$deploy_profile" \
      scripts/aws/cleanup-temp-rds.sh &&
      jq -e '[.[]] | all' "$MINCO_AWS_EVIDENCE_DIR/rds-cleanup.json" >/dev/null; then
      temporary_database_cleanup_verified=true
      rds_created=false
    else
      cleanup_failure=1
    fi
  fi

  if [[ "$user_created" == false ]]; then
    user_discovery_error="$MINCO_AWS_EVIDENCE_DIR/bootstrap-user-discovery-error.txt"
    if root_aws_logged iam get-user \
      "discover a response-lost temporary bootstrap user by exact deterministic name" \
      --user-name "$user_name" \
      --output json >"$MINCO_AWS_EVIDENCE_DIR/bootstrap-user-discovery.json" \
      2>"$user_discovery_error"; then
      if jq -e \
        --arg run_id "$MINCO_AWS_RUN_ID" \
        '.User.Tags
         | from_entries
         | .["minco:managed"] == "true"
           and .["minco:purpose"] == "bounded-smoke"
           and .["minco:run-id"] == $run_id' \
        "$MINCO_AWS_EVIDENCE_DIR/bootstrap-user-discovery.json" >/dev/null; then
        user_created=true
      else
        echo "refusing to delete a bootstrap user without exact run ownership tags" >&2
        cleanup_failure=1
      fi
    elif ! grep -Eq 'NoSuchEntity|cannot be found' "$user_discovery_error"; then
      echo "could not determine whether the bootstrap user requires recovery cleanup" >&2
      cleanup_failure=1
    fi
    rm -f "$user_discovery_error"
  fi

  if [[ "$role_created" == false ]]; then
    role_discovery_error="$MINCO_AWS_EVIDENCE_DIR/bootstrap-role-discovery-error.txt"
    if root_aws_logged iam get-role \
      "discover a response-lost temporary bootstrap role by exact deterministic name" \
      --role-name "$role_name" \
      --output json >"$MINCO_AWS_EVIDENCE_DIR/bootstrap-role-discovery.json" \
      2>"$role_discovery_error"; then
      if jq -e \
        --arg run_id "$MINCO_AWS_RUN_ID" \
        '.Role.Tags
         | from_entries
         | .["minco:managed"] == "true"
           and .["minco:purpose"] == "bounded-smoke"
           and .["minco:run-id"] == $run_id' \
        "$MINCO_AWS_EVIDENCE_DIR/bootstrap-role-discovery.json" >/dev/null; then
        role_created=true
      else
        echo "refusing to delete a bootstrap role without exact run ownership tags" >&2
        cleanup_failure=1
      fi
    elif ! grep -Eq 'NoSuchEntity|cannot be found' "$role_discovery_error"; then
      echo "could not determine whether the bootstrap role requires recovery cleanup" >&2
      cleanup_failure=1
    fi
    rm -f "$role_discovery_error"
  fi

  if [[ "$user_created" == true ]]; then
    access_key_list_error="$MINCO_AWS_EVIDENCE_DIR/bootstrap-access-key-list-error.txt"
    if access_key_ids="$(
      root_aws_logged iam list-access-keys \
        "list keys on the exact run-owned bootstrap user before teardown" \
        --user-name "$user_name" \
        --query 'AccessKeyMetadata[].AccessKeyId' \
        --output text 2>"$access_key_list_error"
    )"; then
      for access_key_id in $access_key_ids; do
        access_key_error="$MINCO_AWS_EVIDENCE_DIR/bootstrap-access-key-delete-error.txt"
        if ! root_aws_logged iam delete-access-key \
          "delete a temporary non-root Minco access key before user teardown" \
          --user-name "$user_name" \
          --access-key-id "$access_key_id" >/dev/null 2>"$access_key_error" &&
          ! grep -Eq 'NoSuchEntity|cannot be found' "$access_key_error"; then
          cleanup_failure=1
        fi
        rm -f "$access_key_error"
      done
    else
      if ! grep -Eq 'NoSuchEntity|cannot be found' "$access_key_list_error"; then
        cleanup_failure=1
      fi
    fi
    rm -f "$access_key_list_error"
    user_policy_error="$MINCO_AWS_EVIDENCE_DIR/bootstrap-user-policy-delete-error.txt"
    if ! root_aws_logged iam delete-user-policy \
      "remove temporary Minco smoke user inline policy" \
      --user-name "$user_name" \
      --policy-name "$user_policy_name" >/dev/null 2>"$user_policy_error" &&
      ! grep -Eq 'NoSuchEntity|cannot be found' "$user_policy_error"; then
      cleanup_failure=1
    fi
    rm -f "$user_policy_error"
    user_delete_error="$MINCO_AWS_EVIDENCE_DIR/bootstrap-user-delete-error.txt"
    if ! root_aws_logged iam delete-user \
      "delete temporary non-root Minco smoke user after bounded cleanup" \
      --user-name "$user_name" >/dev/null 2>"$user_delete_error" &&
      ! grep -Eq 'NoSuchEntity|cannot be found' "$user_delete_error"; then
      cleanup_failure=1
    fi
    rm -f "$user_delete_error"
  fi

  if [[ "$role_created" == true ]]; then
    role_policy_error="$MINCO_AWS_EVIDENCE_DIR/bootstrap-role-policy-delete-error.txt"
    if ! root_aws_logged iam delete-role-policy \
      "remove temporary Minco smoke role inline policy" \
      --role-name "$role_name" \
      --policy-name "$role_policy_name" >/dev/null 2>"$role_policy_error" &&
      ! grep -Eq 'NoSuchEntity|cannot be found' "$role_policy_error"; then
      cleanup_failure=1
    fi
    rm -f "$role_policy_error"
    role_delete_error="$MINCO_AWS_EVIDENCE_DIR/bootstrap-role-delete-error.txt"
    if ! root_aws_logged iam delete-role \
      "delete temporary non-root Minco smoke role after bounded cleanup" \
      --role-name "$role_name" >/dev/null 2>"$role_delete_error" &&
      ! grep -Eq 'NoSuchEntity|cannot be found' "$role_delete_error"; then
      cleanup_failure=1
    fi
    rm -f "$role_delete_error"
  fi

  user_absent=false
  user_error="$MINCO_AWS_EVIDENCE_DIR/bootstrap-user-verify-error.txt"
  if ! root_aws_logged iam get-user \
    "verify temporary Minco smoke bootstrap user is absent" \
    --user-name "$user_name" >/dev/null 2>"$user_error" &&
    grep -Eq 'NoSuchEntity|cannot be found' "$user_error"; then
    user_absent=true
  fi
  rm -f "$user_error"

  role_absent=false
  role_error="$MINCO_AWS_EVIDENCE_DIR/bootstrap-role-verify-error.txt"
  if ! root_aws_logged iam get-role \
    "verify temporary Minco smoke bootstrap role is absent" \
    --role-name "$role_name" >/dev/null 2>"$role_error" &&
    grep -Eq 'NoSuchEntity|cannot be found' "$role_error"; then
    role_absent=true
  fi
  rm -f "$role_error"

  rm -f "$profile_config" "$source_credentials" "$role_credentials"
  profile_absent=false
  source_credentials_absent=false
  role_credentials_absent=false
  [[ ! -e "$profile_config" ]] && profile_absent=true
  [[ ! -e "$source_credentials" ]] && source_credentials_absent=true
  [[ ! -e "$role_credentials" ]] && role_credentials_absent=true
  remove_request_files

  jq -n \
    --argjson application_cleanup "$application_cleanup" \
    --argjson temporary_database_cleanup_verified "$temporary_database_cleanup_verified" \
    --argjson user_absent "$user_absent" \
    --argjson role_absent "$role_absent" \
    --argjson profile_absent "$profile_absent" \
    --argjson source_credentials_absent "$source_credentials_absent" \
    --argjson role_credentials_absent "$role_credentials_absent" \
    '{
      application_cleanup_verified: $application_cleanup,
      temporary_database_cleanup_verified: $temporary_database_cleanup_verified,
      bootstrap_user_absent: $user_absent,
      bootstrap_role_absent: $role_absent,
      local_non_root_profile_absent: $profile_absent,
      local_bootstrap_user_credentials_absent: $source_credentials_absent,
      local_role_session_credentials_absent: $role_credentials_absent
    }' >"$MINCO_AWS_EVIDENCE_DIR/bootstrap-cleanup.json"
  chmod 600 "$MINCO_AWS_EVIDENCE_DIR/bootstrap-cleanup.json"

  if ! jq -e '[.[]] | all' "$MINCO_AWS_EVIDENCE_DIR/bootstrap-cleanup.json" >/dev/null; then
    cleanup_failure=1
  fi
  if ((cleanup_failure != 0)); then
    return 1
  fi
  return "$status"
}

cleanup_on_exit() {
  local status=$?
  trap - EXIT INT TERM
  if [[ "$bootstrap_cleanup_started" == false ]]; then
    if ! cleanup_bootstrap "$status"; then
      status=1
    fi
  fi
  exit "$status"
}
trap cleanup_on_exit EXIT INT TERM

root_identity="$(
  root_aws_logged sts get-caller-identity \
    "verify approved root bootstrap principal before IAM mutation" \
    --query '{Account:Account,Arn:Arn,UserId:UserId}' \
    --output json
)"
account_id="$(jq -er '.Account' <<<"$root_identity")"
root_arn="$(jq -er '.Arn' <<<"$root_identity")"
[[ "$root_arn" == "arn:aws:iam::$account_id:root" ]] || {
  echo "MINCO_ROOT_PROFILE must resolve to the reviewed account root" >&2
  exit 1
}
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/bootstrap-caller-identity.json" "$root_identity"
unset root_identity

MINCO_AWS_ARTIFACT_BUCKET="${MINCO_AWS_ARTIFACT_BUCKET,,}"
MINCO_AWS_ARTIFACT_BUCKET="${MINCO_AWS_ARTIFACT_BUCKET:0:63}"
export MINCO_AWS_ARTIFACT_BUCKET

stack_arn="arn:aws:cloudformation:$AWS_REGION:$account_id:stack/$MINCO_STACK_NAME/*"
rds_stack_arn="arn:aws:cloudformation:$AWS_REGION:$account_id:stack/$MINCO_RDS_STACK_NAME/*"
bucket_arn="arn:aws:s3:::$MINCO_AWS_ARTIFACT_BUCKET"
parameter_arn="arn:aws:ssm:$AWS_REGION:$account_id:parameter${MINCO_DATABASE_URL_PARAMETER}"
bootstrap_role_arn="arn:aws:iam::$account_id:role/$role_name"
bootstrap_user_arn="arn:aws:iam::$account_id:user/$user_name"
function_name="$MINCO_SMOKE_APPLICATION-dev-api"
function_arn="arn:aws:lambda:$AWS_REGION:$account_id:function:$function_name"
execution_role_arn="arn:aws:iam::$account_id:role/$MINCO_STACK_NAME-*"
log_group_arn="arn:aws:logs:$AWS_REGION:$account_id:log-group:/aws/lambda/$function_name"
rds_instance_arn="arn:aws:rds:$AWS_REGION:$account_id:db:$MINCO_RDS_INSTANCE_ID"
rds_subnet_group_arn="arn:aws:rds:$AWS_REGION:$account_id:subgrp:$MINCO_RDS_STACK_NAME-*"
rds_secret_arn="arn:aws:secretsmanager:$AWS_REGION:$account_id:secret:rds!db-*"

jq -n \
  --arg stack_arn "$stack_arn" \
  --arg rds_stack_arn "$rds_stack_arn" \
  --arg bucket_arn "$bucket_arn" \
  --arg parameter_arn "$parameter_arn" \
  --arg function_arn "$function_arn" \
  --arg execution_role_arn "$execution_role_arn" \
  --arg log_group_arn "$log_group_arn" \
  --arg rds_instance_arn "$rds_instance_arn" \
  --arg rds_subnet_group_arn "$rds_subnet_group_arn" \
  --arg rds_secret_arn "$rds_secret_arn" \
  --arg region "$AWS_REGION" \
  --arg account_id "$account_id" \
  --arg run_id "$MINCO_AWS_RUN_ID" \
  --argjson create_temp_rds "$MINCO_CREATE_TEMP_RDS" \
  '{
    Version: "2012-10-17",
    Statement: ([
      {
        Sid: "Identity",
        Effect: "Allow",
        Action: ["sts:GetCallerIdentity"],
        Resource: "*"
      },
      {
        Sid: "OwnedParameter",
        Effect: "Allow",
        Action: [
          "ssm:AddTagsToResource",
          "ssm:DeleteParameter",
          "ssm:GetParameter",
          "ssm:ListTagsForResource",
          "ssm:PutParameter"
        ],
        Resource: $parameter_arn
      },
      {
        Sid: "ParameterMetadataDiscovery",
        Effect: "Allow",
        Action: ["ssm:DescribeParameters"],
        Resource: "*"
      },
      {
        Sid: "CreateOwnedBucket",
        Effect: "Allow",
        Action: [
          "s3:CreateBucket",
          "s3:TagResource"
        ],
        Resource: $bucket_arn,
        Condition: {
          StringEquals: {
            "aws:RequestTag/minco:managed": "true",
            "aws:RequestTag/minco:purpose": "bounded-smoke",
            "aws:RequestTag/minco:run-id": $run_id
          },
          "ForAllValues:StringEquals": {
            "aws:TagKeys": [
              "minco:managed",
              "minco:purpose",
              "minco:run-id"
            ]
          }
        }
      },
      {
        Sid: "OwnedBucket",
        Effect: "Allow",
        Action: [
          "s3:DeleteBucket",
          "s3:GetBucketLocation",
          "s3:GetBucketTagging",
          "s3:ListBucket",
          "s3:PutEncryptionConfiguration",
          "s3:PutLifecycleConfiguration",
          "s3:PutBucketPublicAccessBlock"
        ],
        Resource: $bucket_arn
      },
      {
        Sid: "OwnedBucketObjects",
        Effect: "Allow",
        Action: ["s3:DeleteObject", "s3:GetObject", "s3:PutObject"],
        Resource: ($bucket_arn + "/*")
      },
      {
        Sid: "OwnedStack",
        Effect: "Allow",
        Action: [
          "cloudformation:CreateChangeSet",
          "cloudformation:DeleteChangeSet",
          "cloudformation:DeleteStack",
          "cloudformation:DescribeChangeSet",
          "cloudformation:DescribeStackEvents",
          "cloudformation:DescribeStacks",
          "cloudformation:ExecuteChangeSet",
          "cloudformation:GetTemplate",
          "cloudformation:GetTemplateSummary",
          "cloudformation:ListChangeSets",
          "cloudformation:ListStackResources"
        ],
        Resource: [$stack_arn, $rds_stack_arn]
      },
      {
        Sid: "ServerlessTransform",
        Effect: "Allow",
        Action: ["cloudformation:CreateChangeSet"],
        Resource: ("arn:aws:cloudformation:" + $region + ":aws:transform/Serverless-2016-10-31")
      },
      {
        Sid: "TemplateValidation",
        Effect: "Allow",
        Action: ["cloudformation:ValidateTemplate"],
        Resource: "*"
      },
      {
        Sid: "OwnedExecutionRole",
        Effect: "Allow",
        Action: [
          "iam:AttachRolePolicy",
          "iam:CreateRole",
          "iam:DeleteRole",
          "iam:DeleteRolePolicy",
          "iam:DetachRolePolicy",
          "iam:GetRole",
          "iam:GetRolePolicy",
          "iam:ListAttachedRolePolicies",
          "iam:ListRolePolicies",
          "iam:PassRole",
          "iam:PutRolePolicy",
          "iam:TagRole",
          "iam:UntagRole"
        ],
        Resource: $execution_role_arn
      },
      {
        Sid: "OwnedFunction",
        Effect: "Allow",
        Action: [
          "lambda:AddPermission",
          "lambda:CreateAlias",
          "lambda:CreateFunction",
          "lambda:DeleteAlias",
          "lambda:DeleteFunction",
          "lambda:DeleteFunctionConcurrency",
          "lambda:GetAlias",
          "lambda:GetFunction",
          "lambda:GetFunctionConfiguration",
          "lambda:GetPolicy",
          "lambda:ListAliases",
          "lambda:ListTags",
          "lambda:ListVersionsByFunction",
          "lambda:PublishVersion",
          "lambda:PutFunctionConcurrency",
          "lambda:RemovePermission",
          "lambda:TagResource",
          "lambda:UntagResource",
          "lambda:UpdateAlias",
          "lambda:UpdateFunctionCode",
          "lambda:UpdateFunctionConfiguration"
        ],
        Resource: [$function_arn, ($function_arn + ":*")]
      },
      {
        Sid: "OwnedLogs",
        Effect: "Allow",
        Action: [
          "logs:CreateLogGroup",
          "logs:DeleteLogGroup",
          "logs:ListTagsForResource",
          "logs:PutRetentionPolicy",
          "logs:TagResource",
          "logs:UntagResource"
        ],
        Resource: [$log_group_arn, ($log_group_arn + ":*")]
      },
      {
        Sid: "LogGroupMetadataDiscovery",
        Effect: "Allow",
        Action: ["logs:DescribeLogGroups"],
        Resource: "*"
      },
      {
        Sid: "ReadTemporaryHttpApiMetadata",
        Effect: "Allow",
        Action: ["apigateway:GET"],
        Resource: ("arn:aws:apigateway:" + $region + "::/*")
      },
      {
        Sid: "MutateTemporaryHttpApiViaCloudFormation",
        Effect: "Allow",
        Action: [
          "apigateway:DELETE",
          "apigateway:PATCH",
          "apigateway:POST",
          "apigateway:PUT"
        ],
        Resource: ("arn:aws:apigateway:" + $region + "::/*"),
        Condition: {
          "ForAnyValue:StringEquals": {
            "aws:CalledVia": "cloudformation.amazonaws.com"
          }
        }
      },
      {
        Sid: "CreateRunOwnedTemporaryHttpApiStage",
        Effect: "Allow",
        Action: "apigateway:POST",
        Resource: ("arn:aws:apigateway:" + $region + "::/apis/*/stages"),
        Condition: {
          StringEquals: {
            "aws:RequestTag/minco:run-id": $run_id,
            "aws:RequestTag/minco:managed": "true",
            "aws:RequestTag/minco:purpose": "bounded-smoke"
          },
          "ForAllValues:StringEquals": {
            "aws:TagKeys": [
              "minco:run-id",
              "minco:managed",
              "minco:purpose",
              "MincoEnvironment",
              "MincoReleaseId",
              "MincoReleaseDigest",
              "httpapi:createdBy",
              "aws:cloudformation:stack-name",
              "aws:cloudformation:stack-id",
              "aws:cloudformation:logical-id"
            ]
          }
        }
      },
      {
        Sid: "TagRunOwnedTemporaryHttpApiStage",
        Effect: "Allow",
        Action: "apigateway:PUT",
        Resource: ("arn:aws:apigateway:" + $region + "::/apis/*/stages"),
        Condition: {
          StringEquals: {
            "aws:RequestTag/minco:run-id": $run_id,
            "aws:RequestTag/minco:managed": "true",
            "aws:RequestTag/minco:purpose": "bounded-smoke"
          },
          "ForAllValues:StringEquals": {
            "aws:TagKeys": [
              "minco:run-id",
              "minco:managed",
              "minco:purpose",
              "MincoEnvironment",
              "MincoReleaseId",
              "MincoReleaseDigest",
              "httpapi:createdBy",
              "aws:cloudformation:stack-name",
              "aws:cloudformation:stack-id",
              "aws:cloudformation:logical-id"
            ]
          }
        }
      },
      {
        Sid: "CreateTemporaryCognitoHarness",
        Effect: "Allow",
        Action: ["cognito-idp:CreateUserPool"],
        Resource: "*",
        Condition: {
          StringEquals: {
            "aws:RequestTag/minco:run-id": $run_id,
            "aws:RequestTag/minco:managed": "true",
            "aws:RequestTag/minco:purpose": "bounded-smoke"
          }
        }
      },
      {
        Sid: "TagOwnedTemporaryCognitoHarness",
        Effect: "Allow",
        Action: ["cognito-idp:TagResource"],
        Resource: ("arn:aws:cognito-idp:" + $region + ":" + $account_id + ":userpool/*"),
        Condition: {
          StringEquals: {
            "aws:RequestTag/minco:run-id": $run_id,
            "aws:RequestTag/minco:managed": "true",
            "aws:RequestTag/minco:purpose": "bounded-smoke"
          },
          "ForAllValues:StringEquals": {
            "aws:TagKeys": [
              "minco:run-id",
              "minco:managed",
              "minco:purpose"
            ]
          }
        }
      },
      {
        Sid: "DiscoverTemporaryCognitoHarness",
        Effect: "Allow",
        Action: ["cognito-idp:ListUserPools"],
        Resource: "*"
      },
      {
        Sid: "UseTemporaryCognitoHarness",
        Effect: "Allow",
        Action: [
          "cognito-idp:AdminCreateUser",
          "cognito-idp:AdminInitiateAuth",
          "cognito-idp:AdminSetUserPassword",
          "cognito-idp:CreateUserPoolClient",
          "cognito-idp:DeleteUserPool",
          "cognito-idp:DescribeUserPool",
          "cognito-idp:ListTagsForResource"
        ],
        Resource: ("arn:aws:cognito-idp:" + $region + ":" + $account_id + ":userpool/*"),
        Condition: {
          StringEquals: {
            "aws:ResourceTag/minco:run-id": $run_id,
            "aws:ResourceTag/minco:managed": "true",
            "aws:ResourceTag/minco:purpose": "bounded-smoke"
          }
        }
      }
    ] + (if $create_temp_rds then [
      {
        Sid: "TemporaryRdsControlPlane",
        Effect: "Allow",
        Action: [
          "rds:AddTagsToResource",
          "rds:CreateDBInstance",
          "rds:CreateDBSubnetGroup",
          "rds:DeleteDBInstance",
          "rds:DeleteDBSubnetGroup",
          "rds:ListTagsForResource",
          "rds:ModifyDBInstance"
        ],
        Resource: [$rds_instance_arn, $rds_subnet_group_arn]
      },
      {
        Sid: "TemporaryRdsDiscovery",
        Effect: "Allow",
        Action: [
          "rds:DescribeDBInstances",
          "rds:DescribeDBSubnetGroups",
          "rds:DescribeOrderableDBInstanceOptions"
        ],
        Resource: "*"
      },
      {
        Sid: "CreateTemporaryRdsManagedSecretViaRds",
        Effect: "Allow",
        Action: [
          "secretsmanager:CreateSecret",
          "secretsmanager:TagResource"
        ],
        Resource: $rds_secret_arn,
        Condition: {
          "ForAnyValue:StringEquals": {
            "aws:CalledVia": "rds.amazonaws.com"
          }
        }
      },
      {
        Sid: "UseExactTemporaryRdsManagedSecret",
        Effect: "Allow",
        Action: [
          "secretsmanager:DeleteSecret",
          "secretsmanager:DescribeSecret",
          "secretsmanager:GetSecretValue"
        ],
        Resource: $rds_secret_arn,
        Condition: {
          StringEquals: {
            "aws:ResourceTag/aws:secretsmanager:owningService": "rds",
            "aws:ResourceTag/aws:rds:primaryDBInstanceArn": $rds_instance_arn
          }
        }
      },
      {
        Sid: "DescribeRdsManagedSecretKey",
        Effect: "Allow",
        Action: ["kms:DescribeKey"],
        Resource: "*"
      },
      {
        Sid: "TemporaryVpcDiscovery",
        Effect: "Allow",
        Action: [
          "ec2:DescribeAvailabilityZones",
          "ec2:DescribeInternetGateways",
          "ec2:DescribeNetworkInterfaces",
          "ec2:DescribeRouteTables",
          "ec2:DescribeSecurityGroups",
          "ec2:DescribeSubnets",
          "ec2:DescribeVpcAttribute",
          "ec2:DescribeVpcEndpoints",
          "ec2:DescribeVpcs"
        ],
        Resource: "*"
      },
      {
        Sid: "RevokeRunOwnedSecurityGroupRules",
        Effect: "Allow",
        Action: [
          "ec2:RevokeSecurityGroupEgress",
          "ec2:RevokeSecurityGroupIngress"
        ],
        Resource: ("arn:aws:ec2:" + $region + ":" + $account_id + ":security-group/*"),
        Condition: {
          StringEquals: {
            "aws:ResourceTag/minco:run-id": $run_id,
            "aws:ResourceTag/minco:managed": "true",
            "aws:ResourceTag/minco:purpose": "bounded-smoke"
          }
        }
      },
      {
        Sid: "TemporaryVpcControlPlaneViaCloudFormation",
        Effect: "Allow",
        Action: [
          "ec2:AssociateRouteTable",
          "ec2:AttachInternetGateway",
          "ec2:AuthorizeSecurityGroupEgress",
          "ec2:AuthorizeSecurityGroupIngress",
          "ec2:CreateInternetGateway",
          "ec2:CreateRoute",
          "ec2:CreateRouteTable",
          "ec2:CreateSecurityGroup",
          "ec2:CreateSubnet",
          "ec2:CreateTags",
          "ec2:CreateVpc",
          "ec2:CreateVpcEndpoint",
          "ec2:DeleteInternetGateway",
          "ec2:DeleteRoute",
          "ec2:DeleteRouteTable",
          "ec2:DeleteSecurityGroup",
          "ec2:DeleteSubnet",
          "ec2:DeleteTags",
          "ec2:DeleteVpc",
          "ec2:DeleteVpcEndpoints",
          "ec2:DetachInternetGateway",
          "ec2:DisassociateRouteTable",
          "ec2:ModifySubnetAttribute",
          "ec2:ModifyVpcAttribute",
          "ec2:ModifyVpcEndpoint"
        ],
        Resource: "*",
        Condition: {
          "ForAnyValue:StringEquals": {
            "aws:CalledVia": "cloudformation.amazonaws.com"
          }
        }
      }
    ] else [] end))
  }' >"$request_directory/role-policy.json"
chmod 600 "$request_directory/role-policy.json"

jq -n \
  --arg role_arn "$bootstrap_role_arn" \
  '{
    Version: "2012-10-17",
    Statement: [{
      Sid: "AssumeExactMincoSmokeRole",
      Effect: "Allow",
      Action: "sts:AssumeRole",
      Resource: $role_arn
    }]
  }' >"$request_directory/user-policy.json"
chmod 600 "$request_directory/user-policy.json"

jq -n \
  --arg user_arn "$bootstrap_user_arn" \
  '{
    Version: "2012-10-17",
    Statement: [{
      Effect: "Allow",
      Principal: {AWS: $user_arn},
      Action: "sts:AssumeRole"
    }]
  }' >"$request_directory/trust-policy.json"
chmod 600 "$request_directory/trust-policy.json"

root_aws_logged accessanalyzer validate-policy \
  "validate temporary Minco smoke role permissions before IAM creation" \
  --policy-document "file://$request_directory/role-policy.json" \
  --policy-type IDENTITY_POLICY \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/bootstrap-role-policy-validation.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/bootstrap-role-policy-validation.json"
jq -e '[.findings[] | select(.findingType == "ERROR")] | length == 0' \
  "$MINCO_AWS_EVIDENCE_DIR/bootstrap-role-policy-validation.json" >/dev/null || {
  echo "AWS Access Analyzer rejected the temporary role policy" >&2
  exit 1
}
root_aws_logged accessanalyzer validate-policy \
  "validate exact-role assumption policy before IAM creation" \
  --policy-document "file://$request_directory/user-policy.json" \
  --policy-type IDENTITY_POLICY \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/bootstrap-user-policy-validation.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/bootstrap-user-policy-validation.json"
jq -e '[.findings[] | select(.findingType == "ERROR")] | length == 0' \
  "$MINCO_AWS_EVIDENCE_DIR/bootstrap-user-policy-validation.json" >/dev/null || {
  echo "AWS Access Analyzer rejected the temporary bootstrap user policy" >&2
  exit 1
}

root_aws_logged iam create-user \
  "create temporary bootstrap user restricted to one exact role" \
  --user-name "$user_name" \
  --tags \
  Key=minco:managed,Value=true \
  Key=minco:purpose,Value=bounded-smoke \
  Key=minco:run-id,Value="$MINCO_AWS_RUN_ID" >/dev/null
user_created=true

role_create_error="$MINCO_AWS_EVIDENCE_DIR/bootstrap-role-create-error.txt"
for attempt in {1..15}; do
  if root_aws_logged iam create-role \
    "create temporary Minco smoke role trusted only by the run-scoped bootstrap user; attempt $attempt" \
    --role-name "$role_name" \
    --assume-role-policy-document "file://$request_directory/trust-policy.json" \
    --max-session-duration 3600 \
    --tags \
    Key=minco:managed,Value=true \
    Key=minco:purpose,Value=bounded-smoke \
    Key=minco:run-id,Value="$MINCO_AWS_RUN_ID" >/dev/null 2>"$role_create_error"; then
    role_created=true
    break
  fi
  if ! grep -Eq 'MalformedPolicyDocument.*Invalid principal|Invalid principal in policy' \
    "$role_create_error"; then
    sed -n '1,8p' "$role_create_error" >&2
    exit 1
  fi
  sleep 2
done
if [[ "$role_created" != true ]]; then
  sed -n '1,8p' "$role_create_error" >&2
  exit 1
fi
rm -f "$role_create_error"

root_aws_logged iam put-role-policy \
  "attach reviewed run-scoped permissions to temporary Minco smoke role" \
  --role-name "$role_name" \
  --policy-name "$role_policy_name" \
  --policy-document "file://$request_directory/role-policy.json"

root_aws_logged iam put-user-policy \
  "allow temporary bootstrap user to assume only the Minco smoke role" \
  --user-name "$user_name" \
  --policy-name "$user_policy_name" \
  --policy-document "file://$request_directory/user-policy.json"

root_aws_logged iam create-access-key \
  "create one run-scoped access key for exact-role assumption; secret redacted" \
  --user-name "$user_name" \
  --query AccessKey \
  --output json >"$request_directory/user-access-key.json"
if ! jq -e '
  (.AccessKeyId | type == "string")
  and (.SecretAccessKey | type == "string")
  and .Status == "Active"
' "$request_directory/user-access-key.json" >/dev/null 2>&1; then
  echo "temporary IAM user access key was not created" >&2
  exit 1
fi
write_evidence_value \
  "$MINCO_AWS_EVIDENCE_DIR/bootstrap-access-key-id.txt" \
  "$(jq -er '.AccessKeyId' "$request_directory/user-access-key.json")"
jq '{
  Version: 1,
  AccessKeyId: .AccessKeyId,
  SecretAccessKey: .SecretAccessKey
}' "$request_directory/user-access-key.json" >"$source_credentials"
chmod 600 "$source_credentials"
rm -f "$request_directory/user-access-key.json"

printf '[profile %s]\nregion = %s\ncredential_process = /bin/cat %s\n' \
  "$source_profile" \
  "$AWS_REGION" \
  "$source_credentials" >"$profile_config"
chmod 600 "$profile_config"

source_identity=""
source_identity_verified=false
source_identity_error="$MINCO_AWS_EVIDENCE_DIR/bootstrap-user-identity-error.txt"
for attempt in {1..15}; do
  if source_identity="$(
    source_aws_logged sts get-caller-identity \
      "verify bootstrap user before exact-role assumption; attempt $attempt" \
      --query '{Account:Account,Arn:Arn,UserId:UserId}' \
      --output json 2>"$source_identity_error"
  )"; then
    source_identity_verified=true
    break
  fi
  if ! grep -Eq 'InvalidClientTokenId|AccessDenied|security token included in the request is invalid' \
    "$source_identity_error"; then
    sed -n '1,8p' "$source_identity_error" >&2
    exit 1
  fi
  sleep 2
done
if [[ "$source_identity_verified" != true ]]; then
  sed -n '1,8p' "$source_identity_error" >&2
  exit 1
fi
rm -f "$source_identity_error"
jq -e \
  --arg account "$account_id" \
  --arg user "$user_name" \
  '.Account == $account and .Arn == ("arn:aws:iam::" + $account + ":user/" + $user)' \
  <<<"$source_identity" >/dev/null || {
  echo "source profile did not resolve to the expected bootstrap user" >&2
  exit 1
}
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/bootstrap-user-caller-identity.json" "$source_identity"
unset source_identity

role_session_created=false
role_session_error="$MINCO_AWS_EVIDENCE_DIR/bootstrap-role-session-error.txt"
for attempt in {1..15}; do
  if source_aws_logged sts assume-role \
    "issue a one-hour session for the exact temporary Minco smoke role; attempt $attempt; credentials redacted" \
    --role-arn "$bootstrap_role_arn" \
    --role-session-name "minco-$run_suffix" \
    --duration-seconds 3600 \
    --query Credentials \
    --output json >"$request_directory/role-session.json" 2>"$role_session_error"; then
    role_session_created=true
    break
  fi
  if ! grep -Eq 'AccessDenied|not authorized to perform: sts:AssumeRole' "$role_session_error"; then
    sed -n '1,8p' "$role_session_error" >&2
    exit 1
  fi
  sleep 2
done
if [[ "$role_session_created" != true ]]; then
  sed -n '1,8p' "$role_session_error" >&2
  exit 1
fi
rm -f "$role_session_error"
if ! jq -e '
  (.AccessKeyId | type == "string")
  and (.SecretAccessKey | type == "string")
  and (.SessionToken | type == "string")
  and (.Expiration | type == "string")
' "$request_directory/role-session.json" >/dev/null 2>&1; then
  echo "temporary Minco role session was not created" >&2
  exit 1
fi
jq '{
  Version: 1,
  AccessKeyId: .AccessKeyId,
  SecretAccessKey: .SecretAccessKey,
  SessionToken: .SessionToken,
  Expiration: .Expiration
}' "$request_directory/role-session.json" >"$role_credentials"
chmod 600 "$role_credentials"
rm -f "$request_directory/role-session.json"

printf '\n[profile %s]\nregion = %s\ncredential_process = /bin/cat %s\n' \
  "$deploy_profile" \
  "$AWS_REGION" \
  "$role_credentials" >>"$profile_config"
chmod 600 "$profile_config"
jq -n \
  --arg source_profile "$source_profile" \
  --arg deploy_profile "$deploy_profile" \
  --arg user "$user_name" \
  --arg role "$role_name" \
  '{
    source_profile: $source_profile,
    deploy_profile: $deploy_profile,
    bootstrap_user: $user,
    deploy_role: $role,
    isolated_config: true,
    persisted_in_default_config: false
  }' >"$MINCO_AWS_EVIDENCE_DIR/bootstrap-profile.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/bootstrap-profile.json"

deploy_identity=""
deploy_identity="$(
  deploy_aws_logged sts get-caller-identity \
    "verify temporary non-root Minco role session before application mutation" \
    --query '{Account:Account,Arn:Arn,UserId:UserId}' \
    --output json
)"
jq -e \
  --arg account "$account_id" \
  --arg role "$role_name" \
  --arg session "minco-$run_suffix" \
  '.Account == $account
   and .Arn == ("arn:aws:sts::" + $account + ":assumed-role/" + $role + "/" + $session)' \
  <<<"$deploy_identity" >/dev/null || {
  echo "deploy profile did not resolve to the expected temporary role session" >&2
  exit 1
}
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/deploy-caller-identity.json" "$deploy_identity"
unset deploy_identity

database_url=""
if [[ "$MINCO_CREATE_TEMP_RDS" == "true" ]]; then
  rds_created=true
  AWS_CONFIG_FILE="$profile_config" \
  AWS_PROFILE="$deploy_profile" \
  scripts/aws/create-temp-rds.sh
  database_url="$(<"$MINCO_AWS_EVIDENCE_DIR/database-url-lambda.txt")"
  MINCO_RDS_CA_BUNDLE="$MINCO_AWS_EVIDENCE_DIR/rds-ca-bundle.pem"
  MINCO_LAMBDA_SUBNET_IDS="$(<"$MINCO_AWS_EVIDENCE_DIR/lambda-subnet-ids.txt")"
  MINCO_LAMBDA_SECURITY_GROUP_IDS="$(<"$MINCO_AWS_EVIDENCE_DIR/lambda-security-group-id.txt")"
  MINCO_DATABASE_INSTANCE_OWNED=true
  MINCO_DATABASE_MIGRATION_COMPLETE=true
  export \
    MINCO_DATABASE_INSTANCE_OWNED \
    MINCO_DATABASE_MIGRATION_COMPLETE \
    MINCO_LAMBDA_SECURITY_GROUP_IDS \
    MINCO_LAMBDA_SUBNET_IDS \
    MINCO_RDS_CA_BUNDLE
elif [[ -n "${MINCO_DATABASE_URL_SOURCE_PARAMETER:-}" ]]; then
  root_aws_logged ssm get-parameter \
    "read explicitly selected Minco database source parameter; value redacted" \
    --name "$MINCO_DATABASE_URL_SOURCE_PARAMETER" \
    --query 'Parameter.{Name:Name,Type:Type,Version:Version}' \
    --output json >"$MINCO_AWS_EVIDENCE_DIR/database-source-metadata.json"
  database_url="$(
    root_aws_logged ssm get-parameter \
      "read explicitly selected Minco database source parameter for run-owned copy; value redacted" \
      --name "$MINCO_DATABASE_URL_SOURCE_PARAMETER" \
      --with-decryption \
      --query Parameter.Value \
      --output text
  )"
elif [[ -n "${MINCO_DATABASE_URL_FILE:-}" ]]; then
  database_url="$(<"$MINCO_DATABASE_URL_FILE")"
elif [[ -n "${MINCO_DATABASE_URL:-}" ]]; then
  database_url="$MINCO_DATABASE_URL"
fi

[[ "$database_url" == postgres://* || "$database_url" == postgresql://* ]] || {
  echo "the selected database value is not a PostgreSQL URL" >&2
  exit 1
}
(( ${#database_url} <= 4096 )) || {
  echo "the selected database URL exceeds the SSM Standard parameter limit" >&2
  exit 1
}

if [[ "$MINCO_CREATE_TEMP_RDS" == "false" ]]; then
  record_external_database_touch \
    "bootstrap connectivity check" \
    "verify explicitly selected Minco PostgreSQL endpoint before cloud copy; URL redacted"
  PGCONNECT_TIMEOUT=10 psql_with_url "$database_url" \
    --no-psqlrc \
    --quiet \
    --tuples-only \
    --no-align \
    --set ON_ERROR_STOP=1 \
    --command 'SELECT 1' | grep -qx '1'
fi

jq -n \
  --arg name "$MINCO_DATABASE_URL_PARAMETER" \
  --arg value "$database_url" \
  --arg run_id "$MINCO_AWS_RUN_ID" \
  '{
    Name: $name,
    Description: "Temporary Minco bounded AWS smoke database URL",
    Value: $value,
    Type: "SecureString",
    Tier: "Standard",
    DataType: "text",
    Tags: [
      {Key: "minco:managed", Value: "true"},
      {Key: "minco:purpose", Value: "bounded-smoke"},
      {Key: "minco:run-id", Value: $run_id}
    ]
  }' >"$request_directory/parameter.json"
chmod 600 "$request_directory/parameter.json"
unset database_url MINCO_DATABASE_URL

deploy_aws_logged ssm put-parameter \
  "create run-owned temporary database SecureString; value redacted" \
  --cli-input-json "file://$request_directory/parameter.json" >/dev/null
parameter_created=true
rm -f "$request_directory/parameter.json"

deploy_aws_logged ssm get-parameter \
  "verify run-owned database parameter metadata without requesting its value" \
  --name "$MINCO_DATABASE_URL_PARAMETER" \
  --query 'Parameter.{Name:Name,Type:Type,Version:Version}' \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/created-parameter-metadata.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/created-parameter-metadata.json"
jq -e \
  --arg name "$MINCO_DATABASE_URL_PARAMETER" \
  '.Name == $name and .Type == "SecureString"' \
  "$MINCO_AWS_EVIDENCE_DIR/created-parameter-metadata.json" >/dev/null

AWS_CONFIG_FILE="$profile_config" \
AWS_PROFILE="$deploy_profile" \
scripts/aws/run-bounded-smoke.sh
parameter_created=false

cleanup_bootstrap 0
trap - EXIT INT TERM
printf 'Bounded root bootstrap, non-root smoke and cleanup passed: %s\n' "$MINCO_AWS_RUN_ID"
