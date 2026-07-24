#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

version="$(python3 - <<'PY'
import tomllib
print(tomllib.load(open('Cargo.toml','rb'))['workspace']['package']['version'])
PY
)"
output="${1:-../minco-framework-${version}.zip}"

mkdir -p verification
find . -type d -name __pycache__ -prune -exec rm -rf {} +
find . -type f -name '*.pyc' -delete

python3 scripts/generate_bootstrap_artifacts.py
python3 scripts/test/sqlite_schema.py > verification/sqlite-schema.txt
python3 scripts/test/scaffold_templates.py > verification/scaffold-templates.json
python3 scripts/test/feedback_contract.py > verification/feedback-contract.json
node --check plugins/minco-plugin-feedback/assets/widget.js > verification/widget-js.txt 2>&1
python3 scripts/validate_static.py --output verification/static-validation.json >/dev/null
python3 scripts/validate_publish.py --output verification/publish-validation.json >/dev/null
python3 scripts/deep_review.py >/dev/null
cp target/minco/deep-review.json verification/deep-review.json
python3 scripts/test/feedback_contract.py > verification/feedback-contract.json
node --check plugins/minco-plugin-feedback/assets/widget.js
printf '%s\n' 'Feedback widget JavaScript passed node --check.' > verification/widget-node-check.txt
git diff --check
printf '%s\n' 'Working-tree diff passed git diff --check.' > verification/git-diff-check.txt
python3 -m py_compile $(find scripts -type f -name '*.py' | sort)
printf '%s
' 'All repository Python scripts compiled successfully.' > verification/python-compile.txt
find scripts -type d -name __pycache__ -prune -exec rm -rf {} +
find scripts -type f -name '*.pyc' -delete
while IFS= read -r script; do bash -n "$script"; done < <(find scripts -type f -name '*.sh' | sort)
printf '%s
' 'All repository shell scripts passed bash -n.' > verification/shell-syntax.txt
python3 scripts/source_manifest.py >/dev/null
python3 -m json.tool verification/source-manifest.json >/dev/null

rm -f "$output" "$output.sha256"
zip -qr "$output" . \
  -x '.git/*' '.jj/*' 'target/*' '*.zip' '.env' '*.db' '*.sqlite' \
     '__pycache__/*' '*/__pycache__/*' '*.pyc'
unzip -t "$output" >/dev/null
checksum="$(sha256sum "$output" | awk '{print $1}')"
printf '%s  %s\n' "$checksum" "$(basename "$output")" >"$output.sha256"
printf '%s\n' "$output"
