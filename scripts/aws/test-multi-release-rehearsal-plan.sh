#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

fixture_dir="$(mktemp -d)"
fixture_dir="$(cd "$fixture_dir" && pwd -P)"
cleanup_fixture() {
  rm -r -- "$fixture_dir"
}
trap cleanup_fixture EXIT

create_checkout() {
  local root="$1"
  local role="$2"

  mkdir -p "$root"
  git -C "$root" init -q
  git -C "$root" config user.email minco-test@example.invalid
  git -C "$root" config user.name "Minco test"
  mkdir -p "$root/nested"
  printf '[workspace]\nmembers = []\n# %s\n' "$role" >"$root/Cargo.toml"
  printf 'schema_version = 1\n' >"$root/minco.toml"
  printf 'schema_version = 1\n' >"$root/nested/minco.toml"
  git -C "$root" add Cargo.toml minco.toml nested/minco.toml
  git -C "$root" commit -qm "$role release"
}

prior_root="$fixture_dir/prior"
current_root="$fixture_dir/current"
evidence_root="$fixture_dir/evidence/target/minco/aws/reviewed-multi-release-run"
create_checkout "$prior_root" prior
create_checkout "$current_root" current
prior_revision="$(git -C "$prior_root" rev-parse HEAD)"
current_revision="$(git -C "$current_root" rev-parse HEAD)"

database_boundary='{"mode":"existing-ssm-secure-string","parameter_name":"/minco/rehearsal/database-url","parameter_owned":false,"instance_owned":false}'
approved_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
expires_at="$(jq -nr 'now + 3600 | todateiso8601')"
authority_file="$fixture_dir/multi-release-authority.json"
jq -n \
  --arg approved_at "$approved_at" \
  --arg expires_at "$expires_at" \
  --arg prior_revision "$prior_revision" \
  --arg current_revision "$current_revision" \
  --argjson database_boundary "$database_boundary" \
  '{
    schema_version: 1,
    authority_kind: "minco.aws-multi-release-controller-rehearsal.v1",
    run_id: "reviewed-multi-release-run",
    source_revisions: {
      current: $current_revision,
      prior: $prior_revision
    },
    release_sequence: ["prior", "current", "prior"],
    expected_account_id: "123456789012",
    expected_region: "ap-southeast-2",
    expected_role_arn: "arn:aws:iam::123456789012:role/minco-rehearsal",
    aws_profile: "minco-rehearsal",
    environment: "dev",
    database_boundary: $database_boundary,
    resource_allowlist: "bounded-multi-release-smoke-v1",
    cleanup_blast_radius: "cleanup-bounded-multi-release-smoke-v1",
    max_duration_minutes: 60,
    max_spend_usd: 25,
    approved_by: "release-owner",
    approved_at: $approved_at,
    expires_at: $expires_at
  }' >"$authority_file"
approval_digest="$(shasum -a 256 "$authority_file" | awk '{print $1}')"

fake_bin="$fixture_dir/fake-bin"
provider_contact_log="$fixture_dir/provider-contact.log"
mkdir -p "$fake_bin"
for command in aws cargo curl psql sam uv; do
  # The generated command must expand these variables only when invoked.
  # shellcheck disable=SC2016
  printf '%s\n' \
    '#!/usr/bin/env bash' \
    'printf "%s\\n" "$(basename "$0")" >>"$MINCO_PROVIDER_CONTACT_LOG"' \
    'exit 99' >"$fake_bin/$command"
  chmod +x "$fake_bin/$command"
done

