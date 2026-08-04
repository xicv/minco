#!/usr/bin/env bash
set -euo pipefail
umask 077

# This is the fixed provider-capable child of the root bootstrap. The root
# bootstrap remains the sole owner of application, database and IAM cleanup.

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
controller_root="$(minco_repo_root)"
cd "$controller_root"

for command in awk cmp cp diff git jq mktemp shasum stat; do
  require_command "$command"
done

: "${MINCO_MULTI_RELEASE_ACTION:=plan}"
: "${MINCO_REHEARSAL_AUTHORITY_FILE:?set MINCO_REHEARSAL_AUTHORITY_FILE to the exact reviewed multi-release authority document}"
: "${MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST:?set MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST to the reviewed authority SHA-256}"
: "${MINCO_MULTI_RELEASE_EVIDENCE_ROOT:?set MINCO_MULTI_RELEASE_EVIDENCE_ROOT to the initialized whole-run evidence directory}"
: "${MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST:?set MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST to the initialized controller receipt digest}"
: "${MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST:?set MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST to the exact first phase-start receipt digest}"
: "${MINCO_AWS_RUN_ID:?set MINCO_AWS_RUN_ID to the reviewed run ID}"
: "${AWS_REGION:?set AWS_REGION to the reviewed Region}"
: "${MINCO_REHEARSAL_PROFILE:?set MINCO_REHEARSAL_PROFILE to the reviewed root profile}"
: "${MINCO_REHEARSAL_DATABASE_BOUNDARY_JSON:?set MINCO_REHEARSAL_DATABASE_BOUNDARY_JSON to the reviewed disposable database boundary}"
: "${MINCO_REHEARSAL_RESOURCE_ALLOWLIST:?set MINCO_REHEARSAL_RESOURCE_ALLOWLIST to the reviewed resource scope}"
: "${MINCO_REHEARSAL_CLEANUP_BLAST_RADIUS:?set MINCO_REHEARSAL_CLEANUP_BLAST_RADIUS to the reviewed cleanup scope}"

[[ "$MINCO_MULTI_RELEASE_ACTION" == "plan" ||
  "$MINCO_MULTI_RELEASE_ACTION" == "execute" ]] || {
  echo "MINCO_MULTI_RELEASE_ACTION must equal plan or execute" >&2
  exit 1
}
for approval in \
  "$MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST" \
  "$MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST" \
  "$MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST"; do
  [[ "$approval" =~ ^[0-9a-f]{64}$ ]] || {
    echo "bounded multi-release approvals must be SHA-256 digests" >&2
    exit 1
  }
done
require_safe_name MINCO_AWS_RUN_ID "$MINCO_AWS_RUN_ID"

