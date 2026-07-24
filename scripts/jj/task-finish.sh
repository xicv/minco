#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
[[ $# -ge 2 ]] || { echo "usage: $0 TASK-ID DESCRIPTION [--push]" >&2; exit 2; }
task="$1"; description="$2"; shift 2
exec cargo minco vcs task-finish "$task" --message "$description" "$@"
