#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

npm ci --prefix docs-site
npm audit --prefix docs-site --audit-level=moderate
npm --prefix docs-site run build
