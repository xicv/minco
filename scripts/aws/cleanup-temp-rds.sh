#!/usr/bin/env bash
set -euo pipefail

MINCO_REHEARSAL_CLEANUP_MODE=true
export MINCO_REHEARSAL_CLEANUP_MODE

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
repo_root="$(minco_repo_root)"
cd "$repo_root"

for command in aws jq; do
  require_command "$command"
done
: "${AWS_REGION:=ap-southeast-2}"
: "${MINCO_RDS_STACK_NAME:?set MINCO_RDS_STACK_NAME}"
: "${MINCO_RDS_INSTANCE_ID:?set MINCO_RDS_INSTANCE_ID}"
initialize_cloud_journal

failure=0
set +e

if [[ -f "$MINCO_AWS_EVIDENCE_DIR/lambda-security-group-id.txt" ]]; then
  lambda_security_group_id="$(<"$MINCO_AWS_EVIDENCE_DIR/lambda-security-group-id.txt")"
  lambda_network_interfaces_absent=false
  for attempt in {1..30}; do
    lambda_network_interface_count="$(
      aws_logged ec2 describe-network-interfaces \
        "wait for Lambda VPC network interfaces to release the temporary security group; attempt $attempt" \
        --filters "Name=group-id,Values=$lambda_security_group_id" \
        --query 'length(NetworkInterfaces)' \
        --output text
    )"
    if [[ "$lambda_network_interface_count" == "0" ]]; then
      lambda_network_interfaces_absent=true
      break
    fi
    sleep 10
  done
  if [[ "$lambda_network_interfaces_absent" == "false" ]]; then
    echo "Lambda network interfaces still use the temporary database security boundary" >&2
    failure=1
  fi
fi

stack_error="$MINCO_AWS_EVIDENCE_DIR/rds-stack-cleanup-error.txt"
if aws_logged cloudformation describe-stacks \
  "check whether temporary PostgreSQL stack requires cleanup" \
  --stack-name "$MINCO_RDS_STACK_NAME" \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/rds-stack-before-cleanup.json" \
  2>"$stack_error"; then
  if jq -e \
    --arg stack "$MINCO_RDS_STACK_NAME" \
    --arg run_id "$MINCO_AWS_RUN_ID" \
    '.Stacks[0].StackName == $stack
     and (
       .Stacks[0].Tags
       | from_entries
       | .["minco:managed"] == "true"
         and .["minco:purpose"] == "bounded-smoke"
         and .["minco:run-id"] == $run_id
     )' \
    "$MINCO_AWS_EVIDENCE_DIR/rds-stack-before-cleanup.json" >/dev/null; then
    aws_logged cloudformation delete-stack \
      "delete exact tagged temporary PostgreSQL, managed secret, SSM endpoint and isolated VPC" \
      --stack-name "$MINCO_RDS_STACK_NAME"
    if ! aws_logged cloudformation wait \
      "wait for complete removal of temporary PostgreSQL and its network" \
      stack-delete-complete \
      --stack-name "$MINCO_RDS_STACK_NAME"; then
      aws_logged cloudformation describe-stack-events \
        "retain failed temporary PostgreSQL cleanup events" \
        --stack-name "$MINCO_RDS_STACK_NAME" \
        --output json >"$MINCO_AWS_EVIDENCE_DIR/rds-stack-delete-events.json"
      failure=1
    fi
  else
    echo "refusing to delete a temporary PostgreSQL stack without exact run ownership tags" >&2
    failure=1
  fi
elif ! grep -Eq 'does not exist' "$stack_error"; then
  echo "could not determine temporary PostgreSQL stack state" >&2
  sed -n '1,8p' "$stack_error" >&2
  failure=1
fi
rm -f "$stack_error"

stack_absent=false
stack_verify_error="$MINCO_AWS_EVIDENCE_DIR/rds-stack-verify-error.txt"
if ! aws_logged cloudformation describe-stacks \
  "verify temporary PostgreSQL stack is absent" \
  --stack-name "$MINCO_RDS_STACK_NAME" >/dev/null 2>"$stack_verify_error" &&
  grep -Eq 'does not exist' "$stack_verify_error"; then
  stack_absent=true
fi
rm -f "$stack_verify_error"

database_absent=false
database_verify_error="$MINCO_AWS_EVIDENCE_DIR/rds-instance-verify-error.txt"
if ! aws_logged rds describe-db-instances \
  "verify temporary PostgreSQL instance is absent" \
  --db-instance-identifier "$MINCO_RDS_INSTANCE_ID" >/dev/null 2>"$database_verify_error" &&
  grep -Eq 'DBInstanceNotFound|not found' "$database_verify_error"; then
  database_absent=true
