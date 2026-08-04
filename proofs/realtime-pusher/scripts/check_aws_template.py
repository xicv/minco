#!/usr/bin/env python3
"""Fail closed when the bounded realtime proof template drifts."""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Any

import yaml


class CloudFormationLoader(yaml.SafeLoader):
    """Preserve CloudFormation intrinsic tags as ordinary data."""


def cloudformation_tag(
    loader: CloudFormationLoader, tag_suffix: str, node: yaml.Node
) -> Any:
    if isinstance(node, yaml.ScalarNode):
        value = loader.construct_scalar(node)
    elif isinstance(node, yaml.SequenceNode):
        value = loader.construct_sequence(node)
    else:
        value = loader.construct_mapping(node)
    return {f"Fn::{tag_suffix}": value}


CloudFormationLoader.add_multi_constructor("!", cloudformation_tag)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def main() -> int:
    require(len(sys.argv) == 2, "usage: check_aws_template.py <template>")
    template_path = Path(sys.argv[1])
    require(template_path.is_file(), f"template does not exist: {template_path}")
    template = yaml.load(template_path.read_text(), Loader=CloudFormationLoader)
    require(isinstance(template, dict), "template root must be a mapping")
    resources = template.get("Resources")
    require(isinstance(resources, dict), "Resources must be a mapping")

    by_type: dict[str, list[tuple[str, dict[str, Any]]]] = {}
    for logical_id, resource in resources.items():
        require(isinstance(resource, dict), f"{logical_id} must be a mapping")
        resource_type = resource.get("Type")
        require(isinstance(resource_type, str), f"{logical_id} requires Type")
        by_type.setdefault(resource_type, []).append((logical_id, resource))

    forbidden = {
        "AWS::EC2::NatGateway",
        "AWS::ECS::Service",
        "AWS::Events::Rule",
        "AWS::Lambda::Alias",
        "AWS::Lambda::ProvisionedConcurrencyConfig",
        "AWS::Scheduler::Schedule",
    }
    require(not (forbidden & by_type.keys()), "template adds fixed or scheduled compute")

    apis = by_type.get("AWS::ApiGatewayV2::Api", [])
    require(len(apis) == 1, "exactly one WebSocket API is required")
    require(
        apis[0][1].get("Properties", {}).get("ProtocolType") == "WEBSOCKET",
        "API must be WebSocket",
    )

    route_keys = {
        route.get("Properties", {}).get("RouteKey")
        for _, route in by_type.get("AWS::ApiGatewayV2::Route", [])
    }
    require(
        route_keys == {"$connect", "$disconnect", "$default"},
        "routes must be exactly $connect, $disconnect and $default",
    )

    stages = by_type.get("AWS::ApiGatewayV2::Stage", [])
    require(len(stages) == 1, "exactly one stage is required")
    stage = stages[0][1].get("Properties", {})
    require(stage.get("AutoDeploy") is False, "proof stage must bind an exact deployment")
    require("DeploymentId" in stage, "proof stage requires an exact deployment")
    require(
        "AccessLogSettings" not in stage,
        "proof must not depend on an account-global API Gateway CloudWatch role",
    )
    throttles = stage.get("DefaultRouteSettings", {})
    require(0 < throttles.get("ThrottlingRateLimit", 0) <= 100, "route rate limit must be bounded")
    require(0 < throttles.get("ThrottlingBurstLimit", 0) <= 200, "route burst limit must be bounded")

    functions = by_type.get("AWS::Lambda::Function", [])
    require(
        {name for name, _ in functions} == {"Handler"},
        "the proof must use one bounded handler",
    )
    for logical_id, function in functions:
        properties = function.get("Properties", {})
        require(properties.get("Runtime") == "provided.al2023", f"{logical_id} must use provided.al2023")
        require(properties.get("Architectures") == ["arm64"], f"{logical_id} must be arm64")
        require("VpcConfig" not in properties, f"{logical_id} must not require a VPC or NAT")
        require("ProvisionedConcurrencyConfig" not in properties, f"{logical_id} must not provision concurrency")
        code = properties.get("Code", {})
        require("S3ObjectVersion" in code, f"{logical_id} must bind an exact S3 object version")
        require(1 <= properties.get("Timeout", 0) <= 15, f"{logical_id} timeout must be bounded")
        require(128 <= properties.get("MemorySize", 0) <= 512, f"{logical_id} memory must be bounded")

    tables = by_type.get("AWS::DynamoDB::Table", [])
    require(len(tables) == 1, "exactly one DynamoDB state table is required")
    table = tables[0][1]
    table_properties = table.get("Properties", {})
    require(table_properties.get("BillingMode") == "PAY_PER_REQUEST", "DynamoDB must use on-demand capacity")
    require(table_properties.get("TimeToLiveSpecification", {}).get("Enabled") is True, "connection state requires TTL")
    require(table.get("DeletionPolicy") == "Delete", "proof table must be deleted with the stack")
    require(table.get("UpdateReplacePolicy") == "Delete", "replaced proof table must be deleted")

    retry_configs = by_type.get("AWS::Lambda::EventInvokeConfig", [])
    require(len(retry_configs) == 1, "self-invoked initializer needs one explicit retry policy")
    retry = retry_configs[0][1].get("Properties", {})
    require(retry.get("MaximumRetryAttempts") == 0, "service-managed initializer retries must be disabled")
    require(1 <= retry.get("MaximumEventAgeInSeconds", 0) <= 60, "initializer event age must be bounded")

    log_groups = by_type.get("AWS::Logs::LogGroup", [])
    require(len(log_groups) == 1, "Lambda requires one explicit retained log group")
    for logical_id, log_group in log_groups:
        retention = log_group.get("Properties", {}).get("RetentionInDays", 0)
        require(1 <= retention <= 14, f"{logical_id} retention must be between 1 and 14 days")
        require(log_group.get("DeletionPolicy") == "Delete", f"{logical_id} must delete with the proof stack")

    for logical_id, role in by_type.get("AWS::IAM::Role", []):
        policies = role.get("Properties", {}).get("Policies", [])
        for policy in policies:
            statements = policy.get("PolicyDocument", {}).get("Statement", [])
            for statement in statements:
                actions = statement.get("Action", [])
                actions = [actions] if isinstance(actions, str) else actions
                require("*" not in actions, f"{logical_id} contains a wildcard action")
                resources_for_statement = statement.get("Resource", [])
                resources_for_statement = (
                    [resources_for_statement]
                    if isinstance(resources_for_statement, (str, dict))
                    else resources_for_statement
                )
                require(
                    "*" not in resources_for_statement,
                    f"{logical_id} contains a global wildcard resource",
                )

    serialized = template_path.read_text()
    require(
        "/*/GET/@connections/*" in serialized
        and "/*/POST/@connections/*" in serialized,
        "management IAM must cover exact visibility and callback methods",
    )
    require(
        "${RealtimeApi.ExecutionArn}" not in serialized,
        "Lambda permission must use a supported execute-api ARN",
    )
    for forbidden_text in ["SecretValue", "APP_SECRET="]:
        require(
            forbidden_text not in serialized,
            f"template contains forbidden secret text: {forbidden_text!r}",
        )

    print("Realtime Pusher AWS template policy passed.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (TypeError, ValueError, yaml.YAMLError) as error:
        print(f"realtime-pusher template check failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
