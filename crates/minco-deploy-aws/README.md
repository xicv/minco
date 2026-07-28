# minco-deploy-aws

Fail-closed environment guards and deterministic CloudFormation change-set
review primitives for Minco's AWS deployment controller.

The crate models review and authorization. It does not execute a change set or
claim that CloudFormation success proves application runtime health.