canonical_private_directory() {
  local label="$1"
  local path="$2"
  local canonical

  [[ "$path" == /* && -d "$path" && ! -L "$path" ]] || {
    printf '%s must be an absolute existing non-symlink directory\n' "$label" >&2
    return 1
  }
  canonical="$(cd "$path" && pwd -P)"
  [[ "$canonical" == "$path" && "$(minco_file_mode "$path")" == "700" ]] || {
    printf '%s must be canonical and mode 0700\n' "$label" >&2
    return 1
  }
}

canonical_private_file() {
  local label="$1"
  local path="$2"
  local canonical_parent
  local canonical
  local mode

  [[ "$path" == /* && -f "$path" && ! -L "$path" ]] || {
    printf '%s must be an absolute regular non-symlink file\n' "$label" >&2
    return 1
  }
  canonical_parent="$(cd "$(dirname "$path")" && pwd -P)"
  canonical="$canonical_parent/$(basename "$path")"
  mode="$(minco_file_mode "$path")"
  if [[ "$canonical" != "$path" ]] || (((8#$mode & 8#077) != 0)); then
    printf '%s must be canonical and private\n' "$label" >&2
    return 1
  fi
}

require_exact_clean_checkout() {
  local label="$1"
  local root="$2"
  local expected_revision="$3"
  local canonical
  local status

  [[ "$root" == /* && -d "$root" && ! -L "$root" &&
    -f "$root/minco.toml" && ! -L "$root/minco.toml" ]] || {
    printf '%s source root must be an absolute existing checkout\n' "$label" >&2
    return 1
  }
  canonical="$(cd "$root" && pwd -P)"
  [[ "$canonical" == "$root" ]] || {
    printf '%s source root must be canonical\n' "$label" >&2
    return 1
  }
  if [[ -d "$root/.jj" && ! -L "$root/.jj" ]] && command -v jj >/dev/null; then
    status="$(cd "$root" && jj diff --summary)"
  elif [[ (-d "$root/.git" || -f "$root/.git") && ! -L "$root/.git" ]] &&
    git -C "$root" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    status="$(git -C "$root" status --porcelain=v1 --untracked-files=normal)"
  else
    printf '%s source root must own JJ or Git metadata\n' "$label" >&2
    return 1
  fi
  [[ -z "$status" &&
    "$(cd "$root" && current_source_revision)" == "$expected_revision" ]] || {
    printf '%s source root changed after controller initialization\n' "$label" >&2
    return 1
  }
}

canonical_private_directory \
  "multi-release evidence root" "$MINCO_MULTI_RELEASE_EVIDENCE_ROOT"
evidence_root="$MINCO_MULTI_RELEASE_EVIDENCE_ROOT"
controller_receipt="$evidence_root/control/controller-receipt.json"
sealed_plan="$evidence_root/control/multi-release-plan.json"
phase_one_control="$evidence_root/phases/01-prior-initial"
phase_one_start="$phase_one_control/phase-start-receipt.json"
parent_start="$phase_one_control/parent-session-start-receipt.json"
parent_completion="$phase_one_control/parent-session-completion-receipt.json"
for control_file in \
  "$controller_receipt" "$sealed_plan" "$phase_one_start" \
  "$parent_start" "$parent_completion"; do
  canonical_private_file "multi-release control evidence" "$control_file"
done

jq -e -f scripts/aws/lib/validate-multi-release-controller-receipt.jq \
  "$controller_receipt" >/dev/null
jq -e -f scripts/aws/lib/validate-multi-release-plan.jq "$sealed_plan" >/dev/null
jq -e -f scripts/aws/lib/validate-multi-release-phase-start-receipt.jq \
  "$phase_one_start" >/dev/null
jq -e -f scripts/aws/lib/validate-multi-release-parent-session-receipt.jq \
  "$parent_start" >/dev/null
jq -e -f scripts/aws/lib/validate-multi-release-parent-session-receipt.jq \
  "$parent_completion" >/dev/null

controller_digest="$(jq -er '.receipt_digest' "$controller_receipt")"
phase_one_start_digest="$(jq -er '.receipt_digest' "$phase_one_start")"
parent_start_digest="$(jq -er '.receipt_digest' "$parent_start")"
parent_completion_digest="$(jq -er '.receipt_digest' "$parent_completion")"
[[ "$controller_digest" == \
    "$MINCO_APPROVE_MULTI_RELEASE_CONTROLLER_RECEIPT_DIGEST" &&
  "$phase_one_start_digest" == \
    "$MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST" &&
  "$(jq -cS 'del(.receipt_digest)' "$controller_receipt" |
    shasum -a 256 | awk '{print $1}')" == "$controller_digest" &&
  "$(jq -cS 'del(.receipt_digest)' "$phase_one_start" |
    shasum -a 256 | awk '{print $1}')" == "$phase_one_start_digest" &&
  "$(jq -cS 'del(.receipt_digest)' "$parent_start" |
    shasum -a 256 | awk '{print $1}')" == "$parent_start_digest" &&
  "$(jq -cS 'del(.receipt_digest)' "$parent_completion" |
    shasum -a 256 | awk '{print $1}')" == "$parent_completion_digest" ]] || {
  echo "multi-release control evidence does not match the exact approvals" >&2
  exit 1
}
jq -e \
  --arg start_digest "$parent_start_digest" \
  '
    .state == "provider_resources_absent"
    and .external_aws_contact == true
    and .execution.mode == "provider_resource_preflight"
    and .execution.provider_state == "resources_absent"
    and .session.start_receipt_digest == $start_digest
    and .cleanup == {
      action: "none_read_only_resource_preflight",
      owner: "parent_controller",
      required: true,
      state: "disarmed",
      trap_count: 1
    }
  ' "$parent_completion" >/dev/null || {
  echo "resource absence preflight is incomplete or outside policy" >&2
  exit 1
}

actual_authority_digest="$(
  shasum -a 256 "$MINCO_REHEARSAL_AUTHORITY_FILE" | awk '{print $1}'
)"
[[ "$actual_authority_digest" == "$MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST" &&
  "$actual_authority_digest" == \
    "$(jq -er '.authority.approval_digest' "$controller_receipt")" ]] || {
  echo "multi-release authority changed after controller initialization" >&2
  exit 1
}
prior_root="$(jq -er '.phases[0].source.root' "$sealed_plan")"
current_root="$(jq -er '.phases[1].source.root' "$sealed_plan")"
prior_revision="$(jq -er '.source_revisions.prior' "$controller_receipt")"
current_revision="$(jq -er '.source_revisions.current' "$controller_receipt")"
[[ "$current_root" == "$controller_root" && "$prior_root" != "$current_root" ]] || {
  echo "bounded multi-release runner must execute from the exact current checkout" >&2
  exit 1
}
require_exact_clean_checkout prior "$prior_root" "$prior_revision"
require_exact_clean_checkout current "$current_root" "$current_revision"
for protected_path in \
  Cargo.toml Cargo.lock minco.toml crates extensions examples infra stubs; do
  if [[ -e "$prior_root/$protected_path" || -e "$current_root/$protected_path" ]]; then
    if [[ ! -e "$prior_root/$protected_path" ||
      ! -e "$current_root/$protected_path" ]] ||
      ! diff -qr \
        "$prior_root/$protected_path" \
        "$current_root/$protected_path" >/dev/null; then
      echo "bounded rollback evidence requires identical protected release inputs" >&2
      exit 1
    fi
  fi
done
scripts/aws/validate-multi-release-rehearsal-authority.sh \
  "$MINCO_REHEARSAL_AUTHORITY_FILE" \
  "$actual_authority_digest" \
  "$MINCO_AWS_RUN_ID" \
  "$prior_revision" \
  "$current_revision" \
  "$AWS_REGION" \
  "$MINCO_REHEARSAL_PROFILE" \
  dev \
  "$MINCO_REHEARSAL_DATABASE_BOUNDARY_JSON" \
  "$MINCO_REHEARSAL_RESOURCE_ALLOWLIST" \
  "$MINCO_REHEARSAL_CLEANUP_BLAST_RADIUS"
jq -e '
  .database_boundary.mode == "disposable-rds"
  and .resource_allowlist == "bounded-root-temp-rds-multi-release-v1"
  and .cleanup_blast_radius == "cleanup-bounded-root-temp-rds-multi-release-v1"
  and .release_sequence == ["prior", "current", "prior"]
' "$MINCO_REHEARSAL_AUTHORITY_FILE" >/dev/null || {
  echo "bounded multi-release execution requires the fixed disposable-RDS policy" >&2
  exit 1
}

execution_plan="$(mktemp "${TMPDIR:-/tmp}/minco-multi-release-plan.XXXXXX")"
jq -n \
  --arg controller_digest "$controller_digest" \
  --arg authority_digest "$actual_authority_digest" \
  --arg preflight_digest "$parent_completion_digest" \
  --arg prior_revision "$prior_revision" \
  --arg current_revision "$current_revision" \
  '{
    schema_version: 1,
    operation: "bounded_multi_release_smoke",
    external_aws_contact: false,
    controller_receipt_digest: $controller_digest,
    authority_digest: $authority_digest,
    resource_preflight_receipt_digest: $preflight_digest,
    source_revisions: {
      prior: $prior_revision,
      current: $current_revision
    },
    phases: [
      {id: "01-prior-initial", source: "prior", build: true, replan: true, stack_action: "create"},
      {id: "02-current", source: "current", build: true, replan: true, stack_action: "update"},
      {id: "03-prior-rollback", source: "prior", build: false, replan: false, stack_action: "update"}
    ],
    rollback: {
      reuse_exact_phase_one_release: true,
      fresh_hosted_verification: true,
      reverse_sql: false,
      automatic_data_repair: false
    },
    cleanup: {
      owner: "root_bootstrap",
      child_trap_count: 0,
      required_after_phase: "03-prior-rollback"
    }
  }' >"$execution_plan"
chmod 600 "$execution_plan"
if [[ "$MINCO_MULTI_RELEASE_ACTION" == "plan" ]]; then
  cat "$execution_plan"
  rm -f -- "$execution_plan"
  exit 0
fi
rm -f -- "$execution_plan"

for command in aws cargo curl psql sam uv; do
  require_command "$command"
done
: "${AWS_PROFILE:?execute requires the isolated temporary deploy profile}"
: "${MINCO_DATABASE_URL_PARAMETER:?execute requires the run-owned database parameter}"
: "${MINCO_STACK_NAME:?execute requires the deterministic application stack}"
: "${MINCO_AWS_ARTIFACT_BUCKET:?execute requires the deterministic artifact bucket}"
: "${MINCO_SMOKE_APPLICATION:?execute requires the deterministic application name}"
: "${MINCO_DATABASE_INSTANCE_OWNED:?execute requires the disposable database ownership flag}"
[[ "$MINCO_DATABASE_INSTANCE_OWNED" == "true" ]] || {
  echo "bounded multi-release execution may use only the disposable database" >&2
  exit 1
}

shared_evidence_dir="$current_root/target/minco/aws/$MINCO_AWS_RUN_ID"
canonical_private_directory "root-bootstrap evidence directory" "$shared_evidence_dir"
MINCO_AWS_EVIDENCE_ID="$MINCO_AWS_RUN_ID" initialize_cloud_journal
identity="$(
  aws_logged sts get-caller-identity \
    "reverify the exact temporary deploy role before phase mutation" \
    --query '{Account:Account,Arn:Arn,UserId:UserId}' \
    --output json
)"
account_id="$(jq -er '.Account' <<<"$identity")"
caller_arn="$(jq -er '.Arn' <<<"$identity")"
case "$caller_arn" in
  arn:aws*:iam::"$account_id":role/*)
    caller_role_arn="$caller_arn"
    ;;
  arn:aws*:sts::"$account_id":assumed-role/*/*)
    partition="${caller_arn%%:sts::*}"
    role_session="${caller_arn#*:assumed-role/}"
    role_name="${role_session%%/*}"
    caller_role_arn="${partition}:iam::${account_id}:role/${role_name}"
    ;;
  *)
    echo "bounded multi-release execution requires an IAM role caller" >&2
    exit 1
    ;;
esac
[[ "$account_id" == "$(jq -er '.expected_account_id' "$MINCO_REHEARSAL_AUTHORITY_FILE")" &&
  "$caller_role_arn" == "$(jq -er '.expected_role_arn' "$MINCO_REHEARSAL_AUTHORITY_FILE")" ]] || {
  echo "bounded multi-release caller differs from exact authority" >&2
  exit 1
}
write_evidence_value "$shared_evidence_dir/caller-identity.json" "$identity"
unset caller_arn caller_role_arn identity partition role_name role_session

stack_error="$shared_evidence_dir/stack-preflight-error.txt"
if aws_logged_json cloudformation describe-stacks \
  "reprove application stack absence immediately before phase creation" \
  --stack-name "$MINCO_STACK_NAME" >/dev/null 2>"$stack_error"; then
  echo "refusing to mutate a pre-existing application stack" >&2
  exit 1
else
  stack_status=$?
fi
aws_cli_service_error_is "$stack_error" "$stack_status" ValidationError || {
  echo "could not reprove application stack absence immediately before creation" >&2
  exit 1
}
rm -f "$stack_error"
write_evidence_value "$shared_evidence_dir/stack-preflight-absent.txt" "$MINCO_STACK_NAME"

bucket_error="$shared_evidence_dir/bucket-preflight-error.txt"
if aws_logged_json s3api head-bucket \
  "reprove artifact bucket absence immediately before phase creation" \
  --bucket "$MINCO_AWS_ARTIFACT_BUCKET" >/dev/null 2>"$bucket_error"; then
  echo "refusing to mutate a pre-existing artifact bucket" >&2
  exit 1
else
  bucket_status=$?
fi
aws_cli_service_error_is "$bucket_error" "$bucket_status" 404 || {
  echo "could not reprove artifact bucket absence immediately before creation" >&2
  exit 1
}
rm -f "$bucket_error"
write_evidence_value \
  "$shared_evidence_dir/bucket-preflight-absent.txt" \
  "$MINCO_AWS_ARTIFACT_BUCKET"

MINCO_AWS_EVIDENCE_ID="$MINCO_AWS_RUN_ID" \
  MINCO_SMOKE_JWT_TOKEN="$(
    MINCO_AWS_EVIDENCE_ID="$MINCO_AWS_RUN_ID" \
      scripts/aws/create-smoke-identity.sh
  )"
