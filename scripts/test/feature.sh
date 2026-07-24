#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
exec cargo test -p orders-domain -p orders-application -p orders-adapters -p orders-api --all-features
