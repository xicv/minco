#!/usr/bin/env python3
"""Fail closed when the bounded AppSync live-proof template drifts."""

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


def one(by_type: dict[str, list[tuple[str, dict[str, Any]]]], kind: str) -> dict[str, Any]:
    resources = by_type.get(kind, [])
    require(len(resources) == 1, f"exactly one {kind} is required")
    return resources[0][1]


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

    allowed = {
        "AWS::AppSync::Api",
        "AWS::AppSync::ChannelNamespace",
        "AWS::Cognito::UserPool",
        "AWS::Cognito::UserPoolClient",
        "AWS::IAM::Role",
        "AWS::Lambda::Function",
        "AWS::Logs::LogGroup",
    }
    require(set(by_type) == allowed, "template resource types exceed the approved allowlist")

    api = one(by_type, "AWS::AppSync::Api").get("Properties", {})
    event_config = api.get("EventConfig", {})
    providers = event_config.get("AuthProviders", [])
    require(
        {provider.get("AuthType") for provider in providers}
        == {"AWS_IAM", "AMAZON_COGNITO_USER_POOLS"},
        "AppSync auth providers must be exactly IAM and Cognito user pools",
    )
    cognito = next(
        (
            provider.get("CognitoConfig", {})
            for provider in providers
            if provider.get("AuthType") == "AMAZON_COGNITO_USER_POOLS"
        ),
        {},
    )
    require(
        cognito.get("UserPoolId") == {"Fn::Ref": "ProofUserPool"},
        "Cognito authorization must use the disposable proof user pool",
    )
    require(
        cognito.get("AwsRegion") == {"Fn::Ref": "AWS::Region"},
        "Cognito authorization must use the stack Region",
    )
    require(
        cognito.get("AppIdClientRegex") == {"Fn::Sub": "^${ProofUserPoolClient}$"},
        "Cognito authorization must accept only the disposable proof client",
    )
    require(
        event_config.get("ConnectionAuthModes")
        == [{"AuthType": "AMAZON_COGNITO_USER_POOLS"}],
        "browser connection must be Cognito-only",
    )
    require(
        event_config.get("DefaultPublishAuthModes") == [{"AuthType": "AWS_IAM"}],
        "publication must be IAM-only",
    )
    require(
        event_config.get("DefaultSubscribeAuthModes")
        == [{"AuthType": "AMAZON_COGNITO_USER_POOLS"}],
        "subscription must be Cognito-only",
    )

    namespace = one(by_type, "AWS::AppSync::ChannelNamespace").get("Properties", {})
    require(namespace.get("Name") == "orders", "proof namespace must be orders")
    require(
        namespace.get("PublishAuthModes") == [{"AuthType": "AWS_IAM"}],
        "namespace publication must be IAM-only",
    )
    require(
        namespace.get("SubscribeAuthModes")
        == [{"AuthType": "AMAZON_COGNITO_USER_POOLS"}],
        "namespace subscription must be Cognito-only",
    )
    handlers = namespace.get("CodeHandlers", "")
    require("ctx.identity.sub" in handlers, "onSubscribe must use the Cognito sub identity")
    require(
        "ctx.info.channel.path" in handlers,
        "onSubscribe must authorize the complete requested channel path",
    )
    require(
        "`/orders/${claim}/orders`" in handlers,
        "onSubscribe must bind the exact application channel to sub",
    )
    require("util.unauthorized()" in handlers, "onSubscribe must fail closed")

    user_pool = one(by_type, "AWS::Cognito::UserPool")
    require(user_pool.get("DeletionPolicy") == "Delete", "temporary user pool must delete with the stack")
    client = one(by_type, "AWS::Cognito::UserPoolClient").get("Properties", {})
    require(client.get("GenerateSecret") is False, "temporary browser client must not create a secret")
    require(
        client.get("ExplicitAuthFlows") == ["ALLOW_ADMIN_USER_PASSWORD_AUTH"],
        "temporary client must allow only bounded admin password auth",
    )

    function = one(by_type, "AWS::Lambda::Function").get("Properties", {})
    require(function.get("Runtime") == "provided.al2023", "publisher must use provided.al2023")
    require(function.get("Architectures") == ["arm64"], "publisher must be arm64")
    require("VpcConfig" not in function, "proof must not create a VPC or NAT dependency")
    require("ProvisionedConcurrencyConfig" not in function, "proof must not provision concurrency")
    require(1 <= function.get("Timeout", 0) <= 15, "publisher timeout must be bounded")
    require(128 <= function.get("MemorySize", 0) <= 512, "publisher memory must be bounded")
    require("S3ObjectVersion" in function.get("Code", {}), "publisher must bind an exact artifact version")

    log_group = one(by_type, "AWS::Logs::LogGroup")
    require(log_group.get("DeletionPolicy") == "Delete", "proof logs must delete with the stack")
    require(
        1 <= log_group.get("Properties", {}).get("RetentionInDays", 0) <= 14,
        "proof log retention must be bounded",
    )

    role = one(by_type, "AWS::IAM::Role")
    serialized_role = yaml.safe_dump(role)
    require("appsync:EventPublish" in serialized_role, "publisher role requires EventPublish")
    require("channelNamespace/orders" in serialized_role, "EventPublish must be namespace-scoped")
    require("Action: '*'" not in serialized_role, "publisher role must not have wildcard actions")
    require("Resource: '*'" not in serialized_role, "publisher role must not have global resources")

    serialized = template_path.read_text()
    for forbidden_text in (
        "API_KEY",
        "AWS::EC2::NatGateway",
        "AWS::Events::Rule",
        "AWS::Scheduler::Schedule",
        "ProvisionedConcurrency",
        "SecretValue",
        "APP_SECRET=",
    ):
        require(forbidden_text not in serialized, f"template contains forbidden text: {forbidden_text}")

    print("Realtime AppSync AWS template policy passed.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (TypeError, ValueError, yaml.YAMLError) as error:
        print(f"realtime-appsync template check failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