export MINCO_SMOKE_JWT_TOKEN
issuer="$(<"$shared_evidence_dir/jwt-issuer.txt")"
client_id="$(<"$shared_evidence_dir/cognito-client-id.txt")"

bucket_arguments=(--bucket "$MINCO_AWS_ARTIFACT_BUCKET")
bucket_configuration="$(s3_tagged_create_configuration "$AWS_REGION" "$MINCO_AWS_RUN_ID")"
bucket_arguments+=(--create-bucket-configuration "$bucket_configuration")
aws_logged s3api create-bucket \
  "create the whole-run private artifact bucket" \
  "${bucket_arguments[@]}" >/dev/null
aws_logged s3api put-public-access-block \
  "block all public access on the whole-run artifact bucket" \
  --bucket "$MINCO_AWS_ARTIFACT_BUCKET" \
  --public-access-block-configuration \
  BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true \
  >/dev/null
aws_logged s3api put-bucket-encryption \
  "enable whole-run artifact bucket encryption" \
  --bucket "$MINCO_AWS_ARTIFACT_BUCKET" \
  --server-side-encryption-configuration \
  '{"Rules":[{"ApplyServerSideEncryptionByDefault":{"SSEAlgorithm":"AES256"},"BucketKeyEnabled":false}]}' \
  >/dev/null