plan_rehearsal() {
  local selected_prior_root="$1"
  local selected_current_root="$2"
  local selected_evidence_root="${3:-$evidence_root}"

  PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_PRIOR_ROOT="$selected_prior_root" \
  MINCO_CURRENT_ROOT="$selected_current_root" \
  MINCO_MULTI_RELEASE_EVIDENCE_ROOT="$selected_evidence_root" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_AWS_RUN_ID=reviewed-multi-release-run \
  MINCO_REHEARSAL_PROFILE=minco-rehearsal \
  AWS_REGION=ap-southeast-2 \
  MINCO_REHEARSAL_DATABASE_BOUNDARY_JSON="$database_boundary" \
  MINCO_REHEARSAL_RESOURCE_ALLOWLIST=bounded-multi-release-smoke-v1 \
  MINCO_REHEARSAL_CLEANUP_BLAST_RADIUS=cleanup-bounded-multi-release-smoke-v1 \
    scripts/aws/plan-multi-release-rehearsal.sh
}

plan="$fixture_dir/plan.json"
plan_rehearsal "$prior_root" "$current_root" >"$plan"

[[ ! -e "$provider_contact_log" ]] || {
  echo "multi-release planning contacted a provider or build command" >&2
  exit 1
}

jq -e \
  --arg prior_root "$(cd "$prior_root" && pwd -P)" \
  --arg current_root "$(cd "$current_root" && pwd -P)" \
  --arg evidence_root "$(dirname "$evidence_root")/$(basename "$evidence_root")" \
  --arg prior_revision "$prior_revision" \
  --arg current_revision "$current_revision" \
  '
    keys == [
      "authority",
      "cleanup",
      "evidence_root",
      "external_aws_contact",
      "operation",
      "phases",
      "provider_boundary",
      "rollback",
      "schema_version"
    ]
    and .schema_version == 1
    and .operation == "multi_release_controller_rehearsal"
    and .external_aws_contact == false
    and .authority == {
      approval_digest: .authority.approval_digest,
      kind: "minco.aws-multi-release-controller-rehearsal.v1",
      run_id: "reviewed-multi-release-run"
    }
    and (.authority.approval_digest | test("^[0-9a-f]{64}$"))
    and .evidence_root == $evidence_root
    and .provider_boundary == {
      artifact_bucket_lifetime: "whole_run",
      shared_stack: true,
      stack_lifecycle: ["create", "update", "update", "delete"]
    }
    and .rollback == {
      accepted_result: "compatible",
      compatibility_assessment_required: true,
      current_promotion_phase: "02-current",
      historical_hosted_report_reuse: false,
      target_promotion_phase: "01-prior-initial"
    }
    and .cleanup == {
      after_phase: "03-prior-rollback",
      inner_phase_cleanup: false,
      owner: "parent_controller",
      trap_count: 1
    }
    and [.phases[].release] == ["prior", "current", "prior"]
    and [.phases[].stack_action] == ["create", "update", "update"]
    and [.phases[].evidence_namespace] == [
      "phases/01-prior-initial",
      "phases/02-current",
      "phases/03-prior-rollback"
    ]
    and ([.phases[].evidence_write_policy] | all(. == "create_only"))
    and .phases[0].source == {
      revision: $prior_revision,
      root: $prior_root
    }
    and .phases[1].source == {
      revision: $current_revision,
      root: $current_root
    }
    and .phases[2].source == {
      revision: $prior_revision,
      root: $prior_root
    }
    and .phases[0].artifact_policy == {
      build: true,
      replan: true,
      reuse_exact_release_from_phase: null
    }
    and .phases[1].artifact_policy == {
      build: true,
      replan: true,
      reuse_exact_release_from_phase: null
    }
    and .phases[2].artifact_policy == {
      build: false,
      replan: false,
      reuse_exact_release_from_phase: "01-prior-initial"
    }
    and ([.phases[].fresh_hosted_verification] | all)
    and ([.phases[].promotion_required] | all)
  ' "$plan" >/dev/null || {
  echo "multi-release plan omitted or weakened the exact phase lifecycle" >&2
  jq . "$plan" >&2
  exit 1
}

plan_digest="$(shasum -a 256 "$plan" | awk '{print $1}')"
phase_projection="$fixture_dir/phase-projection.json"
PATH="$fake_bin:$PATH" \
MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
MINCO_MULTI_RELEASE_PHASE_ID=03-prior-rollback \
  scripts/aws/plan-multi-release-phase.sh >"$phase_projection"

