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

current_source_revision() {
  local revision
  if [[ -d .jj ]] && command -v jj >/dev/null; then
    revision="$(jj log -r @ --no-graph -T 'commit_id')"
  elif command -v git >/dev/null && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    revision="$(git rev-parse HEAD)"
  else
    printf 'a JJ or Git checkout is required to bind rehearsal authority\n' >&2
    return 1
  fi
  [[ "$revision" =~ ^[0-9a-f]{40}([0-9a-f]{24})?$ ]] || {
    printf 'could not resolve an exact source revision for rehearsal authority\n' >&2
    return 1
  }
  printf '%s\n' "$revision"
}

write_rehearsal_authority_receipt() {
  local authority_file="$1"
  local approval_digest="$2"
  local output_path="$3"
  jq \
    --arg approval_digest "$approval_digest" \
    '{
      schema_version,
      authority_kind,
      run_id,
      source_revision,
      environment,
      database_boundary_mode: .database_boundary.mode,
      resource_allowlist,
      cleanup_blast_radius,
      max_duration_minutes,
      max_spend_usd,
      approved_at,
      expires_at,
      approval_digest: $approval_digest
    }' "$authority_file" >"$output_path"
  chmod 600 "$output_path"
}

write_multi_release_rehearsal_authority_receipt() {
  local authority_file="$1"
  local approval_digest="$2"
  local output_path="$3"
  jq \
    --arg approval_digest "$approval_digest" \
    '{
      schema_version,
      authority_kind,
      run_id,
      source_revisions,
      release_sequence,
      environment,
      database_boundary_mode: .database_boundary.mode,
      resource_allowlist,
      cleanup_blast_radius,
      max_duration_minutes,
      max_spend_usd,
      approved_at,
      expires_at,
      approval_digest: $approval_digest
    }' "$authority_file" >"$output_path"
  chmod 600 "$output_path"
}

initialize_rehearsal_deadline() {
  local authority_file="$1"
  local duration_minutes
  local now_epoch
  duration_minutes="$(jq -er '.max_duration_minutes' "$authority_file")"
  now_epoch="$(date -u +%s)"
  if [[ -n "${MINCO_REHEARSAL_STARTED_EPOCH:-}" ||
    -n "${MINCO_REHEARSAL_DEADLINE_EPOCH:-}" ]]; then
    [[ "${MINCO_REHEARSAL_STARTED_EPOCH:-}" =~ ^[0-9]+$ &&
      "${MINCO_REHEARSAL_DEADLINE_EPOCH:-}" =~ ^[0-9]+$ &&
      "$MINCO_REHEARSAL_STARTED_EPOCH" -le "$now_epoch" &&
      "$MINCO_REHEARSAL_DEADLINE_EPOCH" -eq \
        $((MINCO_REHEARSAL_STARTED_EPOCH + duration_minutes * 60)) ]] || {
      printf 'inherited rehearsal duration boundary is invalid\n' >&2
      return 1
    }
  else
    MINCO_REHEARSAL_STARTED_EPOCH="$now_epoch"
    MINCO_REHEARSAL_DEADLINE_EPOCH="$((now_epoch + duration_minutes * 60))"
  fi
  MINCO_REHEARSAL_CLEANUP_MODE=false
  export \
    MINCO_REHEARSAL_CLEANUP_MODE \
    MINCO_REHEARSAL_DEADLINE_EPOCH \
    MINCO_REHEARSAL_STARTED_EPOCH
}

enforce_rehearsal_duration() {
  [[ -n "${MINCO_REHEARSAL_DEADLINE_EPOCH:-}" ]] || return 0
  [[ "$MINCO_REHEARSAL_DEADLINE_EPOCH" =~ ^[0-9]+$ ]] || {
    printf 'rehearsal duration boundary is invalid\n' >&2
    return 1
  }
  [[ "${MINCO_REHEARSAL_CLEANUP_MODE:-false}" == true ]] && return 0
  if (( $(date -u +%s) > MINCO_REHEARSAL_DEADLINE_EPOCH )); then
    printf 'rehearsal duration authority expired; only cleanup may continue\n' >&2
    return 1
  fi
}

normalized_ssm_parameter_name() {
  local parameter_name="$1"
  [[ "$parameter_name" =~ ^/[A-Za-z0-9_./-]+$ &&
    "$parameter_name" != *//* &&
    "$parameter_name" != */ ]]
}