wait_for_s3_bucket_visibility \
  "$MINCO_AWS_ARTIFACT_BUCKET" "$AWS_REGION" \
  "$shared_evidence_dir/bucket-visibility-error.txt"

phase_start_digest="$phase_one_start_digest"
previous_completion_digest=
phase_one_release=
rollback_assessment=

retain_cleanup_identifier() {
  local identifier_name="$1"
  local phase_dir="$2"
  local phase_identifier="$phase_dir/$identifier_name"
  local cleanup_identifier="$shared_evidence_dir/$identifier_name"

  [[ -f "$phase_identifier" && ! -L "$phase_identifier" ]] || {
    printf 'phase deployment did not retain %s for cleanup proof\n' \
      "$identifier_name" >&2
    return 1
  }
  if [[ -e "$cleanup_identifier" || -L "$cleanup_identifier" ]]; then
    if [[ ! -f "$cleanup_identifier" || -L "$cleanup_identifier" ]] ||
      ! cmp -s "$phase_identifier" "$cleanup_identifier"; then
      printf '%s changed across the bounded release sequence\n' \
        "$identifier_name" >&2
      return 1
    fi
  else
    cp "$phase_identifier" "$cleanup_identifier"
    chmod 600 "$cleanup_identifier"
  fi
}

run_release_phase() {
  local phase_id="$1"
  local source_root="$2"
  local review_policy="$3"
  local build_release="$4"
  local phase_dir="$source_root/target/minco/aws/$phase_id"
  local phase_relative="target/minco/aws/$phase_id"
  local build_directory="$source_root/target/lambda/orders-lambda"
  local release_manifest="$phase_dir/release.json"
  local target_config="$phase_dir/deployment-targets.toml"
  local smoke_config="$build_directory/minco.smoke.toml"
  local migration_plan="$phase_dir/database-migration-plan.json"
  local migration_receipt="$phase_dir/database-migration-receipt.json"
  local release_digest
  local change_set_digest
  local verification_digest

  mkdir -p "$phase_dir"
  chmod 700 "$source_root/target" "$source_root/target/minco" \
    "$source_root/target/minco/aws" "$phase_dir"
  if [[ "$phase_id" == "03-prior-rollback" ]]; then
    cp "$prior_root/target/minco/aws/01-prior-initial/database-migration-plan.json" \
      "$migration_plan"
    cp "$prior_root/target/minco/aws/01-prior-initial/database-migration-receipt.json" \
      "$migration_receipt"
    chmod 600 "$migration_plan" "$migration_receipt"
  else
    for migration_file in "$migration_plan" "$migration_receipt"; do
      [[ -f "$migration_file" && ! -L "$migration_file" ]] || {
        echo "exact-source migration evidence is missing for $phase_id" >&2
        return 1
      }
      chmod 600 "$migration_file"
    done
  fi

  if [[ "$build_release" == "true" ]]; then
    (
      cd "$source_root"
      scripts/aws/build-lambda.sh
      awk \
        -v issuer="$issuer" \
        -v audience="$client_id" \
        -v application="$MINCO_SMOKE_APPLICATION" \
        '
          /^application = / { print "application = \"" application "\""; next }
          /^issuer = / { print "issuer = \"" issuer "\""; next }
          /^audiences = / { print "audiences = [\"" audience "\"]"; next }
          /^artifact_path = / {
            print "artifact_path = \"target/lambda/orders-lambda/bootstrap.zip\""
            next
          }
          { print }
        ' examples/orders/config/minco.dev.toml >"$smoke_config"
      chmod 600 "$smoke_config"
      cargo minco deploy plan \
        --config "target/lambda/orders-lambda/minco.smoke.toml" \
        --output "target/lambda/orders-lambda/plan.json"
      cargo minco deploy render-sam \
        --config "target/lambda/orders-lambda/minco.smoke.toml" \
        --output "target/lambda/orders-lambda/template.yaml"
      uv run --locked python scripts/validate_static.py
      SAM_CLI_TELEMETRY=0 sam validate --lint \
        --template-file "target/lambda/orders-lambda/template.yaml"
      MINCO_CONFIG__APPLICATION__NAME="$MINCO_SMOKE_APPLICATION" \
        cargo minco release create \
          --artifact "target/lambda/orders-lambda/bootstrap.zip" \
          --plan "target/lambda/orders-lambda/plan.json" \
          --template "target/lambda/orders-lambda/template.yaml" \
          --output "$phase_relative/release.json"
      cargo minco release verify "$phase_relative/release.json"
    )
  else
    [[ "$phase_id" == "03-prior-rollback" && -n "$phase_one_release" ]] || {
      echo "only rollback may reuse the exact initial release" >&2
      return 1
    }
    cp "$phase_one_release" "$release_manifest"
    chmod 600 "$release_manifest"
    cmp -s "$phase_one_release" "$release_manifest"
    (cd "$source_root" && cargo minco release verify "$phase_relative/release.json")
  fi

  write_bounded_deployment_target_config \
    "$target_config" "$account_id" "$AWS_REGION" \
    "$(jq -er '.expected_role_arn' "$MINCO_REHEARSAL_AUTHORITY_FILE")" \
    "$MINCO_STACK_NAME" "$MINCO_AWS_ARTIFACT_BUCKET" \
    "$MINCO_DATABASE_URL_PARAMETER" "${MINCO_DATABASE_KMS_KEY_ARN:-}" \
    "${MINCO_LAMBDA_SUBNET_IDS:-}" \
    "${MINCO_LAMBDA_SECURITY_GROUP_IDS:-}" "$MINCO_AWS_RUN_ID"

  release_digest="$(jq -er '.release_digest' "$release_manifest")"
  (
    cd "$source_root"
    MINCO_AWS_EVIDENCE_ID="$phase_id" \
    MINCO_RELEASE_MANIFEST="$phase_relative/release.json" \
    MINCO_DEPLOY_TARGET_CONFIG="$phase_relative/deployment-targets.toml" \
    MINCO_DEPLOY_PHASE=changeset \
    MINCO_APPROVE_RELEASE_DIGEST="$release_digest" \
      scripts/aws/deploy.sh
  )
  bounded_phase_change_set_is_authorized \
    "$phase_dir/change-set-receipt.json" "$review_policy" || {
    echo "phase change set exceeded the fixed review policy" >&2
    return 1
  }
  change_set_digest="$(jq -er '.receipt_digest' "$phase_dir/change-set-receipt.json")"
  (
    cd "$source_root"
    MINCO_AWS_EVIDENCE_ID="$phase_id" \
    MINCO_RELEASE_MANIFEST="$phase_relative/release.json" \
    MINCO_DEPLOY_TARGET_CONFIG="$phase_relative/deployment-targets.toml" \
    MINCO_DEPLOY_PHASE=apply \
    MINCO_CHANGESET_RECEIPT="$phase_relative/change-set-receipt.json" \
    MINCO_MIGRATION_PLAN="$phase_relative/database-migration-plan.json" \
    MINCO_MIGRATION_RECEIPT="$phase_relative/database-migration-receipt.json" \
    MINCO_APPROVE_CHANGESET_DIGEST="$change_set_digest" \
      scripts/aws/deploy.sh

    retain_cleanup_identifier function-name.txt "$phase_dir"
    retain_cleanup_identifier function-role-name.txt "$phase_dir"
    retain_cleanup_identifier http-api-id.txt "$phase_dir"
    MINCO_AWS_EVIDENCE_ID="$phase_id"
    MINCO_AWS_EVIDENCE_RELATIVE="$phase_relative"
    MINCO_AWS_EVIDENCE_DIR="$phase_dir"
    MINCO_AWS_TOUCH_LOG="$phase_dir/cloud-touches.jsonl"
    MINCO_SMOKE_DATA_ID="$phase_id"
    export \
      MINCO_AWS_EVIDENCE_DIR \
      MINCO_AWS_EVIDENCE_ID \
      MINCO_AWS_EVIDENCE_RELATIVE \
      MINCO_AWS_TOUCH_LOG \
      MINCO_SMOKE_DATA_ID
    touch "$MINCO_AWS_TOUCH_LOG"
    chmod 600 "$MINCO_AWS_TOUCH_LOG"
    record_cloud_touch \
      "aws:lambda,execute-api" "hosted-verification" \
      "freshly verify the exact $phase_id candidate"
    cargo minco deploy verify \
      --manifest "$phase_relative/release.json" \
      --receipt "$phase_relative/deployment-receipt.json" \
      --output "$phase_relative/hosted-verification.json" \
      --json >"$phase_dir/hosted-verification-output.json"
    chmod 600 "$phase_dir/hosted-verification.json" \
      "$phase_dir/hosted-verification-output.json"
    verification_digest="$(
      shasum -a 256 "$phase_dir/hosted-verification.json" | awk '{print $1}'
    )"
    record_cloud_touch \
      "aws:cloudformation" "guarded-promotion" \
      "route live traffic to the freshly verified $phase_id candidate"
    cargo minco promote \
      --manifest "$phase_relative/release.json" \
      --receipt "$phase_relative/deployment-receipt.json" \
      --verification "$phase_relative/hosted-verification.json" \
      --output "$phase_relative/promotion-receipt.json" \
      --approve-verification-digest "$verification_digest" \
      --json >"$phase_dir/promotion-output.json"
    chmod 600 "$phase_dir/promotion-receipt.json" \
      "$phase_dir/promotion-output.json"
  )
}

