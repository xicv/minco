#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

mapfile -t packages < <(
  python3 - <<'PY'
import tomllib
from pathlib import Path
value = tomllib.loads(Path('Cargo.toml').read_text())
for package in value['workspace']['metadata']['minco']['release']['publish']:
    print(package)
PY
)

for package in "${packages[@]}"; do
  printf '\n## %s\n' "$package"
  cargo package --locked --list --package "$package"
done
