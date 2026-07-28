#!/usr/bin/env python3
"""Generate deterministic checked-in artifacts without requiring Cargo.

The Rust CLI remains the authoritative implementation. This bootstrap path exists so a newly
cloned repository can validate the contract/plan relationship before Rust is installed. Static
validation compares these snapshots with their source documents.
"""
from __future__ import annotations

import json
import re
import tomllib
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]
HTTP_METHODS = ("get", "post", "put", "patch", "delete", "options", "head")


def allows_anonymous(security: object) -> bool:
    return security is None or (
        isinstance(security, list)
        and (not security or any(requirement == {} for requirement in security))
    )


def main() -> None:
    manifest = tomllib.loads((ROOT / "minco.toml").read_text())
    contract = yaml.safe_load((ROOT / manifest["contract"]).read_text())
    config = tomllib.loads((ROOT / manifest["deployment_config"]).read_text())
    routes = []
    for path, item in contract["paths"].items():
        for method in HTTP_METHODS:
            if method not in item:
                continue
            operation = item[method]
            security = operation.get("security", contract.get("security"))
            routes.append(
                {
                    "operation_id": operation["operationId"],
                    "method": method,
                    "path": path,
                    "authenticated": not allows_anonymous(security),
                }
            )
    routes.sort(key=lambda route: route["operation_id"])
    plan = dict(config)
    plan["routes"] = routes
    output = ROOT / "infra/aws/generated/plan.json"
    if output.is_file():
        # The Rust planner is authoritative for the statically linked typed
        # plugin graph. Preserve those compiler-derived fields when this
        # no-Cargo bootstrap refreshes contract/config projections.
        existing = json.loads(output.read_text())
        for key in ("application_graph", "local_aws_services"):
            if key in existing:
                plan[key] = existing[key]
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n")
    (ROOT / "infra/aws/generated/template.yaml").write_text(render_sam(plan))
    roadmap = yaml.safe_load((ROOT / manifest["roadmap"]).read_text())
    (ROOT / "roadmap/roadmap.mmd").write_text(render_graph(roadmap["milestones"]))
    tasks = [read_task(path) for path in sorted((ROOT / manifest["tasks"]).rglob("*.md"))]
    (ROOT / "roadmap/tasks.mmd").write_text(render_graph(tasks))


