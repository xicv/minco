#!/usr/bin/env bash
set -euo pipefail

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
repo_root="$(minco_repo_root)"
cd "$repo_root"

for command in aws awk cargo curl grep jq psql shasum sort tail tr; do
  require_command "$command"
done
: "${AWS_REGION:=ap-southeast-2}"
: "${MINCO_RDS_STACK_NAME:?set MINCO_RDS_STACK_NAME}"
: "${MINCO_RDS_INSTANCE_ID:?set MINCO_RDS_INSTANCE_ID}"
: "${MINCO_DATABASE_URL_PARAMETER:?set MINCO_DATABASE_URL_PARAMETER}"
initialize_cloud_journal

template="infra/aws/smoke/temporary-postgres.yaml"
[[ -f "$template" ]]
require_safe_name "MINCO_RDS_STACK_NAME" "$MINCO_RDS_STACK_NAME"
require_safe_name "MINCO_RDS_INSTANCE_ID" "$MINCO_RDS_INSTANCE_ID"

record_cloud_touch \
  "external:network" \
  "discover migration IPv4" \
  "resolve this Mac's current public IPv4 through the AWS checkip endpoint; no credential"
local_ip="$(curl -4 --fail --silent --show-error --max-time 20 https://checkip.amazonaws.com)"
local_ip="${local_ip//[[:space:]]/}"
if ! [[ "$local_ip" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]]; then
  echo "AWS checkip did not return an IPv4 address" >&2
  exit 1
fi
IFS=. read -r octet1 octet2 octet3 octet4 <<<"$local_ip"
for octet in "$octet1" "$octet2" "$octet3" "$octet4"; do
  ((octet >= 0 && octet <= 255)) || {
    echo "AWS checkip returned an invalid IPv4 address" >&2
    exit 1
  }
done
local_cidr="$local_ip/32"
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/local-migration-cidr.txt" "$local_cidr"

engine_versions="$(
  aws_logged rds describe-orderable-db-instance-options \
    "select a currently orderable PostgreSQL 17 engine for the temporary db.t4g.micro instance" \
    --engine postgres \
    --db-instance-class db.t4g.micro \
    --query 'OrderableDBInstanceOptions[].EngineVersion' \
    --output text
)"
engine_version="$(
  tr '\t' '\n' <<<"$engine_versions" |
    grep -E '^17\.[0-9]+$' |
    sort -V |
    tail -1
)"
[[ -n "$engine_version" ]] || {
  echo "no orderable PostgreSQL 17 db.t4g.micro engine is available in $AWS_REGION" >&2
  exit 1
}
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/rds-engine-version.txt" "$engine_version"

rds_stack_error="$MINCO_AWS_EVIDENCE_DIR/rds-stack-preflight-error.txt"
if aws_logged cloudformation describe-stacks \
  "ensure temporary PostgreSQL stack $MINCO_RDS_STACK_NAME does not pre-exist" \
  --stack-name "$MINCO_RDS_STACK_NAME" >/dev/null 2>"$rds_stack_error"; then
  echo "refusing to mutate pre-existing stack $MINCO_RDS_STACK_NAME" >&2
  exit 1
elif ! grep -Eq 'does not exist' "$rds_stack_error"; then
  echo "could not prove that temporary PostgreSQL stack $MINCO_RDS_STACK_NAME is absent" >&2
  sed -n '1,8p' "$rds_stack_error" >&2
  exit 1
fi
rm -f "$rds_stack_error"
write_evidence_value \
  "$MINCO_AWS_EVIDENCE_DIR/rds-stack-preflight-absent.txt" \
  "$MINCO_RDS_STACK_NAME"

aws_logged cloudformation validate-template \
  "validate the disposable RDS/VPC smoke template before mutation" \
  --template-body "file://$template" \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/rds-template-validation.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/rds-template-validation.json"

