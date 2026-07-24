#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
command -v cargo-lambda >/dev/null || { echo 'cargo-lambda is required' >&2; exit 1; }
cargo lambda build --release --arm64 --output-format zip \
  -p orders-service --bin orders-lambda --no-default-features --features lambda
artifact=target/lambda/orders-lambda/bootstrap.zip
[[ -f "$artifact" ]] || { echo "missing $artifact" >&2; exit 1; }
printf 'Built %s (%s bytes)\n' "$artifact" "$(wc -c < "$artifact")"
