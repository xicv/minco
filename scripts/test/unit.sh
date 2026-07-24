#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
exec cargo test --workspace --lib --all-features
