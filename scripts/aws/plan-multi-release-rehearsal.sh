#!/usr/bin/env bash
set -euo pipefail

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
repo_root="$(minco_repo_root)"
cd "$repo_root"

for command in git jq shasum; do
  require_command "$command"
done

: "${MINCO_PRIOR_ROOT:?set MINCO_PRIOR_ROOT to the exact prior-release checkout}"
: "${MINCO_CURRENT_ROOT:?set MINCO_CURRENT_ROOT to the exact current-release checkout}"
: "${MINCO_REHEARSAL_AUTHORITY_FILE:?set MINCO_REHEARSAL_AUTHORITY_FILE to the exact reviewed multi-release authority document}"
: "${MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST:?set MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST to the reviewed authority SHA-256}"
: "${MINCO_AWS_RUN_ID:?set MINCO_AWS_RUN_ID to the reviewed run ID}"
: "${MINCO_REHEARSAL_PROFILE:?set MINCO_REHEARSAL_PROFILE to the approved non-root profile}"
: "${AWS_REGION:?set AWS_REGION to the approved Region}"
: "${MINCO_REHEARSAL_DATABASE_BOUNDARY_JSON:?set MINCO_REHEARSAL_DATABASE_BOUNDARY_JSON to the reviewed database boundary}"
: "${MINCO_REHEARSAL_RESOURCE_ALLOWLIST:?set MINCO_REHEARSAL_RESOURCE_ALLOWLIST to the reviewed resource scope}"
: "${MINCO_REHEARSAL_CLEANUP_BLAST_RADIUS:?set MINCO_REHEARSAL_CLEANUP_BLAST_RADIUS to the reviewed cleanup scope}"

require_safe_name MINCO_AWS_RUN_ID "$MINCO_AWS_RUN_ID"

canonical_checkout_root() {
  local label="$1"
  local root="$2"

  [[ "$root" == /* ]] || {
    printf '%s root must be absolute\n' "$label" >&2
    return 1
  }
  [[ -d "$root" && ! -L "$root" ]] || {
    printf '%s root must be an existing non-symlink directory\n' "$label" >&2
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

checkout_revision() {
  local label="$1"
  local root="$2"
  local revision

  revision="$(
    cd "$root"
    current_source_revision
  )" || {
    printf 'could not resolve the %s checkout revision\n' "$label" >&2
    return 1
  }
  printf '%s\n' "$revision"
}

require_clean_checkout() {
  local label="$1"
  local root="$2"
  local status

  if [[ -d "$root/.jj" && ! -L "$root/.jj" ]] && command -v jj >/dev/null; then
    status="$(cd "$root" && jj diff --summary)"
  elif [[ (-d "$root/.git" || -f "$root/.git") && ! -L "$root/.git" ]] &&
    git -C "$root" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    status="$(git -C "$root" status --porcelain=v1 --untracked-files=normal)"
  else
    printf '%s root must be a JJ or Git checkout\n' "$label" >&2
    return 1
  fi
  [[ -z "$status" ]] || {
    printf '%s root must be clean before multi-release planning\n' "$label" >&2
    return 1
  }
}

prior_root="$(canonical_checkout_root prior "$MINCO_PRIOR_ROOT")"
current_root="$(canonical_checkout_root current "$MINCO_CURRENT_ROOT")"
[[ "$prior_root" != "$current_root" ]] || {
  echo "prior and current roots must be distinct canonical checkouts" >&2
  exit 1
}
require_clean_checkout prior "$prior_root"
require_clean_checkout current "$current_root"
prior_revision="$(checkout_revision prior "$prior_root")"
current_revision="$(checkout_revision current "$current_root")"

scripts/aws/validate-multi-release-rehearsal-authority.sh \
  "$MINCO_REHEARSAL_AUTHORITY_FILE" \
  "$MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST" \
  "$MINCO_AWS_RUN_ID" \
  "$prior_revision" \
  "$current_revision" \
  "$AWS_REGION" \
  "$MINCO_REHEARSAL_PROFILE" \
  dev \
  "$MINCO_REHEARSAL_DATABASE_BOUNDARY_JSON" \
  "$MINCO_REHEARSAL_RESOURCE_ALLOWLIST" \
  "$MINCO_REHEARSAL_CLEANUP_BLAST_RADIUS"

evidence_root="target/minco/aws/$MINCO_AWS_RUN_ID"
jq -n \
  --arg approval_digest "$MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST" \
  --arg run_id "$MINCO_AWS_RUN_ID" \
  --arg evidence_root "$evidence_root" \
  --arg prior_root "$prior_root" \
  --arg current_root "$current_root" \
  --arg prior_revision "$prior_revision" \
  --arg current_revision "$current_revision" \
  '{
    schema_version: 1,
    operation: "multi_release_controller_rehearsal",
    external_aws_contact: false,
    authority: {
      kind: "minco.aws-multi-release-controller-rehearsal.v1",
      run_id: $run_id,
      approval_digest: $approval_digest
    },
    evidence_root: $evidence_root,
    provider_boundary: {
      shared_stack: true,
      stack_lifecycle: ["create", "update", "update", "delete"],
      artifact_bucket_lifetime: "whole_run"
    },
    rollback: {
      compatibility_assessment_required: true,
      current_promotion_phase: "02-current",
      target_promotion_phase: "01-prior-initial",
      accepted_result: "compatible",
      historical_hosted_report_reuse: false
    },
    cleanup: {
      owner: "parent_controller",
      trap_count: 1,
      inner_phase_cleanup: false,
      after_phase: "03-prior-rollback"
    },
    phases: [
      {
        id: "01-prior-initial",
        release: "prior",
        source: {root: $prior_root, revision: $prior_revision},
        evidence_namespace: "phases/01-prior-initial",
        evidence_write_policy: "create_only",
        stack_action: "create",
        artifact_policy: {
          build: true,
          replan: true,
          reuse_exact_release_from_phase: null
        },
        fresh_hosted_verification: true,
        promotion_required: true
      },
      {
        id: "02-current",
        release: "current",
        source: {root: $current_root, revision: $current_revision},
        evidence_namespace: "phases/02-current",
        evidence_write_policy: "create_only",
        stack_action: "update",
        artifact_policy: {
          build: true,
          replan: true,
          reuse_exact_release_from_phase: null
        },
        fresh_hosted_verification: true,
        promotion_required: true
      },
      {
        id: "03-prior-rollback",
        release: "prior",
        source: {root: $prior_root, revision: $prior_revision},
        evidence_namespace: "phases/03-prior-rollback",
        evidence_write_policy: "create_only",
        stack_action: "update",
        artifact_policy: {
          build: false,
          replan: false,
          reuse_exact_release_from_phase: "01-prior-initial"
        },
        fresh_hosted_verification: true,
        promotion_required: true
      }
    ]
  }'