http_response_request_id() {
  local headers_path="$1"
  [[ -f "$headers_path" ]] || return 1
  awk '
    BEGIN { IGNORECASE = 1 }
    /^x-request-id:/ || /^x-amzn-requestid:/ || /^apigw-requestid:/ {
      sub(/^[^:]+:[[:space:]]*/, "")
      sub(/\r$/, "")
      print
      exit
    }
  ' "$headers_path"
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

bounded_phase_change_set_is_authorized() {
  local receipt_path="$1"
  local review_policy="$2"
  require_command jq

  [[ -f "$receipt_path" && ! -L "$receipt_path" ]] || return 1
  jq -e '
    def resource_change:
      type == "object"
      and keys == [
        "action",
        "logical_id",
        "policy_action",
        "replacement",
        "resource_type",
        "scope"
      ]
      and (.logical_id | type == "string" and length > 0)
      and (.resource_type | type == "string" and length > 0)
      and (
        .action
        | . == "add"
          or . == "modify"
          or . == "remove"
          or . == "import"
          or . == "dynamic"
          or . == "sync_with_actual"
      )
      and (
        .replacement == null
        or .replacement == "never"
        or .replacement == "conditional"
        or .replacement == "always"
      )
      and (
        .policy_action == null
        or .policy_action == "delete"
        or .policy_action == "retain"
        or .policy_action == "snapshot"
        or .policy_action == "replace_and_delete"
        or .policy_action == "replace_and_retain"
        or .policy_action == "replace_and_snapshot"
      )
      and (
        .scope
        | type == "array"
          and all(
            . == "properties"
            or . == "metadata"
            or . == "creation_policy"
            or . == "update_policy"
            or . == "deletion_policy"
            or . == "update_replace_policy"
            or . == "tags"
          )
      );
    .change_set.review
    | type == "object"
      and keys == [
        "additions",
        "deletions",
        "imports",
        "indeterminate",
        "metadata_syncs",
        "modifications",
        "replacements"
      ]
      and ([.[] | type] | all(. == "array"))
      and ([.[] | .[] | resource_change] | all)
  ' "$receipt_path" >/dev/null || return 1
  case "$review_policy" in
    bounded_create_v1)
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
          [
            .change_set.review.additions[]
            | .action == "add"
              and .replacement == null
              and .policy_action == null
              and .scope == []
          ]
          | all
        )
        and (
          [.change_set.review.additions[].resource_type]
          | all(
              . == "AWS::ApiGatewayV2::Api"
              or . == "AWS::ApiGatewayV2::Stage"
              or . == "AWS::IAM::Role"
              or . == "AWS::Lambda::Alias"
              or . == "AWS::Lambda::Function"
              or . == "AWS::Lambda::Permission"
              or . == "AWS::Lambda::Version"
              or . == "AWS::Logs::LogGroup"
            )
        )
      ' "$receipt_path" >/dev/null
      ;;
    bounded_release_update_v1)
      jq -e '
        def candidate_version:
          .resource_type == "AWS::Lambda::Version"
          and (.logical_id | startswith("ApiFunctionVersion"));
        def candidate_update:
          (
            .resource_type == "AWS::Lambda::Function"
            and .logical_id == "ApiFunction"
          )
          or (
            .resource_type == "AWS::Lambda::Alias"
            and .logical_id == "ApiFunctionAliascandidate"
          );
        def addition:
          .action == "add"
          and .replacement == null
          and .policy_action == null
          and .scope == [];
        def modification:
          .action == "modify"
          and (.replacement == null or .replacement == "never")
          and .policy_action == null
          and (.scope | length > 0 and all(. == "properties"));
        def replacement:
          .action == "modify"
          and (.replacement == "conditional" or .replacement == "always")
          and (
            .policy_action == null
            or .policy_action == "replace_and_delete"
          )
          and (.scope | length > 0 and all(. == "properties"));
        def deletion:
          .action == "remove"
          and .replacement == null
          and (.policy_action == null or .policy_action == "delete")
          and .scope == [];
        .change_set.change_set_type == "update"
        and (.change_set.review.imports | length == 0)
        and (.change_set.review.indeterminate | length == 0)
        and (.change_set.review.metadata_syncs | length == 0)
        and (
          [
            .change_set.review.additions[],
            .change_set.review.modifications[],
            .change_set.review.replacements[],
            .change_set.review.deletions[]
          ]
          | length > 0
        )
        and (
          [.change_set.review.additions[] | candidate_version and addition]
          | all
        )
        and (
          [.change_set.review.modifications[] | candidate_update and modification]
          | all
        )
        and (
          [.change_set.review.replacements[] | candidate_version and replacement]
          | all
        )
        and (
          [.change_set.review.deletions[] | candidate_version and deletion]
          | all
        )
      ' "$receipt_path" >/dev/null
      ;;
    *)
      printf 'unsupported bounded change-set review policy: %s\n' \
        "$review_policy" >&2
      return 1
      ;;
  esac
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
  enforce_rehearsal_duration || return
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