def render_sam(plan: dict) -> str:
    function = plan["functions"][0]
    description = f"Minco deployment for {plan['application']} ({plan['environment']})"
    lines = [
        "AWSTemplateFormatVersion: '2010-09-09'",
        "Transform: AWS::Serverless-2016-10-31",
        f"Description: {quote(description)}",
        "Parameters:",
        "  LiveFunctionVersion:",
        "    Type: String",
        "    Default: candidate",
        "    AllowedPattern: '^(candidate|[1-9][0-9]*)$'",
        "    Description: Published API function version receiving live traffic; candidate is allowed only for initial deployment.",
        "  DatabaseUrlParameterName:",
        "    Type: String",
        "    AllowedPattern: '^/[A-Za-z0-9_.\\-/]+$'",
        "    Description: Existing SSM SecureString containing the pooled runtime PostgreSQL URL.",
        "  DatabaseUrlKmsKeyArn:",
        "    Type: String",
        "    Default: ''",
        "    AllowedPattern: '^$|^arn:[a-z0-9-]+:kms:[a-z0-9-]+:[0-9]{12}:key/([A-Fa-f0-9-]+|mrk-[A-Fa-f0-9]+)$'",
        "    Description: Customer-managed KMS key ARN for the database parameter; leave empty only when the AWS-managed aws/ssm key is used.",
        "  LambdaSubnetIds:",
        "    Type: String",
        "    Default: ''",
        "    AllowedPattern: '^$|^subnet-[a-z0-9]+(,subnet-[a-z0-9]+)*$'",
        "    Description: Optional comma-separated private subnet IDs for an externally provisioned VPC database profile.",
        "  LambdaSecurityGroupIds:",
        "    Type: String",
        "    Default: ''",
        "    AllowedPattern: '^$|^sg-[a-z0-9]+(,sg-[a-z0-9]+)*$'",
        "    Description: Optional comma-separated security group IDs paired with LambdaSubnetIds.",
        "Rules:",
        "  VpcParametersArePaired:",
        "    Assertions:",
        "      - Assert: !Or",
        "          - !And",
        "            - !Equals [!Ref LambdaSubnetIds, '']",
        "            - !Equals [!Ref LambdaSecurityGroupIds, '']",
        "          - !And",
        "            - !Not [!Equals [!Ref LambdaSubnetIds, '']]",
        "            - !Not [!Equals [!Ref LambdaSecurityGroupIds, '']]",
        "        AssertDescription: LambdaSubnetIds and LambdaSecurityGroupIds must be set together.",
        "Conditions:",
        "  UsesCustomerManagedDatabaseKey: !Not [!Equals [!Ref DatabaseUrlKmsKeyArn, '']]",
        "  UsesVpc: !And",
        "    - !Not [!Equals [!Ref LambdaSubnetIds, '']]",
        "    - !Not [!Equals [!Ref LambdaSecurityGroupIds, '']]",
        "Resources:",
        "  HttpApi:",
        "    Type: AWS::Serverless::HttpApi",
        "    Properties:",
        "      StageName: '$default'",
        "      StageVariables:",
        "        lambdaVersion: !Ref LiveFunctionVersion",
        "      CorsConfiguration:",
        "        AllowMethods: [GET, POST, PUT, PATCH, DELETE, OPTIONS]",
        "        AllowHeaders:",
    ]
    lines.extend(f"          - {quote(header)}" for header in plan["allowed_headers"])
    lines.append("        AllowOrigins:")
    lines.extend(f"          - {quote(origin)}" for origin in plan["allowed_origins"])
    auth = plan["auth"]
    if auth["kind"] == "jwt":
        lines.extend(
            [
                "      Auth:",
                "        DefaultAuthorizer: JwtAuthorizer",
                "        Authorizers:",
                "          JwtAuthorizer:",
                "            IdentitySource: '$request.header.Authorization'",
                "            JwtConfiguration:",
                f"              issuer: {quote(auth['issuer'])}",
                "              audience:",
            ]
        )
        lines.extend(f"                - {quote(audience)}" for audience in auth["audiences"])
    lines.extend(
        [
            "      DefinitionBody:",
            "        openapi: '3.0.1'",
            "        info:",
            f"          title: {quote(plan['application'] + '-' + plan['environment'] + '-promotion-api')}",
            "          version: '1.0'",
            "        paths:",
        ]
    )
    for route in plan["routes"]:
        lines.extend(
            [
                f"          {quote(route['path'])}:",
                f"            {route['method'].lower()}:",
                f"              operationId: {quote(route['operation_id'])}",
                "              responses:",
                "                default:",
                "                  description: Lambda proxy response",
                "              x-amazon-apigateway-integration:",
                "                httpMethod: POST",
                "                payloadFormatVersion: '2.0'",
                "                type: aws_proxy",
                "                uri: !Sub 'arn:${AWS::Partition}:apigateway:${AWS::Region}:lambda:path/2015-03-31/functions/arn:${AWS::Partition}:lambda:${AWS::Region}:${AWS::AccountId}:function:${ApiFunction}:${!stageVariables.lambdaVersion}/invocations'",
            ]
        )
        if not route["authenticated"] and auth["kind"] == "jwt":
            lines.append("              security: []")
    lines.extend(
        [
            "  ApiFunction:",
            "    Type: AWS::Serverless::Function",
            "    Properties:",
            f"      FunctionName: {quote(plan['application'] + '-' + plan['environment'] + '-api')}",
            f"      CodeUri: {quote('../../../' + function['artifact_path'])}",
            "      AutoPublishAlias: candidate",
            "      Handler: bootstrap",
            "      Runtime: provided.al2023",
            "      Architectures: [arm64]",
            f"      MemorySize: {function['memory_mb']}",
            f"      Timeout: {function['timeout_seconds']}",
            f"      ReservedConcurrentExecutions: {function['reserved_concurrency']}",
            "      VpcConfig: !If",
            "        - UsesVpc",
            "        - SubnetIds: !Split [',', !Ref LambdaSubnetIds]",
            "          SecurityGroupIds: !Split [',', !Ref LambdaSecurityGroupIds]",
            "        - !Ref AWS::NoValue",
            "      Environment:",
            "        Variables:",
            f"          APP_ENV: {quote(plan['environment'])}",
            "          DATABASE_KIND: postgres",
            "          DATABASE_URL_PARAMETER: !Ref DatabaseUrlParameterName",
            f"          DATABASE_MAX_CONNECTIONS: {quote(str(function['database_connections_per_instance']))}",
            f"          ALLOWED_ORIGINS: {quote(','.join(plan['allowed_origins']))}",
            "          ALLOW_DEVELOPMENT_HEADERS: 'false'",
            "      Policies:",
            "        - Statement:",
            "            - Effect: Allow",
            "              Action: [ssm:GetParameter]",
            "              Resource: !Sub 'arn:${AWS::Partition}:ssm:${AWS::Region}:${AWS::AccountId}:parameter${DatabaseUrlParameterName}'",
            "            - !If",
            "              - UsesCustomerManagedDatabaseKey",
            "              - Effect: Allow",
            "                Action: [kms:Decrypt]",
            "                Resource: !Ref DatabaseUrlKmsKeyArn",
            "                Condition:",
            "                  StringEquals:",
            "                    kms:ViaService: !Sub 'ssm.${AWS::Region}.amazonaws.com'",
            "                    kms:EncryptionContext:PARAMETER_ARN: !Sub 'arn:${AWS::Partition}:ssm:${AWS::Region}:${AWS::AccountId}:parameter${DatabaseUrlParameterName}'",
            "              - !Ref AWS::NoValue",
            "            - !If",
            "              - UsesVpc",
            "              - Effect: Allow",
            "                Action:",
            "                  - ec2:AssignPrivateIpAddresses",
            "                  - ec2:CreateNetworkInterface",
            "                  - ec2:DeleteNetworkInterface",
            "                  - ec2:DescribeNetworkInterfaces",
            "                  - ec2:DescribeSubnets",
            "                  - ec2:UnassignPrivateIpAddresses",
            "                Resource: '*'",
            "              - !Ref AWS::NoValue",
            "  ApiInvokePermission:",
            "    Type: AWS::Lambda::Permission",
            "    Properties:",
            "      Action: lambda:InvokeFunction",
            "      FunctionName: !Ref ApiFunction",
            "      Principal: apigateway.amazonaws.com",
            "      SourceArn: !Sub 'arn:${AWS::Partition}:execute-api:${AWS::Region}:${AWS::AccountId}:${HttpApi}/*'",
            "  CandidateStage:",
            "    Type: AWS::ApiGatewayV2::Stage",
            "    Properties:",
            "      ApiId: !Ref HttpApi",
            "      AutoDeploy: true",
            "      StageName: candidate",
            "      StageVariables:",
            "        lambdaVersion: 'candidate'",
        ]
    )
    lines.extend(
        [
            "  ApiLogGroup:",
            "    Type: AWS::Logs::LogGroup",
            "    Properties:",
            "      LogGroupName: !Sub '/aws/lambda/${ApiFunction}'",
            f"      RetentionInDays: {plan['log_retention_days']}",
            "Outputs:",
            "  ApiUrl:",
            "    Value: !Sub 'https://${HttpApi}.execute-api.${AWS::Region}.${AWS::URLSuffix}'",
            "  CandidateApiUrl:",
            "    Value: !Sub 'https://${HttpApi}.execute-api.${AWS::Region}.${AWS::URLSuffix}/candidate'",
            "  ApiFunctionName:",
            "    Value: !Ref ApiFunction",
            "  CandidateAliasName:",
            "    Value: candidate",
        ]
    )
    return "\n".join(lines) + "\n"


def read_task(path: Path) -> dict:
    source = path.read_text()
    front = source[4:].split("\n---\n", 1)[0]
    return yaml.safe_load(front)


def render_graph(nodes: list[dict]) -> str:
    lines = ["flowchart LR"]
    for node in nodes:
        node_id = "N" + re.sub(r"[^A-Za-z0-9]", "", node["id"])
        label = str(node.get("name") or node.get("title")).replace('"', "&quot;")
        lines.append(f'    {node_id}["{node["id"]}<br/>{label}"]')
    for node in nodes:
        node_id = "N" + re.sub(r"[^A-Za-z0-9]", "", node["id"])
        for dependency in node.get("depends_on", []):
            dependency_id = "N" + re.sub(r"[^A-Za-z0-9]", "", dependency)
            lines.append(f"    {dependency_id} --> {node_id}")
    return "\n".join(lines) + "\n"


def quote(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


if __name__ == "__main__":
    main()
