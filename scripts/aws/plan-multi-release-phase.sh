#!/usr/bin/env bash
set -euo pipefail

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
repo_root="$(minco_repo_root)"
cd "$repo_root"

for command in git jq shasum; do
  require_command "$command"
done

: "${MINCO_MULTI_RELEASE_PLAN_FILE:?set MINCO_MULTI_RELEASE_PLAN_FILE to the exact whole-run plan}"
: "${MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST:?set MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST to the whole-run plan SHA-256}"
: "${MINCO_REHEARSAL_AUTHORITY_FILE:?set MINCO_REHEARSAL_AUTHORITY_FILE to the exact reviewed multi-release authority document}"
: "${MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST:?set MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST to the reviewed authority SHA-256}"
: "${MINCO_MULTI_RELEASE_PHASE_ID:?set MINCO_MULTI_RELEASE_PHASE_ID to the exact planned phase ID}"

[[ -f "$MINCO_MULTI_RELEASE_PLAN_FILE" && ! -L "$MINCO_MULTI_RELEASE_PLAN_FILE" ]] || {
  echo "multi-release plan must be a regular non-symlink file" >&2
  exit 1
}
[[ "$MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST" =~ ^[0-9a-f]{64}$ ]] || {
  echo "multi-release plan approval must be a SHA-256 digest" >&2
  exit 1
}
[[ -f "$MINCO_REHEARSAL_AUTHORITY_FILE" && ! -L "$MINCO_REHEARSAL_AUTHORITY_FILE" ]] || {
  echo "multi-release authority must be a regular non-symlink file" >&2
  exit 1
}
actual_plan_digest="$(shasum -a 256 "$MINCO_MULTI_RELEASE_PLAN_FILE" | awk '{print $1}')"
[[ "$actual_plan_digest" == "$MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST" ]] || {
  echo "multi-release plan approval does not match the exact document digest" >&2
  exit 1
}
jq -e -f scripts/aws/lib/validate-multi-release-plan.jq \
  "$MINCO_MULTI_RELEASE_PLAN_FILE" >/dev/null || {
  echo "multi-release plan is missing or broader than the fixed controller policy" >&2
  exit 1
}

case "$MINCO_MULTI_RELEASE_PHASE_ID" in
  01-prior-initial | 02-current | 03-prior-rollback) ;;
  *)
    echo "multi-release phase ID is not part of the fixed sequence" >&2
    exit 1
    ;;
esac

plan_authority_digest="$(jq -er '.authority.approval_digest' "$MINCO_MULTI_RELEASE_PLAN_FILE")"
[[ "$plan_authority_digest" == "$MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST" ]] || {
  echo "multi-release plan does not bind the approved authority digest" >&2
  exit 1
}
run_id="$(jq -er '.authority.run_id' "$MINCO_MULTI_RELEASE_PLAN_FILE")"
prior_revision="$(jq -er '.phases[0].source.revision' "$MINCO_MULTI_RELEASE_PLAN_FILE")"
current_revision="$(jq -er '.phases[1].source.revision' "$MINCO_MULTI_RELEASE_PLAN_FILE")"
authority_region="$(jq -er '.expected_region' "$MINCO_REHEARSAL_AUTHORITY_FILE")"
authority_profile="$(jq -er '.aws_profile' "$MINCO_REHEARSAL_AUTHORITY_FILE")"
authority_environment="$(jq -er '.environment' "$MINCO_REHEARSAL_AUTHORITY_FILE")"
authority_database_boundary="$(jq -cer '.database_boundary' "$MINCO_REHEARSAL_AUTHORITY_FILE")"
authority_resource_allowlist="$(jq -er '.resource_allowlist' "$MINCO_REHEARSAL_AUTHORITY_FILE")"
authority_cleanup_blast_radius="$(jq -er '.cleanup_blast_radius' "$MINCO_REHEARSAL_AUTHORITY_FILE")"
scripts/aws/validate-multi-release-rehearsal-authority.sh \
  "$MINCO_REHEARSAL_AUTHORITY_FILE" \
  "$MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST" \
  "$run_id" \
  "$prior_revision" \
  "$current_revision" \
  "$authority_region" \
  "$authority_profile" \
  "$authority_environment" \
  "$authority_database_boundary" \
  "$authority_resource_allowlist" \
  "$authority_cleanup_blast_radius"

canonical_checkout_root() {
  local label="$1"
  local root="$2"

  [[ "$root" == /* && -d "$root" && ! -L "$root" ]] || {
    printf '%s root must be an absolute existing non-symlink directory\n' "$label" >&2
    return 1
  }
  [[ -f "$root/minco.toml" && ! -L "$root/minco.toml" ]] || {
    printf '%s root must contain a regular non-symlink minco.toml\n' "$label" >&2
    return 1
  }
  (
    cd "$root"
    pwd -P
  )
}

canonical_planned_directory() {
  local label="$1"
  local path="$2"
  local existing_ancestor
  local canonical_ancestor
  local suffix

  [[ "$path" == /* && "$path" != *//* &&
    "$path" != *"/../"* && "$path" != */.. &&
    "$path" != *"/./"* && "$path" != */. && ! -L "$path" ]] || {
    printf '%s must be an absolute normalized non-symlink path\n' "$label" >&2
    return 1
  }
  if [[ -e "$path" && ! -d "$path" ]]; then
    printf '%s must be a directory when it exists\n' "$label" >&2
    return 1
  fi
  existing_ancestor="$path"
  while [[ ! -e "$existing_ancestor" && ! -L "$existing_ancestor" ]]; do
    existing_ancestor="$(dirname "$existing_ancestor")"
  done
  [[ -d "$existing_ancestor" && ! -L "$existing_ancestor" ]] || {
    printf '%s must descend from an existing non-symlink directory\n' "$label" >&2
    return 1
  }
  canonical_ancestor="$(cd "$existing_ancestor" && pwd -P)"
  suffix="${path#"$existing_ancestor"}"
  printf '%s%s\n' "$canonical_ancestor" "$suffix"
}

