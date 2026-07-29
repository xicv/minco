#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
# shellcheck source=scripts/aws/lib/common.sh
source scripts/aws/lib/common.sh
command -v cargo-lambda >/dev/null || { echo 'cargo-lambda is required' >&2; exit 1; }
cargo lambda build --release --arm64 --output-format zip \
  -p orders-service --bin orders-lambda --no-default-features --features lambda \
  --locked
artifact=target/lambda/orders-lambda/bootstrap.zip
[[ -f "$artifact" ]] || { echo "missing $artifact" >&2; exit 1; }
if [[ -n "${MINCO_RDS_CA_BUNDLE:-}" ]]; then
  for command in cp mktemp touch unzip zip; do
    command -v "$command" >/dev/null || {
      printf '%s is required to package the RDS CA bundle\n' "$command" >&2
      exit 1
    }
  done
  [[ -f "$MINCO_RDS_CA_BUNDLE" && ! -L "$MINCO_RDS_CA_BUNDLE" ]] || {
    echo "MINCO_RDS_CA_BUNDLE must be a regular non-symlink file" >&2
    exit 1
  }
  grep -q -- '-----BEGIN CERTIFICATE-----' "$MINCO_RDS_CA_BUNDLE" || {
    echo "MINCO_RDS_CA_BUNDLE does not contain a PEM certificate" >&2
    exit 1
  }
  bundle_directory="$(mktemp -d /tmp/minco-rds-bundle.XXXXXX)"
  cleanup_bundle() {
    rm -f "$bundle_directory/rds-ca-bundle.pem"
    rmdir "$bundle_directory" >/dev/null 2>&1 || true
  }
  trap cleanup_bundle EXIT
  cp "$MINCO_RDS_CA_BUNDLE" "$bundle_directory/rds-ca-bundle.pem"
  touch -t 198001010000 "$bundle_directory/rds-ca-bundle.pem"
  zip -X -q -j "$artifact" "$bundle_directory/rds-ca-bundle.pem"
  unzip -Z1 "$artifact" | grep -qx 'rds-ca-bundle.pem'
  cleanup_bundle
  trap - EXIT
fi
normalize_lambda_zip "$artifact"
printf 'Built %s (%s bytes)\n' "$artifact" "$(wc -c < "$artifact")"