aws_logged cloudformation create-change-set \
  "create a reviewable change set for temporary encrypted PostgreSQL and private Lambda networking" \
  --stack-name "$MINCO_RDS_STACK_NAME" \
  --change-set-name "$MINCO_RDS_STACK_NAME-create" \
  --change-set-type CREATE \
  --template-body "file://$template" \
  --parameters \
  "ParameterKey=RunId,ParameterValue=$MINCO_AWS_RUN_ID" \
  "ParameterKey=DatabaseInstanceIdentifier,ParameterValue=$MINCO_RDS_INSTANCE_ID" \
  "ParameterKey=DatabaseEngineVersion,ParameterValue=$engine_version" \
  "ParameterKey=LocalMigrationCidr,ParameterValue=$local_cidr" \
  "ParameterKey=DatabaseParameterName,ParameterValue=$MINCO_DATABASE_URL_PARAMETER" \
  --tags \
  Key=minco:managed,Value=true \
  Key=minco:purpose,Value=bounded-smoke \
  Key=minco:run-id,Value="$MINCO_AWS_RUN_ID" >/dev/null
aws_logged cloudformation wait \
  "wait for the temporary PostgreSQL change set to become reviewable" \
  change-set-create-complete \
  --stack-name "$MINCO_RDS_STACK_NAME" \
  --change-set-name "$MINCO_RDS_STACK_NAME-create"
aws_logged cloudformation describe-change-set \
  "retain and inspect the temporary PostgreSQL create-only change set" \
  --stack-name "$MINCO_RDS_STACK_NAME" \
  --change-set-name "$MINCO_RDS_STACK_NAME-create" \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/rds-change-set.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/rds-change-set.json"
jq -e '
  .Status == "CREATE_COMPLETE"
  and (.Changes | length > 0)
  and ([.Changes[].ResourceChange.Action] | all(. == "Add"))
  and (
    [.Changes[].ResourceChange.ResourceType]
    | all(
        . == "AWS::EC2::InternetGateway"
        or . == "AWS::EC2::Route"
        or . == "AWS::EC2::RouteTable"
        or . == "AWS::EC2::SecurityGroup"
        or . == "AWS::EC2::SecurityGroupEgress"
        or . == "AWS::EC2::SecurityGroupIngress"
        or . == "AWS::EC2::Subnet"
        or . == "AWS::EC2::SubnetRouteTableAssociation"
        or . == "AWS::EC2::VPC"
        or . == "AWS::EC2::VPCEndpoint"
        or . == "AWS::EC2::VPCGatewayAttachment"
        or . == "AWS::RDS::DBInstance"
        or . == "AWS::RDS::DBSubnetGroup"
      )
  )
' "$MINCO_AWS_EVIDENCE_DIR/rds-change-set.json" >/dev/null || {
  echo "temporary PostgreSQL change set exceeded its reviewed resource boundary" >&2
  exit 1
}

aws_logged cloudformation execute-change-set \
  "execute the reviewed temporary PostgreSQL create-only change set" \
  --stack-name "$MINCO_RDS_STACK_NAME" \
  --change-set-name "$MINCO_RDS_STACK_NAME-create" >/dev/null
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/rds-stack-created.txt" true
if ! aws_logged cloudformation wait \
  "wait for temporary PostgreSQL and its private network to become available" \
  stack-create-complete \
  --stack-name "$MINCO_RDS_STACK_NAME"; then
  aws_logged cloudformation describe-stack-events \
    "retain temporary PostgreSQL stack failure events before cleanup" \
    --stack-name "$MINCO_RDS_STACK_NAME" \
    --output json >"$MINCO_AWS_EVIDENCE_DIR/rds-stack-create-events.json" || true
  chmod 600 "$MINCO_AWS_EVIDENCE_DIR/rds-stack-create-events.json" 2>/dev/null || true
  echo "temporary PostgreSQL stack creation failed; retained CloudFormation events" >&2
  exit 1
fi
aws_logged cloudformation describe-stacks \
  "retain temporary PostgreSQL stack outputs and status" \
  --stack-name "$MINCO_RDS_STACK_NAME" \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/rds-stack.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/rds-stack.json"
aws_logged cloudformation list-stack-resources \
  "retain every temporary PostgreSQL physical resource identifier" \
  --stack-name "$MINCO_RDS_STACK_NAME" \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/rds-stack-resources.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/rds-stack-resources.json"

stack_output() {
  local key="$1"
  jq -er \
    --arg key "$key" \
    '.Stacks[0].Outputs[] | select(.OutputKey == $key).OutputValue' \
    "$MINCO_AWS_EVIDENCE_DIR/rds-stack.json"
}