fi
rm -f "$database_verify_error"

secret_absent=true
if [[ -f "$MINCO_AWS_EVIDENCE_DIR/rds-master-secret-arn.txt" ]]; then
  master_secret_arn="$(<"$MINCO_AWS_EVIDENCE_DIR/rds-master-secret-arn.txt")"
  secret_absent=false
  secret_error="$MINCO_AWS_EVIDENCE_DIR/rds-secret-verify-error.txt"
  if aws_logged secretsmanager describe-secret \
    "check whether the RDS-managed temporary master secret requires explicit cleanup" \
    --secret-id "$master_secret_arn" >/dev/null 2>"$secret_error"; then
    aws_logged secretsmanager delete-secret \
      "force-delete the orphaned temporary RDS master secret after database removal" \
      --secret-id "$master_secret_arn" \
      --force-delete-without-recovery >/dev/null
    for attempt in {1..15}; do
      if ! aws_logged secretsmanager describe-secret \
        "verify temporary RDS master secret is absent; attempt $attempt" \
        --secret-id "$master_secret_arn" >/dev/null 2>"$secret_error" &&
        grep -Eq 'ResourceNotFoundException|not found' "$secret_error"; then
        secret_absent=true
        break
      fi
      sleep 2
    done
  elif grep -Eq 'ResourceNotFoundException|not found' "$secret_error"; then
    secret_absent=true
  fi
  rm -f "$secret_error"
fi

vpc_absent=true
if [[ -f "$MINCO_AWS_EVIDENCE_DIR/rds-vpc-id.txt" ]]; then
  smoke_vpc_id="$(<"$MINCO_AWS_EVIDENCE_DIR/rds-vpc-id.txt")"
  vpc_absent=false
  vpc_error="$MINCO_AWS_EVIDENCE_DIR/rds-vpc-verify-error.txt"
  if vpc_count="$(
    aws_logged ec2 describe-vpcs \
      "verify the isolated temporary PostgreSQL VPC is absent" \
      --vpc-ids "$smoke_vpc_id" \
      --query 'length(Vpcs)' \
      --output text 2>"$vpc_error"
  )" && [[ "$vpc_count" == "0" ]]; then
    vpc_absent=true
  elif grep -Eq 'InvalidVpcID.NotFound|does not exist' "$vpc_error"; then
    vpc_absent=true
  fi
  rm -f "$vpc_error"
fi

database_secret_files_absent=false
rm -f \
  "$MINCO_AWS_EVIDENCE_DIR/database-url-lambda.txt" \
  "$MINCO_AWS_EVIDENCE_DIR/rds-ca-bundle.pem"
if [[ ! -e "$MINCO_AWS_EVIDENCE_DIR/database-url-lambda.txt" &&
  ! -e "$MINCO_AWS_EVIDENCE_DIR/rds-ca-bundle.pem" ]]; then
  database_secret_files_absent=true
fi

synthetic_data_absent=false
if [[ "$database_absent" == true ]]; then
  synthetic_data_absent=true
  write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/database-cleanup-complete.txt" true
fi

jq -n \
  --argjson stack_absent "$stack_absent" \
  --argjson database_absent "$database_absent" \
  --argjson secret_absent "$secret_absent" \
  --argjson vpc_absent "$vpc_absent" \
  --argjson database_secret_files_absent "$database_secret_files_absent" \
  --argjson synthetic_data_absent "$synthetic_data_absent" \
  '{
    temporary_database_stack_absent: $stack_absent,
    temporary_database_instance_absent: $database_absent,
    temporary_database_master_secret_absent: $secret_absent,
    temporary_database_vpc_absent: $vpc_absent,
    local_database_secret_files_absent: $database_secret_files_absent,
    synthetic_database_data_absent: $synthetic_data_absent
  }' >"$MINCO_AWS_EVIDENCE_DIR/rds-cleanup.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/rds-cleanup.json"

if ! jq -e '[.[]] | all' "$MINCO_AWS_EVIDENCE_DIR/rds-cleanup.json" >/dev/null; then
  jq . "$MINCO_AWS_EVIDENCE_DIR/rds-cleanup.json" >&2
  failure=1
fi
((failure == 0)) || exit 1
printf 'Verified temporary PostgreSQL and VPC cleanup for run %s\n' "$MINCO_AWS_RUN_ID"
