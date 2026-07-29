#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."
# shellcheck source=scripts/aws/lib/common.sh
source scripts/aws/lib/common.sh

for parameter_name in \
  /minco/smoke/abc123/database-url \
  /minco/service.name/database_url \
  /minco/service-name/database.url; do
  normalized_ssm_parameter_name "$parameter_name"
done

for parameter_name in \
  minco/smoke/abc123/database-url \
  /minco//database-url \
  /minco/database-url/ \
  "/minco/database url"; do
  if normalized_ssm_parameter_name "$parameter_name"; then
    printf 'accepted invalid SSM parameter name\n' >&2
    exit 1
  fi
done

printf 'AWS shell portability checks passed.\n'
