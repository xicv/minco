def revision:
  type == "string" and test("^[0-9a-f]{40}([0-9a-f]{24})?$");

def absolute_root:
  type == "string" and startswith("/");

def normalized_absolute_root:
  absolute_root
  and (contains("//") | not)
  and (contains("/../") | not)
  and (endswith("/..") | not)
  and (contains("/./") | not)
  and (endswith("/.") | not);

def source_shape:
  (keys == ["revision", "root"])
  and (.revision | revision);

def artifact_shape:
  keys == ["build", "replan", "reuse_exact_release_from_phase"];

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
and (.authority | keys) == ["approval_digest", "kind", "run_id"]
and .authority.kind == "minco.aws-multi-release-controller-rehearsal.v1"
and (.authority.approval_digest | test("^[0-9a-f]{64}$"))
and (.authority.run_id | test("^[A-Za-z0-9][A-Za-z0-9._-]{0,47}$"))
and (.evidence_root | normalized_absolute_root)
and (
  .authority.run_id as $run_id
  | (.evidence_root | endswith("/target/minco/aws/" + $run_id))
)
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
and (.phases | length) == 3
and ([.phases[].id] == ["01-prior-initial", "02-current", "03-prior-rollback"])
and ([.phases[].release] == ["prior", "current", "prior"])
and ([.phases[].stack_action] == ["create", "update", "update"])
and ([.phases[].evidence_namespace] == [
  "phases/01-prior-initial",
  "phases/02-current",
  "phases/03-prior-rollback"
])
and ([.phases[].evidence_write_policy] | all(. == "create_only"))
and ([.phases[].fresh_hosted_verification] | all(. == true))
and ([.phases[].promotion_required] | all(. == true))
and ([.phases[] | keys] | all(. == [
  "artifact_policy",
  "evidence_namespace",
  "evidence_write_policy",
  "fresh_hosted_verification",
  "id",
  "promotion_required",
  "release",
  "source",
  "stack_action"
]))
and ([.phases[].source | source_shape] | all(. == true))
and ([.phases[].source.root | normalized_absolute_root] | all(. == true))
and ([.phases[].artifact_policy | artifact_shape] | all(. == true))
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
and .phases[0].source == .phases[2].source
and .phases[0].source.root != .phases[1].source.root
and .phases[0].source.revision != .phases[1].source.revision
