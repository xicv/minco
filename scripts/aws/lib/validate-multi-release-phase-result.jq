def revision:
  type == "string" and test("^[0-9a-f]{40}([0-9a-f]{24})?$");

def digest:
  type == "string" and test("^[0-9a-f]{64}$");

keys == [
  "artifacts",
  "cleanup",
  "external_aws_contact",
  "operation",
  "phase",
  "rollback",
  "schema_version",
  "state",
  "verification"
]
and .schema_version == 1
and .operation == "multi_release_phase_result"
and .state == "succeeded"
and .external_aws_contact == true
and (.phase | keys) == ["evidence_id", "id", "source_revision"]
and (.phase.source_revision | revision)
and .phase.evidence_id == .phase.id
and (.artifacts | keys) == [
  "change_set_receipt_digest",
  "deployment_receipt_digest",
  "hosted_verification_digest",
  "migration_plan_digest",
  "migration_receipt_digest",
  "promotion_receipt_digest",
  "release_manifest_digest"
]
and ([.artifacts[]] | all(digest))
and (.rollback | keys) == [
  "assessment_digest",
  "exact_initial_release_reused",
  "reused_release_manifest_digest"
]
and .verification == {
  fresh: true,
  historical_report_reused: false
}
and .cleanup == {
  owner: "parent_controller",
  performed: false
}
and (
  if .phase.id == "01-prior-initial" or .phase.id == "02-current" then
    .rollback == {
      assessment_digest: null,
      exact_initial_release_reused: false,
      reused_release_manifest_digest: null
    }
  elif .phase.id == "03-prior-rollback" then
    (.rollback.assessment_digest | digest)
    and .rollback.exact_initial_release_reused == true
    and (.rollback.reused_release_manifest_digest | digest)
    and .rollback.reused_release_manifest_digest
      == .artifacts.release_manifest_digest
  else
    false
  end
)