seal_and_complete_phase() {
  local phase_id="$1"
  local source_root="$2"
  local phase_dir="$source_root/target/minco/aws/$phase_id"
  local phase_start="$evidence_root/phases/$phase_id/phase-start-receipt.json"
  local result_file="$phase_dir/multi-release-phase-result.json"
  local result_digest
  local completion_file="$evidence_root/phases/$phase_id/phase-completion-receipt.json"
  local writer_args=()

  if [[ "$phase_id" == "03-prior-rollback" ]]; then
    writer_args=(
      MINCO_MULTI_RELEASE_INITIAL_RELEASE_MANIFEST="$phase_one_release"
      MINCO_MULTI_RELEASE_ROLLBACK_ASSESSMENT="$rollback_assessment"
    )
  fi
  env \
    MINCO_MULTI_RELEASE_PHASE_ID="$phase_id" \
    MINCO_MULTI_RELEASE_PHASE_SOURCE_ROOT="$source_root" \
    MINCO_MULTI_RELEASE_PHASE_START_RECEIPT="$phase_start" \
    MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_start_digest" \
    MINCO_MULTI_RELEASE_PHASE_RESULT_OUTPUT="$result_file" \
    "${writer_args[@]}" \
    scripts/aws/write-multi-release-phase-result.sh \
      >"$shared_evidence_dir/$phase_id-result-output.json"
  chmod 600 "$shared_evidence_dir/$phase_id-result-output.json"
  result_digest="$(shasum -a 256 "$result_file" | awk '{print $1}')"
  MINCO_MULTI_RELEASE_PHASE_ID="$phase_id" \
  MINCO_MULTI_RELEASE_PHASE_RESULT_FILE="$result_file" \
  MINCO_APPROVE_MULTI_RELEASE_PHASE_RESULT_DIGEST="$result_digest" \
  MINCO_APPROVE_MULTI_RELEASE_PHASE_START_RECEIPT_DIGEST="$phase_start_digest" \
    scripts/aws/complete-multi-release-phase.sh \
      >"$shared_evidence_dir/$phase_id-completion-output.json"
  chmod 600 "$shared_evidence_dir/$phase_id-completion-output.json"
  previous_completion_digest="$(jq -er '.receipt_digest' "$completion_file")"
}

