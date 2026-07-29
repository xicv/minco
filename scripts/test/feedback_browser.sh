#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

plugin_dir="plugins/minco-plugin-feedback"
install_args=(--only-shell chromium firefox)

if [[ "$(uname -s)" == "Linux" ]]; then
  install_args=(--with-deps "${install_args[@]}")
fi

npm ci --prefix "$plugin_dir"
"$plugin_dir/node_modules/.bin/playwright" install "${install_args[@]}"
npm run --prefix "$plugin_dir" test:browser
