#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
: "${MINCO_STACK_NAME:?set MINCO_STACK_NAME}"
: "${MINCO_DATABASE_URL_PARAMETER:?set MINCO_DATABASE_URL_PARAMETER}"
: "${AWS_REGION:=ap-southeast-2}"
scripts/aws/build-lambda.sh
scripts/aws/plan.sh "${MINCO_DEPLOYMENT_CONFIG:-examples/orders/config/minco.dev.toml}"
scripts/aws/validate.sh
sam deploy \
  --template-file infra/aws/generated/template.yaml \
  --stack-name "$MINCO_STACK_NAME" \
  --region "$AWS_REGION" \
  --capabilities CAPABILITY_IAM \
  --resolve-s3 \
  --parameter-overrides "DatabaseUrlParameterName=$MINCO_DATABASE_URL_PARAMETER" \
  --no-fail-on-empty-changeset