begin_next_phase() {
  local phase_id="$1"
  MINCO_MULTI_RELEASE_PHASE_ID="$phase_id" \
  MINCO_APPROVE_PREVIOUS_PHASE_COMPLETION_DIGEST="$previous_completion_digest" \
    scripts/aws/begin-multi-release-phase.sh \
      >"$shared_evidence_dir/$phase_id-start-output.json"
  chmod 600 "$shared_evidence_dir/$phase_id-start-output.json"
  phase_start_digest="$(jq -er '.receipt_digest' \
    "$evidence_root/phases/$phase_id/phase-start-receipt.json")"
}

run_release_phase 01-prior-initial "$prior_root" bounded_create_v1 true
phase_one_release="$prior_root/target/minco/aws/01-prior-initial/release.json"
seal_and_complete_phase 01-prior-initial "$prior_root"
begin_next_phase 02-current

run_release_phase 02-current "$current_root" bounded_release_update_v1 true
seal_and_complete_phase 02-current "$current_root"
begin_next_phase 03-prior-rollback

current_release="$current_root/target/minco/aws/02-current/release.json"
current_release_id="$(jq -er '.release_id' "$current_release")"
target_release_id="$(jq -er '.release_id' "$phase_one_release")"
compatibility_evidence="$current_root/target/minco/aws/02-current/rollback-data-compatibility.json"
jq -n \
  --arg current_release_id "$current_release_id" \
  --arg target_release_id "$target_release_id" \
  '{
    schema_version: 1,
    current_release_id: $current_release_id,
    target_release_id: $target_release_id,
    decision: "compatible",
    reviewed_by: "approved bounded multi-release controller",
    reason: "The exact source diff contains no application, contract, configuration, migration, infrastructure or dependency change; the forward-only schema is retained."
  }' >"$compatibility_evidence"
