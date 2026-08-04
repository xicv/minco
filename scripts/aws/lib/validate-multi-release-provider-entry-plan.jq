def revision:
  type == "string" and test("^[0-9a-f]{40}([0-9a-f]{24})?$");

def digest:
  type == "string" and test("^[0-9a-f]{64}$");

keys == [
  "authority",
  "cleanup",
  "controller",
  "external_aws_contact",
  "operation",
  "phase",
  "provider",
  "schema_version"
]
and .schema_version == 1
and .operation == "multi_release_provider_entry"
and .external_aws_contact == false
and (.controller | keys) == ["plan_digest", "receipt_digest"]
and (.controller.plan_digest | digest)
and (.controller.receipt_digest | digest)
and (.authority | keys) == ["approval_digest", "kind", "run_id"]
and .authority.kind == "minco.aws-multi-release-controller-rehearsal.v1"
and (.authority.approval_digest | digest)
and (.authority.run_id | test("^[A-Za-z0-9][A-Za-z0-9._-]{0,47}$"))
and (.phase | keys) == [
  "id",
  "projection_digest",
  "source_revision",
  "start_receipt_digest"
]
and .phase.id == "01-prior-initial"
and (.phase.projection_digest | digest)
and (.phase.source_revision | revision)
and (.phase.start_receipt_digest | digest)
and .provider == {
  action: "sts_get_caller_identity",
  expected_region: .provider.expected_region,
  mutation: false,
  secrets_requested: false
}
and (.provider.expected_region | test("^[a-z]{2}(-gov)?-[a-z]+-[0-9]+$"))
and .cleanup == {
  owner: "parent_controller",
  required: true,
  trap_count: 1
}
