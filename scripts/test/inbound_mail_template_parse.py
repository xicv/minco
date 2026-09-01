#!/usr/bin/env python3
"""Structural YAML validation of the rendered inbound-mail SAM template
(exact-head review R14/P0-4).

Substring assertions missed a real defect: nested quoting inside a
quoted !Sub scalar produced invalid YAML. This gate renders the
template through the real example binary, parses the COMPLETE document
with a loader that understands CloudFormation intrinsic tags as
structural nodes, and asserts the SES chain's shape.
"""
from __future__ import annotations

import subprocess
import unittest

ROOT = __import__("os").path.dirname(
    __import__("os").path.dirname(
        __import__("os").path.dirname(__import__("os").path.abspath(__file__))
    )
)


def render_template() -> str:
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--locked",
            "-p",
            "minco-plan",
            "--example",
            "render_inbound_mail",
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return result.stdout


def cfn_loader():
    """A SafeLoader derivative that treats any CloudFormation ``!Tag``
    node structurally: mappings and sequences keep their shape, scalars
    come back as strings."""
    import yaml

    class CfnLoader(yaml.SafeLoader):
        pass

    def construct_tagged(loader, tag_suffix, node):
        if isinstance(node, yaml.MappingNode):
            return loader.construct_mapping(node)
        if isinstance(node, yaml.SequenceNode):
            return loader.construct_sequence(node)
        return loader.construct_scalar(node)

    yaml.add_multi_constructor("!", construct_tagged, CfnLoader)
    return CfnLoader


class InboundMailTemplateParseTests(unittest.TestCase):
    def test_full_template_parses_and_carries_the_ses_chain(self) -> None:
        import yaml

        template = render_template()
        document = yaml.load(template, Loader=cfn_loader())
        self.assertIsInstance(document, dict, "the template must parse as a mapping")
        resources = document["Resources"]
        self.assertIsInstance(resources, dict)

        policy = resources["TicketingRawMailBucketPolicy"]
        statement = policy["Properties"]["PolicyDocument"]["Statement"][0]
        # The SES write grant: prefix-scoped resource and the exact
        # receipt-rule ARN, each quoted exactly once.
        resource = str(statement["Resource"])
        self.assertTrue(
            resource.startswith("${TicketingRawMailBucket.Arn}/mail/*"),
            f"resource must be the prefix-scoped Sub, got {resource!r}",
        )
        self.assertNotIn(
            "'", resource, "no nested quotes inside the quoted !Sub scalar"
        )
        source_arn = str(statement["Condition"]["ArnLike"]["aws:SourceArn"])
        # The rule-set identity is a stable deployment digest (exact-head
        # review 5072859042): application, environment, the sidecar
        # marker and a hex digest — never the first binding's logical id.
        rule_set_segment = next(
            (
                part
                for part in source_arn.split(":")
                if "-inbound-mail-" in part and "/" in part
            ),
            None,
        )
        self.assertIsNotNone(rule_set_segment, f"no rule-set identity in {source_arn!r}")
        assert rule_set_segment is not None
        identity = rule_set_segment.rsplit("/", 1)[-1]
        self.assertTrue(
            identity.startswith("orders-dev-inbound-mail-"),
            f"identity carries app+environment: {identity!r}",
        )
        digest = identity.rsplit("-", 1)[-1]
        self.assertEqual(len(digest), 12, "bounded hex digest suffix")
        self.assertTrue(all(c in "0123456789abcdef" for c in digest))
        self.assertTrue(
            source_arn.endswith(":receipt-rule/ticketing-inbound-mail"),
            f"the source ARN targets the receipt rule: {source_arn!r}",
        )
        self.assertNotIn("'", source_arn)

        rule = resources["TicketingReceiptRule"]["Properties"]["Rule"]
        self.assertEqual(rule["TlsPolicy"], "Require")
        self.assertEqual(rule["ScanEnabled"], True)
        self.assertEqual(rule["Recipients"], ["support@example.test"])

        # Clean-create dependency graph (exact-head review 5083559431
        # P0-1/P0-2): the queue policy's SourceArn uses the EXPLICIT
        # bucket name (never !GetAtt the bucket resource), the bucket
        # waits for the queue policy (S3 validates the notification
        # destination permission at bucket-creation time), and the
        # receipt rule carries a REAL dependency on the rule set (!Ref)
        # plus the SES-write bucket policy.
        queue_policy = resources["TicketingMailQueuePolicy"]
        queue_condition = queue_policy["Properties"]["PolicyDocument"]["Statement"][0]
        queue_source = str(queue_condition["Condition"]["ArnLike"]["aws:SourceArn"])
        self.assertIn(
            "arn:${AWS::Partition}:s3:::orders-dev-raw-mail",
            queue_source,
            f"the queue policy must not reference the bucket resource: {queue_source!r}",
        )
        self.assertIn(
            "TicketingMailQueuePolicy",
            resources["TicketingRawMailBucket"]["DependsOn"],
            "the bucket must wait for the queue policy",
        )
        rule_depends = resources["TicketingReceiptRule"]["DependsOn"]
        self.assertIn("TicketingRawMailBucketPolicy", rule_depends)
        self.assertIn("TicketingMailQueuePolicy", rule_depends)
        # The tag-aware loader renders !Ref as the bare logical id, so
        # equality with the LOGICAL ID (not the rule-set name) proves a
        # reference; a literal would carry the digest identity string.
        self.assertEqual(
            resources["TicketingReceiptRule"]["Properties"]["RuleSetName"],
            "InboundMailReceiptRuleSet",
            "the rule must reference the rule set with !Ref, not a literal",
        )

    def test_sam_validate_lint_accepts_the_template(self) -> None:
        """The full cfn-lint pass (exact-head review R34): `sam validate
        --lint` runs the CloudFormation resource specification checks the
        structural parse cannot. The SAM CLI is materialized through
        `uv tool run` so the gate is reproducible."""
        import shutil
        import tempfile

        template = render_template()
        with tempfile.NamedTemporaryFile(
            "w", suffix=".yaml", delete=False
        ) as handle:
            handle.write(template)
            path = handle.name
        command = [
            "uv",
            "tool",
            "run",
            "--from",
            "aws-sam-cli",
            "sam",
            "validate",
            "--lint",
            "--template",
            path,
        ]
        if shutil.which("uv") is None:
            self.skipTest("uv is not available to materialize the SAM CLI")
        result = subprocess.run(
            command, capture_output=True, text=True, timeout=300
        )
        combined = result.stdout + result.stderr
        self.assertEqual(
            result.returncode,
            0,
            f"sam validate --lint rejected the template:\n{combined}",
        )


if __name__ == "__main__":
    unittest.main()