jq -e \
  --arg controller_root "$(cd "$current_root" && pwd -P)" \
  --arg prior_root "$(cd "$prior_root" && pwd -P)" \
  --arg prior_revision "$prior_revision" \
  --arg evidence_root "$evidence_root" \
  --arg plan_digest "$plan_digest" \
  '
    keys == [
      "authority",
      "controller",
      "evidence",
      "external_aws_contact",
      "operation",
      "phase",
      "plan_digest",
      "rollback",
      "schema_version"
    ]
    and .schema_version == 1
    and .operation == "multi_release_phase"
    and .external_aws_contact == false
    and .plan_digest == $plan_digest
    and .authority == {
      approval_digest: .authority.approval_digest,
      kind: "minco.aws-multi-release-controller-rehearsal.v1",
      run_id: "reviewed-multi-release-run"
    }
    and .controller == {
      cleanup_owner: "parent_controller",
      root: $controller_root
    }
    and .evidence == {
      namespace: "phases/03-prior-rollback",
      path: ($evidence_root + "/phases/03-prior-rollback"),
      write_policy: "create_only"
    }
    and .phase.id == "03-prior-rollback"
    and .phase.release == "prior"
    and .phase.source == {
      revision: $prior_revision,
      root: $prior_root
    }
    and .phase.stack_action == "update"
    and .phase.artifact_policy == {
      build: false,
      replan: false,
      reuse_exact_release_from_phase: "01-prior-initial"
    }
    and .phase.fresh_hosted_verification == true
    and .phase.promotion_required == true
    and .rollback == {
      accepted_result: "compatible",
      compatibility_assessment_required: true,
      current_promotion_phase: "02-current",
      historical_hosted_report_reuse: false,
      target_promotion_phase: "01-prior-initial"
    }
  ' "$phase_projection" >/dev/null || {
  echo "phase projection omitted or weakened the sealed rollback handoff" >&2
  jq . "$phase_projection" >&2
  exit 1
}

first_phase_projection="$fixture_dir/first-phase-projection.json"
PATH="$fake_bin:$PATH" \
MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
MINCO_MULTI_RELEASE_PHASE_ID=01-prior-initial \
  MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
  MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  scripts/aws/plan-multi-release-phase.sh >"$first_phase_projection"
first_phase_evidence_path="$(jq -er '.evidence.path' "$first_phase_projection")"
mkdir -p "$first_phase_evidence_path"

PATH="$fake_bin:$PATH" \
MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
MINCO_MULTI_RELEASE_PHASE_ID=02-current \
  MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
  MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  scripts/aws/plan-multi-release-phase.sh >/dev/null || {
  echo "earlier phase evidence blocked the next exact-source handoff" >&2
  exit 1
}

if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_PHASE_ID=02-current \
  MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
  MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST=0000000000000000000000000000000000000000000000000000000000000000 \
  scripts/aws/plan-multi-release-phase.sh >/dev/null 2>&1; then
  echo "phase projection accepted an authority digest outside the whole-run plan" >&2
  exit 1
fi

broadened_plan="$fixture_dir/broadened-plan.json"
jq '.cleanup.owner = "inner_phase"' "$plan" >"$broadened_plan"
broadened_digest="$(shasum -a 256 "$broadened_plan" | awk '{print $1}')"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_PLAN_FILE="$broadened_plan" \
  MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$broadened_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=03-prior-rollback \
  scripts/aws/plan-multi-release-phase.sh >/dev/null 2>&1; then
  echo "phase projection accepted a digest-matched plan outside fixed policy" >&2
  exit 1
fi

traversal_root="$fixture_dir/missing/../current/target/minco/aws/reviewed-multi-release-run"
traversal_plan="$fixture_dir/traversal-plan.json"
jq --arg evidence_root "$traversal_root" \
  '.evidence_root = $evidence_root' "$plan" >"$traversal_plan"
