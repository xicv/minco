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
        self.assertIn(
            "receipt-rule-set/Ticketing-inbound-mail-ruleset:receipt-rule/ticketing-inbound-mail",
            source_arn,
        )
        self.assertNotIn("'", source_arn)

        rule = resources["TicketingReceiptRule"]["Properties"]["Rule"]
        self.assertEqual(rule["TlsPolicy"], "Require")
        self.assertEqual(rule["ScanEnabled"], True)
        self.assertEqual(rule["Recipients"], ["support@example.test"])
        # The receipt rule (the consumer of the queue policy) carries the
        # ordering dependency; the bucket must NOT depend on the policy
        # that references its ARN — that is the E3004 circular dependency
        # `sam validate --lint` catches (exact-head review R34).
        self.assertNotIn(
            "DependsOn",
            resources["TicketingRawMailBucket"],
            "the bucket must not depend on the policy referencing its ARN",
        )
        self.assertIn(
            "TicketingMailQueuePolicy",
            resources["TicketingReceiptRule"]["DependsOn"],
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
