#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
[[ $# -ge 1 ]] || { echo "usage: $0 TASK-ID [DESTINATION]" >&2; exit 2; }
task="$1"
destination="${2:-../minco-task-${task,,}}"
exec cargo minco vcs task-start "$task" --destination "$destination"
