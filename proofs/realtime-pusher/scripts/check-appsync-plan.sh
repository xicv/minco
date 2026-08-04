#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
proof_root="proofs/realtime-pusher/appsync-plan"
template="$proof_root/generated/template.yaml"

cd "$repository_root"
cargo minco deploy render-sam --root "$proof_root" --output generated/template.yaml --json
sam validate --lint --template-file "$template"
