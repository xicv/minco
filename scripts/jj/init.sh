#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
command -v jj >/dev/null || { echo 'Jujutsu (jj) is required' >&2; exit 1; }
[[ -d .jj ]] || jj git init .
jj config set --repo git.colocate true
jj config set --repo git.push-new-bookmarks false
jj config set --repo ui.default-command '["log", "--limit", "12"]'
printf 'Initialized colocated JJ/Git repository. Use JJ for mutations and GitHub for transport.\n'
