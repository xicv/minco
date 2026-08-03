#!/usr/bin/env bash
set -euo pipefail

if (( $# != 10 )); then
  echo "usage: validate-rehearsal-authority.sh AUTHORITY APPROVAL RUN_ID SOURCE REGION PROFILE ENVIRONMENT DATABASE_JSON RESOURCE_SCOPE CLEANUP_SCOPE" >&2
  exit 2
fi

authority_file="$1"
approval_digest="$2"
run_id="$3"
source_revision="$4"
region="$5"
profile="$6"
environment="$7"
database_boundary="$8"
resource_allowlist="$9"
cleanup_blast_radius="${10}"

for command in jq shasum; do
  command -v "$command" >/dev/null || {
    printf '%s is required\n' "$command" >&2
    exit 1
  }
done

[[ -f "$authority_file" && ! -L "$authority_file" ]] || {
  echo "rehearsal authority must be a regular non-symlink file" >&2
  exit 1
}
[[ "$approval_digest" =~ ^[0-9a-f]{64}$ ]] || {
  echo "rehearsal authority approval must be a SHA-256 digest" >&2
  exit 1
}
actual_digest="$(shasum -a 256 "$authority_file" | awk '{print $1}')"
[[ "$actual_digest" == "$approval_digest" ]] || {
  echo "rehearsal authority approval does not match the exact document digest" >&2
  exit 1
}
jq -e . <<<"$database_boundary" >/dev/null || {
  echo "expected database boundary is not valid JSON" >&2
  exit 1
}

now_epoch="$(date -u +%s)"
jq -e \
  --arg run_id "$run_id" \
  --arg source_revision "$source_revision" \
  --arg region "$region" \
  --arg profile "$profile" \
  --arg environment "$environment" \
  --argjson database_boundary "$database_boundary" \
  --arg resource_allowlist "$resource_allowlist" \
  --arg cleanup_blast_radius "$cleanup_blast_radius" \
  --argjson now_epoch "$now_epoch" \
  '
    keys == [
      "approved_at",
      "approved_by",
      "authority_kind",
      "aws_profile",
      "cleanup_blast_radius",
      "database_boundary",
      "environment",
      "expected_account_id",
      "expected_region",
      "expected_role_arn",
      "expires_at",
      "max_duration_minutes",
      "max_spend_usd",
      "resource_allowlist",
      "run_id",
      "schema_version",
      "source_revision"
    ]
    and .schema_version == 1
    and .authority_kind == "minco.aws-controller-rehearsal.v1"
    and .run_id == $run_id
    and .source_revision == $source_revision
    and (.source_revision | test("^[0-9a-f]{40}([0-9a-f]{24})?$"))
    and .expected_region == $region
    and .aws_profile == $profile
    and .environment == $environment
    and .database_boundary == $database_boundary
    and .resource_allowlist == $resource_allowlist
    and .cleanup_blast_radius == $cleanup_blast_radius
    and (
      (
        $resource_allowlist == "bounded-direct-smoke-v1"
        and $cleanup_blast_radius == "cleanup-bounded-direct-smoke-v1"
        and .database_boundary.mode == "existing-ssm-secure-string"
        and (.database_boundary | keys) == [
          "instance_owned",
          "mode",
          "parameter_name",
          "parameter_owned"
        ]
        and .database_boundary.parameter_owned == false
        and .database_boundary.instance_owned == false
        and (.database_boundary.parameter_name | test("^/[A-Za-z0-9_./-]+$"))
        and (.database_boundary.parameter_name | contains("//") | not)
        and (.database_boundary.parameter_name | endswith("/") | not)
      )
      or (
        $resource_allowlist == "bounded-root-bootstrap-v1"
        and $cleanup_blast_radius == "cleanup-bounded-root-bootstrap-v1"
        and .database_boundary.mode == "run-owned-ssm-copy"
        and (.database_boundary.parameter_name | test("^/[A-Za-z0-9_./-]+$"))
        and (.database_boundary.parameter_name | contains("//") | not)
        and (.database_boundary.parameter_name | endswith("/") | not)
        and (
          (
            .database_boundary.source_kind == "process-environment"
            and (.database_boundary | keys) == [
              "mode",
              "parameter_name",
              "source_environment_variable",
              "source_kind"
            ]
            and .database_boundary.source_environment_variable == "MINCO_DATABASE_URL"
          )
          or (
            .database_boundary.source_kind == "local-mode-0600-file"
            and (.database_boundary | keys) == [
              "mode",
              "parameter_name",
              "source_file",
              "source_kind"
            ]
            and (.database_boundary.source_file | type == "string" and startswith("/"))
          )
          or (
            .database_boundary.source_kind == "ssm-secure-string"
            and (.database_boundary | keys) == [
              "mode",
              "parameter_name",
              "source_kind",
              "source_parameter_name"
            ]
            and (.database_boundary.source_parameter_name | test("^/minco/[A-Za-z0-9_./-]+$"))
            and (.database_boundary.source_parameter_name | contains("//") | not)
            and (.database_boundary.source_parameter_name | endswith("/") | not)
          )
        )
      )
      or (
        $resource_allowlist == "bounded-root-temp-rds-v1"
        and $cleanup_blast_radius == "cleanup-bounded-root-temp-rds-v1"
        and .database_boundary.mode == "disposable-rds"
        and (.database_boundary | keys) == [
          "instance_id",
          "mode",
          "parameter_name",
          "rds_stack_name"
        ]
        and (.database_boundary.rds_stack_name | test("^[A-Za-z][A-Za-z0-9-]{0,47}$"))
        and (.database_boundary.instance_id | test("^[a-z][a-z0-9-]{0,47}$"))
        and (.database_boundary.parameter_name | test("^/[A-Za-z0-9_./-]+$"))
        and (.database_boundary.parameter_name | contains("//") | not)
        and (.database_boundary.parameter_name | endswith("/") | not)
      )
    )
    and (.expected_account_id | type == "string" and test("^[0-9]{12}$"))
    and (
      .expected_role_arn
      | type == "string"
        and test("^arn:aws(-us-gov|-cn)?:iam::[0-9]{12}:role/[A-Za-z0-9+=,.@_/-]{1,512}$")
    )
    and (
      .expected_account_id as $account
      | .expected_role_arn
      | contains("::" + $account + ":role/")
    )
    and (.max_duration_minutes | type == "number" and floor == . and . >= 1 and . <= 60)
    and (.max_spend_usd | type == "number" and . > 0 and . <= 25)
    and (.approved_by | type == "string" and test("^[A-Za-z0-9][A-Za-z0-9._@ -]{0,127}$"))
    and (.approved_at | type == "string" and test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$"))
    and (.expires_at | type == "string" and test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$"))
    and ((.approved_at | fromdateiso8601) <= ($now_epoch + 300))
    and ((.expires_at | fromdateiso8601) >= $now_epoch)
    and ((.expires_at | fromdateiso8601) >= (.approved_at | fromdateiso8601))
    and (((.expires_at | fromdateiso8601) - (.approved_at | fromdateiso8601)) <= 86400)
  ' "$authority_file" >/dev/null || {
  echo "rehearsal authority is missing, stale, broader than policy, or does not match this exact run" >&2
  exit 1
}
