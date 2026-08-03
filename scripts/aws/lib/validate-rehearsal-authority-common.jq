(
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