traversal_digest="$(shasum -a 256 "$traversal_plan" | awk '{print $1}')"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_PLAN_FILE="$traversal_plan" \
  MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$traversal_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=02-current \
  scripts/aws/plan-multi-release-phase.sh >/dev/null 2>&1; then
  echo "phase projection accepted an evidence path traversing into a source checkout" >&2
  exit 1
fi

phase_evidence_path="$evidence_root/phases/03-prior-rollback"
mkdir -p "$phase_evidence_path"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
  MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  MINCO_MULTI_RELEASE_PHASE_ID=03-prior-rollback \
  scripts/aws/plan-multi-release-phase.sh >/dev/null 2>&1; then
  echo "phase projection accepted a pre-existing create-only evidence namespace" >&2
  exit 1
fi
rm -r -- "$evidence_root"

printf '# post-plan drift\n' >>"$current_root/minco.toml"
if PATH="$fake_bin:$PATH" \
  MINCO_PROVIDER_CONTACT_LOG="$provider_contact_log" \
  MINCO_MULTI_RELEASE_PHASE_ID=02-current \
  MINCO_MULTI_RELEASE_PLAN_FILE="$plan" \
  MINCO_APPROVE_MULTI_RELEASE_PLAN_DIGEST="$plan_digest" \
  MINCO_REHEARSAL_AUTHORITY_FILE="$authority_file" \
  MINCO_APPROVE_REHEARSAL_AUTHORITY_DIGEST="$approval_digest" \
  scripts/aws/plan-multi-release-phase.sh >/dev/null 2>&1; then
  echo "phase projection accepted source drift after whole-run planning" >&2
  exit 1
fi
git -C "$current_root" restore minco.toml

if (
  cd "$fixture_dir"
  plan_rehearsal prior "$current_root" >/dev/null 2>&1
); then
  echo "multi-release planning accepted a relative prior root" >&2
  exit 1
fi

if plan_rehearsal "$prior_root" "$prior_root" >/dev/null 2>&1; then
  echo "multi-release planning accepted one checkout for both releases" >&2
  exit 1
fi

if plan_rehearsal \
  "$prior_root" "$current_root" \
  "$current_root/target/minco/aws/reviewed-multi-release-run" \
  >/dev/null 2>&1; then
  echo "multi-release planning accepted an evidence root inside a source checkout" >&2
  exit 1
fi

if plan_rehearsal \
  "$prior_root" "$current_root" "$traversal_root" \
  >/dev/null 2>&1; then
  echo "multi-release planning accepted traversal in the evidence root" >&2
  exit 1
fi

mkdir -p "$evidence_root"
if plan_rehearsal "$prior_root" "$current_root" >/dev/null 2>&1; then
  echo "multi-release planning accepted a pre-existing whole-run evidence root" >&2
  exit 1
fi
rm -r -- "$evidence_root"

prior_link="$fixture_dir/prior-link"
ln -s "$prior_root" "$prior_link"
if plan_rehearsal "$prior_link" "$current_root" >/dev/null 2>&1; then
  echo "multi-release planning accepted a symlink checkout root" >&2
  exit 1
fi

if plan_rehearsal "$prior_root/nested" "$current_root" >/dev/null 2>&1; then
  echo "multi-release planning accepted a nested directory as a checkout root" >&2
  exit 1
fi

printf '# uncommitted\n' >>"$current_root/minco.toml"
if plan_rehearsal "$prior_root" "$current_root" >/dev/null 2>&1; then
  echo "multi-release planning accepted a dirty current checkout" >&2
  exit 1
fi
git -C "$current_root" restore minco.toml

printf '# new revision\n' >>"$current_root/minco.toml"
git -C "$current_root" add minco.toml
git -C "$current_root" commit -qm "unexpected current revision"
if plan_rehearsal "$prior_root" "$current_root" >/dev/null 2>&1; then
  echo "multi-release planning accepted a source revision outside its authority" >&2
  exit 1
fi

[[ ! -e "$provider_contact_log" ]] || {
  echo "a rejected multi-release plan contacted a provider or build command" >&2
  exit 1
}

printf 'Multi-release rehearsal plan checks passed.\n'
