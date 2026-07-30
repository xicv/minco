#!/usr/bin/env bash
set -euo pipefail

minco_repo_root() {
  cd "$(dirname "${BASH_SOURCE[0]}")/../../.." >/dev/null 2>&1 || return
  pwd
}

require_command() {
  command -v "$1" >/dev/null || {
    printf '%s is required\n' "$1" >&2
    return 1
  }
}

require_safe_name() {
  local label="$1"
  local value="$2"
  [[ ${#value} -le 48 && "$value" =~ ^[a-zA-Z0-9][a-zA-Z0-9._-]*$ ]] || {
    printf '%s contains unsupported characters: %s\n' "$label" "$value" >&2
    return 1
  }
}

normalized_ssm_parameter_name() {
  local parameter_name="$1"
  [[ "$parameter_name" =~ ^/[A-Za-z0-9_./-]+$ &&
    "$parameter_name" != *//* &&
    "$parameter_name" != */ ]]
}

write_bounded_deployment_target_config() {
  local output_path="$1"
  local account_id="$2"
  local region="$3"
  local role_arn="$4"
  local stack_name="$5"
  local artifact_bucket="$6"
  local database_parameter_name="$7"
  local database_kms_key_arn="$8"
  local lambda_subnet_ids="$9"
  local lambda_security_group_ids="${10}"
  local run_id="${11}"

  {
    printf 'schema_version = 1\ndefault_environment = "dev"\n\n'
    printf '[environments.dev]\nenabled = true\n'
    printf 'expected_account_id = "%s"\n' "$account_id"
    printf 'expected_region = "%s"\n' "$region"
    printf 'expected_role_arn = "%s"\n' "$role_arn"
    printf 'stack_name = "%s"\n' "$stack_name"
    printf 'artifact_bucket = "%s"\n' "$artifact_bucket"
    printf 'database_url_parameter_name = "%s"\n' "$database_parameter_name"
    if [[ -n "$database_kms_key_arn" ]]; then
      printf 'database_kms_key_arn = "%s"\n' "$database_kms_key_arn"
    fi
    if [[ -n "$lambda_subnet_ids" ]]; then
      printf 'lambda_subnet_ids = ["%s"]\n' "${lambda_subnet_ids//,/\",\"}"
      printf 'lambda_security_group_ids = ["%s"]\n' \
        "${lambda_security_group_ids//,/\",\"}"
    fi
    printf 'stack_tags = { "minco:managed" = "true", '
    printf '"minco:purpose" = "bounded-smoke", "minco:run-id" = "%s" }\n' "$run_id"
  } >"$output_path"
  chmod 600 "$output_path"
}

bounded_review_stack_cleanup_is_authorized() {
  local stack_description_path="$1"
  local stack_resources_path="$2"
  local stack_preflight_absence_path="$3"
  local expected_stack_name="$4"
  require_command jq

  [[ -f "$stack_description_path" &&
    -f "$stack_resources_path" &&
    -f "$stack_preflight_absence_path" &&
    "$(<"$stack_preflight_absence_path")" == "$expected_stack_name" ]] || return 1

  jq -e \
    --arg stack "$expected_stack_name" \
    '(.Stacks | type == "array" and length == 1)
     and .Stacks[0].StackName == $stack
     and .Stacks[0].StackStatus == "REVIEW_IN_PROGRESS"
     and ((.Stacks[0].Tags // null) | type == "array" and length == 0)' \
    "$stack_description_path" >/dev/null &&
    jq -e \
      'has("StackResourceSummaries")
       and (.StackResourceSummaries | type == "array" and length == 0)' \
      "$stack_resources_path" >/dev/null
}

access_analyzer_role_policy_is_accepted() {
  local validation_path="$1"
  local policy_path="$2"
  local region="$3"
  local run_id="$4"
  require_command jq

  jq -e \
    --arg region "$region" \
    --arg run_id "$run_id" \
    --slurpfile validation "$validation_path" \
    '
      ($validation | length == 1) as $has_one_validation
      | ($validation[0].findings? | type == "array") as $has_findings
      | [$validation[0].findings[]? | select(.findingType == "ERROR")] as $errors
      | if $has_one_validation and $has_findings and ($errors | length == 0)
        then true
        else
          (
            .Statement
            | to_entries
            | map(select(.value.Sid == "TagRunOwnedTemporaryHttpApiStage"))
          ) as $stage_tag_statements
          | ($stage_tag_statements | length == 1)
          and (
            [
              .Statement[]
              | .Action
              | if type == "array" then .[] else . end
              | select(. == "apigateway:TagResource")
            ]
            | length == 1
          )
          and (
            [
              .Statement[]
              | .Action
              | if type == "array" then .[] else . end
              | select(type == "string" and contains("*"))
            ]
            | length == 0
          )
          and (
            $stage_tag_statements[0].value == {
              Sid: "TagRunOwnedTemporaryHttpApiStage",
              Effect: "Allow",
              Action: "apigateway:TagResource",
              Resource: (
                "arn:aws:apigateway:" + $region + "::/apis/*/stages"
              ),
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
            }
          )
          and ($errors | length == 1)
          and ($errors[0].issueCode == "INVALID_ACTION")
          and (
            $errors[0].findingDetails
            == "The action apigateway:TagResource does not exist."
          )
          and ($errors[0].locations | type == "array" and length == 1)
          and (
            $errors[0].locations[0].path == [
              {value: "Statement"},
              {index: $stage_tag_statements[0].key},
              {value: "Action"}
            ]
          )
        end
    ' "$policy_path" >/dev/null
}

s3_tagged_create_configuration() {
  local region="$1"
  local run_id="$2"
  require_command jq
  jq -cn \
    --arg region "$region" \
    --arg run_id "$run_id" \
    '{
      Tags: [
        {Key: "minco:managed", Value: "true"},
        {Key: "minco:purpose", Value: "bounded-smoke"},
        {Key: "minco:run-id", Value: $run_id}
      ]
    }
    | if $region == "us-east-1"
      then .
      else . + {LocationConstraint: $region}
      end'
}

postgres_url_to_conninfo() {
  local database_url="$1"
  require_command python3
  printf '%s' "$database_url" | python3 -c '
import re
import sys
from urllib.parse import parse_qsl, unquote, urlsplit

url = sys.stdin.read()
parsed = urlsplit(url)
if parsed.scheme not in {"postgres", "postgresql"}:
    raise SystemExit("PostgreSQL URL must use postgres or postgresql")
if parsed.fragment:
    raise SystemExit("PostgreSQL URL fragments are unsupported")
if not parsed.hostname:
    raise SystemExit("PostgreSQL URL must include a host")

def quote(value: str) -> str:
    single_quote = chr(39)
    return (
        single_quote
        + value.replace("\\", "\\\\").replace(single_quote, "\\" + single_quote)
        + single_quote
    )

values = [
    ("host", parsed.hostname),
]
if parsed.port is not None:
    values.append(("port", str(parsed.port)))
if parsed.username is not None:
    values.append(("user", unquote(parsed.username)))
if parsed.password is not None:
    values.append(("password", unquote(parsed.password)))
if parsed.path and parsed.path != "/":
    values.append(("dbname", unquote(parsed.path.removeprefix("/"))))

structural = {"host", "hostaddr", "port", "user", "password", "dbname"}
seen = set()
for key, value in parse_qsl(parsed.query, keep_blank_values=True):
    if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", key):
        raise SystemExit("PostgreSQL URL contains an invalid query key")
    if key in structural or key in seen:
        raise SystemExit("PostgreSQL URL contains a duplicate connection key")
    seen.add(key)
    values.append((key, value))

sys.stdout.write(" ".join(f"{key}={quote(value)}" for key, value in values))
'
}

psql_with_url() {
  local database_url="$1"
  local conninfo
  shift
  conninfo="$(postgres_url_to_conninfo "$database_url")"
  PGCONNECT_TIMEOUT="${PGCONNECT_TIMEOUT:-10}" \
    PGDATABASE="$conninfo" \
    command psql "$@"
}

initialize_cloud_journal() {
  : "${MINCO_AWS_RUN_ID:?set MINCO_AWS_RUN_ID}"
  require_safe_name "MINCO_AWS_RUN_ID" "$MINCO_AWS_RUN_ID"
  MINCO_AWS_EVIDENCE_DIR="$(minco_repo_root)/target/minco/aws/$MINCO_AWS_RUN_ID"
  mkdir -p "$MINCO_AWS_EVIDENCE_DIR"
  chmod 700 "$MINCO_AWS_EVIDENCE_DIR"
  MINCO_AWS_TOUCH_LOG="$MINCO_AWS_EVIDENCE_DIR/cloud-touches.jsonl"
  touch "$MINCO_AWS_TOUCH_LOG"
  chmod 600 "$MINCO_AWS_TOUCH_LOG"
  export MINCO_AWS_EVIDENCE_DIR MINCO_AWS_TOUCH_LOG
}

record_cloud_touch() {
  local service="$1"
  local action="$2"
  local detail="$3"
  jq -cn \
    --arg at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    --arg run_id "$MINCO_AWS_RUN_ID" \
    --arg service "$service" \
    --arg action "$action" \
    --arg detail "$detail" \
    '{at: $at, run_id: $run_id, service: $service, action: $action, detail: $detail}' \
    >>"$MINCO_AWS_TOUCH_LOG"
}

aws_logged() {
  local service="$1"
  local operation="$2"
  local detail="$3"
  shift 3
  record_cloud_touch "aws:$service" "$operation" "$detail" || return
  AWS_PAGER="" command aws \
    --no-cli-pager \
    --region "$AWS_REGION" \
    "$service" "$operation" "$@"
}

wait_for_s3_bucket_visibility() {
  local bucket="$1"
  local region="$2"
  local error_path="$3"
  local max_attempts="${4:-15}"
  local delay_seconds="${5:-2}"
  local attempt

  [[ "$max_attempts" =~ ^[1-9][0-9]*$ ]] || {
    printf 'S3 bucket visibility attempts must be a positive integer\n' >&2
    return 1
  }
  [[ "$delay_seconds" =~ ^[0-9]+$ ]] || {
    printf 'S3 bucket visibility delay must be a non-negative integer\n' >&2
    return 1
  }

  for ((attempt = 1; attempt <= max_attempts; attempt += 1)); do
    : >"$error_path"
    chmod 600 "$error_path"
    if aws_logged s3api head-bucket \
      "wait for newly created artifact bucket $bucket to become visible; attempt $attempt" \
      --bucket "$bucket" \
      --region "$region" >/dev/null 2>"$error_path"; then
      rm -f "$error_path"
      return 0
    fi
    if ! grep -Eq '404|NoSuchBucket|Not Found' "$error_path"; then
      printf 'could not verify artifact bucket visibility\n' >&2
      sed -n '1,8p' "$error_path" >&2
      return 1
    fi
    if ((attempt < max_attempts)); then
      sleep "$delay_seconds"
    fi
  done

  printf 'artifact bucket did not become visible after %s attempts\n' \
    "$max_attempts" >&2
  sed -n '1,8p' "$error_path" >&2
  return 1
}

sam_logged() {
  local action="$1"
  local detail="$2"
  shift 2
  record_cloud_touch "aws:sam" "$action" "$detail" || return
  AWS_PAGER="" SAM_CLI_TELEMETRY=0 command sam "$@"
}

record_external_database_touch() {
  record_cloud_touch "external:postgresql" "$1" "$2"
}

write_evidence_value() {
  local path="$1"
  local value="$2"
  printf '%s\n' "$value" >"$path"
  chmod 600 "$path"
}

normalize_lambda_zip() (
  local artifact="$1"
  local artifact_directory
  local temporary_directory
  local entries
  local bootstrap_count
  local certificate_count
  local entry_count

  for command in basename chmod dirname grep mktemp mv pwd rmdir rm touch unzip zip; do
    require_command "$command"
  done
  [[ -f "$artifact" && ! -L "$artifact" ]] || {
    printf 'Lambda artifact must be a regular non-symlink file: %s\n' "$artifact" >&2
    return 1
  }
  artifact_directory="$(cd "$(dirname "$artifact")" && pwd -P)"
  artifact="$artifact_directory/$(basename "$artifact")"
  unzip -tqq "$artifact"
  entries="$(unzip -Z1 "$artifact")"
  bootstrap_count="$(printf '%s\n' "$entries" | grep -c '^bootstrap$' || true)"
  certificate_count="$(
    printf '%s\n' "$entries" | grep -c '^rds-ca-bundle\.pem$' || true
  )"
  entry_count="$(printf '%s\n' "$entries" | grep -c '.' || true)"
  [[ "$bootstrap_count" == 1 && "$certificate_count" -le 1 ]] || {
    printf 'Lambda artifact has an invalid bootstrap or CA bundle inventory\n' >&2
    return 1
  }
  [[ "$entry_count" -eq $((bootstrap_count + certificate_count)) ]] || {
    printf 'Lambda artifact contains an unexpected archive entry\n' >&2
    return 1
  }

  temporary_directory="$(
    mktemp -d "$artifact_directory/.minco-lambda-normalize.XXXXXX"
  )"
  # Invoked indirectly by the EXIT trap.
  # shellcheck disable=SC2329
  cleanup_normalized_lambda() {
    rm -f \
      "$temporary_directory/bootstrap" \
      "$temporary_directory/rds-ca-bundle.pem" \
      "$temporary_directory/bootstrap.zip"
    rmdir "$temporary_directory" >/dev/null 2>&1 || true
  }
  trap cleanup_normalized_lambda EXIT

  unzip -p "$artifact" bootstrap >"$temporary_directory/bootstrap"
  [[ -s "$temporary_directory/bootstrap" ]] || {
    printf 'Lambda bootstrap must not be empty\n' >&2
    return 1
  }
  chmod 0755 "$temporary_directory/bootstrap"
  touch -t 198001010000 "$temporary_directory/bootstrap"
  if [[ "$certificate_count" == 1 ]]; then
    unzip -p "$artifact" rds-ca-bundle.pem \
      >"$temporary_directory/rds-ca-bundle.pem"
    chmod 0644 "$temporary_directory/rds-ca-bundle.pem"
    touch -t 198001010000 "$temporary_directory/rds-ca-bundle.pem"
    (
      cd "$temporary_directory"
      zip -X -q bootstrap.zip bootstrap rds-ca-bundle.pem
    )
  else
    (
      cd "$temporary_directory"
      zip -X -q bootstrap.zip bootstrap
    )
  fi
  mv "$temporary_directory/bootstrap.zip" "$artifact"
)