database_endpoint="$(stack_output DatabaseEndpoint)"
database_security_group_id="$(stack_output DatabaseSecurityGroupId)"
lambda_security_group_id="$(stack_output LambdaSecurityGroupId)"
lambda_subnet_ids="$(stack_output LambdaSubnetIds)"
master_secret_arn="$(stack_output MasterUserSecretArn)"
smoke_vpc_id="$(stack_output SmokeVpcId)"
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/rds-instance-id.txt" "$MINCO_RDS_INSTANCE_ID"
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/rds-security-group-id.txt" "$database_security_group_id"
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/lambda-security-group-id.txt" "$lambda_security_group_id"
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/lambda-subnet-ids.txt" "$lambda_subnet_ids"
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/rds-master-secret-arn.txt" "$master_secret_arn"
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/rds-vpc-id.txt" "$smoke_vpc_id"

ca_bundle="$MINCO_AWS_EVIDENCE_DIR/rds-ca-bundle.pem"
record_cloud_touch \
  "external:aws-rds-truststore" \
  "download regional CA bundle" \
  "download the official $AWS_REGION RDS CA bundle over verified HTTPS"
curl \
  --fail \
  --silent \
  --show-error \
  --proto '=https' \
  --tlsv1.2 \
  --max-time 30 \
  "https://truststore.pki.rds.amazonaws.com/$AWS_REGION/$AWS_REGION-bundle.pem" \
  --output "$ca_bundle"
chmod 600 "$ca_bundle"
grep -q -- '-----BEGIN CERTIFICATE-----' "$ca_bundle"
write_evidence_value \
  "$MINCO_AWS_EVIDENCE_DIR/rds-ca-bundle.sha256" \
  "$(shasum -a 256 "$ca_bundle" | awk '{print $1}')"

secret_json="$(
  aws_logged secretsmanager get-secret-value \
    "retrieve the RDS-managed temporary master credential for migration; value redacted" \
    --secret-id "$master_secret_arn" \
    --query SecretString \
    --output text
)"
database_username="$(jq -er '.username' <<<"$secret_json")"
database_password="$(jq -er '.password' <<<"$secret_json")"
unset secret_json
encoded_username="$(jq -rn --arg value "$database_username" '$value | @uri')"
encoded_password="$(jq -rn --arg value "$database_password" '$value | @uri')"
encoded_local_ca="$(jq -rn --arg value "$ca_bundle" '$value | @uri')"
encoded_lambda_ca="$(jq -rn --arg value '/var/task/rds-ca-bundle.pem' '$value | @uri')"
migration_url="postgresql://$encoded_username:$encoded_password@$database_endpoint:5432/minco?sslmode=verify-full&sslrootcert=$encoded_local_ca"
lambda_url="postgresql://$encoded_username:$encoded_password@$database_endpoint:5432/minco?sslmode=verify-full&sslrootcert=$encoded_lambda_ca"
unset encoded_username encoded_password

record_external_database_touch \
  "explicit migration" \
  "apply release migrations to the temporary encrypted RDS PostgreSQL instance over TLS verify-full; URL redacted"
migration_plan="$MINCO_AWS_EVIDENCE_DIR/database-migration-plan.json"
cargo minco db plan --set orders-postgres --json >"$migration_plan"
migration_digest="$(jq -er '.digest' "$migration_plan")"
MIGRATION_DATABASE_URL="$migration_url" \
  cargo minco db migrate \
    --set orders-postgres \
    --database-url-env MIGRATION_DATABASE_URL \
    --expected-plan-digest "$migration_digest" \
    --receipt "target/minco/aws/$MINCO_AWS_RUN_ID/database-migration-receipt.json" \
    --json >"$MINCO_AWS_EVIDENCE_DIR/database-migration-output.json"
MIGRATION_DATABASE_URL="$migration_url" \
  cargo minco db verify \
    --set orders-postgres \
    --database-url-env MIGRATION_DATABASE_URL \
    --json >"$MINCO_AWS_EVIDENCE_DIR/database-migration-verification.json"
unset migration_digest
record_external_database_touch \
  "migration verification" \
  "verify TLS and the orders schema on temporary RDS PostgreSQL; URL redacted"