chmod 600 "$compatibility_evidence"
rollback_assessment="$prior_root/target/minco/aws/03-prior-rollback/rollback-assessment.json"
mkdir -p "$(dirname "$rollback_assessment")"
chmod 700 "$(dirname "$rollback_assessment")"
(
  cd "$current_root"
  cargo minco rollback \
    --current-root "$current_root" \
    --target-root "$prior_root" \
    --current-promotion target/minco/aws/02-current/promotion-receipt.json \
    --target-promotion target/minco/aws/01-prior-initial/promotion-receipt.json \
    --data-compatibility-evidence \
      target/minco/aws/02-current/rollback-data-compatibility.json \
    --json >"$rollback_assessment"
)
chmod 600 "$rollback_assessment"
jq -e '
  .operation == "rollback_compatibility_assessment"
  and .external_aws_contact == false
  and .rebuild == false
  and .replan == false
  and .reverse_sql == false
  and .automatic_data_repair == false
  and .reuse_historical_hosted_report == false
  and .assessment.classification == "compatible"
  and .routing_authorized == true
  and .blockers == []
' "$rollback_assessment" >/dev/null || {
  echo "rollback compatibility assessment did not authorize exact artifact reuse" >&2
  exit 1
}

run_release_phase 03-prior-rollback "$prior_root" bounded_release_update_v1 false
seal_and_complete_phase 03-prior-rollback "$prior_root"
unset MINCO_SMOKE_JWT_TOKEN

jq -n \
  --arg controller_receipt_digest "$controller_digest" \
  --arg final_phase_completion_digest "$previous_completion_digest" \
  '{
    schema_version: 1,
    operation: "bounded_multi_release_smoke",
    state: "provider_phases_succeeded",
    external_aws_contact: true,
    controller_receipt_digest: $controller_receipt_digest,
    final_phase_completion_digest: $final_phase_completion_digest,
    release_sequence: ["prior", "current", "prior"],
    exact_initial_release_reused: true,
    fresh_hosted_verification_per_phase: true,
    cleanup: {
      owner: "root_bootstrap",
      performed: false,
      required: true
    }
  }' >"$shared_evidence_dir/multi-release-provider-completion.json"
chmod 600 "$shared_evidence_dir/multi-release-provider-completion.json"
cat "$shared_evidence_dir/multi-release-provider-completion.json"
