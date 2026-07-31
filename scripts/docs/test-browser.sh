#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

npm ci --prefix docs-site
npm --prefix docs-site exec playwright install chromium
npm --prefix docs-site run test:browser