PGCONNECT_TIMEOUT=15 \
PGHOST="$database_endpoint" \
PGPORT=5432 \
PGUSER="$database_username" \
PGPASSWORD="$database_password" \
PGDATABASE=minco \
PGSSLMODE=verify-full \
PGSSLROOTCERT="$ca_bundle" \
  psql \
  --no-psqlrc \
  --quiet \
  --tuples-only \
  --no-align \
  --set ON_ERROR_STOP=1 \
  --command "SELECT current_setting('ssl'), to_regclass('public.orders') IS NOT NULL" \
  >"$MINCO_AWS_EVIDENCE_DIR/rds-migration-verification.txt"
grep -qx 'on|t' "$MINCO_AWS_EVIDENCE_DIR/rds-migration-verification.txt"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/rds-migration-verification.txt"
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/database-migration-complete.txt" true
unset database_password database_username migration_url

aws_logged ec2 revoke-security-group-ingress \
  "remove this Mac's temporary PostgreSQL /32 immediately after migration" \
  --group-id "$database_security_group_id" \
  --protocol tcp \
  --port 5432 \
  --cidr "$local_cidr" >/dev/null
aws_logged rds modify-db-instance \
  "remove the temporary database public IP before Lambda deployment" \
  --db-instance-identifier "$MINCO_RDS_INSTANCE_ID" \
  --no-publicly-accessible \
  --apply-immediately >/dev/null
rds_private=false
for attempt in {1..60}; do
  aws_logged rds describe-db-instances \
    "wait for proof that temporary PostgreSQL is private and available; attempt $attempt" \
    --db-instance-identifier "$MINCO_RDS_INSTANCE_ID" \
    --query 'DBInstances[0].{Identifier:DBInstanceIdentifier,Engine:Engine,EngineVersion:EngineVersion,Class:DBInstanceClass,Status:DBInstanceStatus,StorageEncrypted:StorageEncrypted,AllocatedStorage:AllocatedStorage,MultiAZ:MultiAZ,PubliclyAccessible:PubliclyAccessible,PendingPubliclyAccessible:PendingModifiedValues.PubliclyAccessible,DeletionProtection:DeletionProtection,BackupRetentionPeriod:BackupRetentionPeriod,Endpoint:Endpoint.Address}' \
    --output json >"$MINCO_AWS_EVIDENCE_DIR/rds-private.json"
  if jq -e '
    .Status == "available"
    and .PubliclyAccessible == false
    and .PendingPubliclyAccessible == null
  ' "$MINCO_AWS_EVIDENCE_DIR/rds-private.json" >/dev/null; then
    rds_private=true
    break
  fi
  sleep 10
done
[[ "$rds_private" == true ]] || {
  echo "temporary PostgreSQL did not become private within ten minutes" >&2
  exit 1
}
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/rds-private.json"
jq -e '
  .Engine == "postgres"
  and .Status == "available"
  and .Class == "db.t4g.micro"
  and .StorageEncrypted == true
  and .AllocatedStorage == 20
  and .MultiAZ == false
  and .PubliclyAccessible == false
  and .PendingPubliclyAccessible == null
  and .DeletionProtection == false
  and .BackupRetentionPeriod == 0
' "$MINCO_AWS_EVIDENCE_DIR/rds-private.json" >/dev/null
aws_logged ec2 describe-security-groups \
  "prove database ingress is limited to the temporary Lambda security group" \
  --group-ids "$database_security_group_id" \
  --query 'SecurityGroups[0].IpPermissions' \
  --output json >"$MINCO_AWS_EVIDENCE_DIR/rds-ingress-private.json"
chmod 600 "$MINCO_AWS_EVIDENCE_DIR/rds-ingress-private.json"
jq -e \
  --arg lambda_group "$lambda_security_group_id" \
  'length == 1
   and .[0].IpProtocol == "tcp"
   and .[0].FromPort == 5432
   and .[0].ToPort == 5432
   and (.[0].IpRanges | length) == 0
   and (.[0].Ipv6Ranges | length) == 0
   and (.[0].UserIdGroupPairs | map(.GroupId)) == [$lambda_group]' \
  "$MINCO_AWS_EVIDENCE_DIR/rds-ingress-private.json" >/dev/null

write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/database-url-lambda.txt" "$lambda_url"
unset lambda_url
printf 'Temporary PostgreSQL is migrated and private: %s\n' "$MINCO_RDS_INSTANCE_ID"
