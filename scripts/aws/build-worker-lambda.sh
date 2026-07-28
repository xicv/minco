#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
# shellcheck source=scripts/aws/lib/common.sh
source scripts/aws/lib/common.sh

command -v cargo-lambda >/dev/null || {
  echo 'cargo-lambda is required' >&2
  exit 1
}
cargo lambda build --release --arm64 --output-format zip \
  -p minco-aws-worker --example sqs_worker --locked
artifact=target/lambda/sqs_worker/bootstrap.zip
[[ -f "$artifact" ]] || { echo "missing $artifact" >&2; exit 1; }
normalize_lambda_zip "$artifact"
printf 'Built %s (%s bytes)\n' "$artifact" "$(wc -c < "$artifact")"
