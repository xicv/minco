#!/usr/bin/env bash
set -euo pipefail

proof_root="$(cd "$(dirname "$0")/.." && pwd)"
template="$proof_root/aws/template.yaml"
repository_root="$(cd "$proof_root/../.." && pwd)"

cd "$repository_root"
uv run --locked python "$proof_root/scripts/check_aws_template.py" "$template"
sam validate --lint --region ap-southeast-2 --template-file "$template"

if rg -n 'AWS::EC2::NatGateway|ProvisionedConcurrency|AWS::ECS|AWS::EC2::Instance|AWS::RDS::DBInstance' "$template"; then
  echo "forbidden fixed-cost resource found in proof template" >&2
  exit 1
fi

rg -q 'BillingMode: PAY_PER_REQUEST' "$template"
rg -q 'ReservedConcurrentExecutions: 5' "$template"
rg -q 'RetentionInDays: 1' "$template"
rg -q 'DeletionPolicy: Delete' "$template"
rg -q 'execute-api:ManageConnections' "$template"
