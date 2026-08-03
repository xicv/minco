def revision:
  type == "string" and test("^[0-9a-f]{40}([0-9a-f]{24})?$");

def digest:
  type == "string" and test("^[0-9a-f]{64}$");

keys == [
  "authority",
  "cleanup",
  "execution",
  "external_aws_contact",
  "operation",
  "plan_digest",
  "provider_boundary",
  "receipt_digest",
  "schema_version",
  "source_revisions",
  "state"
]
and .schema_version == 1
and .operation == "multi_release_controller_rehearsal"
and .state == "initialized"
and .external_aws_contact == false
and (.plan_digest | digest)
and (.receipt_digest | digest)
and (.authority | keys) == ["approval_digest", "kind", "run_id"]
and .authority.kind == "minco.aws-multi-release-controller-rehearsal.v1"
and (.authority.approval_digest | digest)
and (.authority.run_id | test("^[A-Za-z0-9][A-Za-z0-9._-]{0,47}$"))
and (.source_revisions | keys) == ["current", "prior"]
and (.source_revisions.current | revision)
and (.source_revisions.prior | revision)
and .source_revisions.current != .source_revisions.prior
and (.execution | keys) == ["next_phase", "phase_sequence", "phases"]
and .execution.phase_sequence == [
  "01-prior-initial",
  "02-current",
  "03-prior-rollback"
]
and .execution.next_phase == "01-prior-initial"
and (.execution.phases | length) == 3
and ([.execution.phases[].id] == .execution.phase_sequence)
and ([.execution.phases[].evidence_namespace] == [
  "phases/01-prior-initial",
  "phases/02-current",
  "phases/03-prior-rollback"
])
and ([.execution.phases[] | keys] | all(. == [
  "evidence_namespace",
  "id",
  "projection_digest",
  "state"
]))
and ([.execution.phases[].projection_digest | digest] | all)
and ([.execution.phases[].state] | all(. == "pending"))
and .provider_boundary == {
  artifact_bucket_state: "not_created",
  shared_stack_state: "not_created"
}
and .cleanup == {
  owner: "parent_controller",
  required: true,
  state: "pending",
  trap_count: 1
}