path_is_within() {
  local path="$1"
  local parent="$2"
  [[ "$path" == "$parent" || "$path" == "$parent/"* ]]
}

require_exact_clean_checkout() {
  local label="$1"
  local root="$2"
  local expected_revision="$3"
  local status
  local revision

  if [[ -d "$root/.jj" && ! -L "$root/.jj" ]] && command -v jj >/dev/null; then
    status="$(cd "$root" && jj diff --summary)"
  elif [[ (-d "$root/.git" || -f "$root/.git") && ! -L "$root/.git" ]] &&
    git -C "$root" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    status="$(git -C "$root" status --porcelain=v1 --untracked-files=normal)"
  else
    printf '%s root must own JJ or Git checkout metadata\n' "$label" >&2
    return 1
  fi
  [[ -z "$status" ]] || {
    printf '%s root must be clean at phase handoff\n' "$label" >&2
    return 1
  }
  revision="$(cd "$root" && current_source_revision)"
  [[ "$revision" == "$expected_revision" ]] || {
    printf '%s root revision does not match the whole-run plan\n' "$label" >&2
    return 1
  }
}

prior_root="$(canonical_checkout_root prior "$(jq -er '.phases[0].source.root' "$MINCO_MULTI_RELEASE_PLAN_FILE")")"
current_root="$(canonical_checkout_root current "$(jq -er '.phases[1].source.root' "$MINCO_MULTI_RELEASE_PLAN_FILE")")"
[[ "$prior_root" != "$current_root" ]] || {
  echo "multi-release phase roots must remain distinct" >&2
  exit 1
}
require_exact_clean_checkout prior "$prior_root" "$prior_revision"
require_exact_clean_checkout current "$current_root" "$current_revision"

phase_json="$(jq -cer \
  --arg phase_id "$MINCO_MULTI_RELEASE_PHASE_ID" \
  '.phases | map(select(.id == $phase_id)) | if length == 1 then .[0] else error("phase") end' \
  "$MINCO_MULTI_RELEASE_PLAN_FILE")"
planned_evidence_root="$(jq -er '.evidence_root' "$MINCO_MULTI_RELEASE_PLAN_FILE")"
evidence_root="$(canonical_planned_directory \
  "multi-release evidence root" "$planned_evidence_root")"
[[ "$evidence_root" == "$planned_evidence_root" ]] || {
  echo "multi-release evidence root is not canonical" >&2
  exit 1
}
if path_is_within "$evidence_root" "$prior_root" ||
  path_is_within "$evidence_root" "$current_root"; then
  echo "multi-release evidence root must remain outside both source checkouts" >&2
  exit 1
fi
evidence_namespace="$(jq -er '.evidence_namespace' <<<"$phase_json")"
evidence_path="$evidence_root/$evidence_namespace"
[[ ! -e "$evidence_path" && ! -L "$evidence_path" ]] || {
  echo "multi-release phase evidence namespace must not already exist" >&2
  exit 1
}
existing_ancestor="$evidence_path"
while [[ ! -e "$existing_ancestor" && ! -L "$existing_ancestor" ]]; do
  existing_ancestor="$(dirname "$existing_ancestor")"
done
[[ ! -L "$existing_ancestor" ]] || {
  echo "multi-release phase evidence ancestor must not be a symlink" >&2
  exit 1
}
canonical_ancestor="$(cd "$existing_ancestor" && pwd -P)"
[[ "$canonical_ancestor" == "$evidence_root" || "$canonical_ancestor" == "$evidence_root/"* ||
  "$evidence_root" == "$canonical_ancestor/"* ]] || {
  echo "multi-release phase evidence path escapes the whole-run evidence root" >&2
  exit 1
}

authority_json="$(jq -c '.authority' "$MINCO_MULTI_RELEASE_PLAN_FILE")"
rollback_json="$(jq -c \
  --arg phase_id "$MINCO_MULTI_RELEASE_PHASE_ID" \
  'if $phase_id == "03-prior-rollback" then .rollback else null end' \
  "$MINCO_MULTI_RELEASE_PLAN_FILE")"
jq -n \
  --arg plan_digest "$actual_plan_digest" \
  --arg controller_root "$current_root" \
  --arg evidence_namespace "$evidence_namespace" \
  --arg evidence_path "$evidence_path" \
  --argjson authority "$authority_json" \
  --argjson phase "$phase_json" \
  --argjson rollback "$rollback_json" \
  '{
    schema_version: 1,
    operation: "multi_release_phase",
    external_aws_contact: false,
    plan_digest: $plan_digest,
    authority: $authority,
    controller: {
      root: $controller_root,
      cleanup_owner: "parent_controller"
    },
    evidence: {
      namespace: $evidence_namespace,
      path: $evidence_path,
      write_policy: "create_only"
    },
    phase: $phase,
    rollback: $rollback
  }'
