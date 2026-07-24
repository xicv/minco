#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

version="$(uv run --locked python - <<'PY'
import tomllib
print(tomllib.load(open('Cargo.toml','rb'))['workspace']['package']['version'])
PY
)"
output="${1:-../minco-framework-${version}.zip}"

mkdir -p verification
find . -path './.venv' -prune -o -type d -name __pycache__ -prune -exec rm -rf {} +
find . -path './.venv' -prune -o -type f -name '*.pyc' -delete

uv run --locked python scripts/generate_bootstrap_artifacts.py
uv run --locked python scripts/test/sqlite_schema.py > verification/sqlite-schema.txt
uv run --locked python scripts/test/scaffold_templates.py > verification/scaffold-templates.json
uv run --locked python scripts/test/feedback_contract.py > verification/feedback-contract.json
node --check plugins/minco-plugin-feedback/assets/widget.js > verification/widget-js.txt 2>&1
uv run --locked python scripts/validate_static.py --output verification/static-validation.json >/dev/null
uv run --locked python scripts/validate_publish.py --output verification/publish-validation.json >/dev/null
uv run --locked python scripts/deep_review.py >/dev/null
cp target/minco/deep-review.json verification/deep-review.json
uv run --locked python scripts/test/feedback_contract.py > verification/feedback-contract.json
node --check plugins/minco-plugin-feedback/assets/widget.js
printf '%s\n' 'Feedback widget JavaScript passed node --check.' > verification/widget-node-check.txt
if [[ -d .jj ]] && command -v jj >/dev/null 2>&1; then
  if [[ -n "$(jj diff --summary)" ]]; then
    jj diff --git | git apply --check --reverse --whitespace=error-all
  fi
  printf '%s\n' 'Working-copy diff passed JJ/Git whitespace validation.' > verification/git-diff-check.txt
elif [[ -d .git ]]; then
  git diff --check
  printf '%s\n' 'Working-tree diff passed git diff --check.' > verification/git-diff-check.txt
else
  printf '%s\n' 'A JJ or Git working copy is required for whitespace validation.' >&2
  exit 1
fi
while IFS= read -r script; do
  uv run --locked python -m py_compile "$script"
done < <(find scripts -type f -name '*.py' | sort)
printf '%s
' 'All repository Python scripts compiled successfully.' > verification/python-compile.txt
find scripts -type d -name __pycache__ -prune -exec rm -rf {} +
find scripts -type f -name '*.pyc' -delete
while IFS= read -r script; do bash -n "$script"; done < <(find scripts -type f -name '*.sh' | sort)
printf '%s
' 'All repository shell scripts passed bash -n.' > verification/shell-syntax.txt
uv run --locked python scripts/source_manifest.py >/dev/null
uv run --locked python -m json.tool verification/source-manifest.json >/dev/null

rm -f "$output" "$output.sha256"
zip -qr "$output" . \
  -x '.git/*' '.jj/*' 'target/*' '*.zip' '.env' '*.db' '*.sqlite' \
     '.venv/*' '__pycache__/*' '*/__pycache__/*' '*.pyc'
unzip -t "$output" >/dev/null
checksum="$(sha256sum "$output" | awk '{print $1}')"
printf '%s  %s\n' "$checksum" "$(basename "$output")" >"$output.sha256"
printf '%s\n' "$output"
