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
        "  DatabaseUrlParameterName:",
        "    Type: String",
        "    Description: Existing SSM SecureString containing the pooled runtime PostgreSQL URL.",
        "Resources:",
        "  HttpApi:",
        "    Type: AWS::Serverless::HttpApi",
        "    Properties:",
        "      StageName: '$default'",
        "      CorsConfiguration:",
        "        AllowMethods: [GET, POST, PUT, PATCH, DELETE, OPTIONS]",
        "        AllowHeaders:",
        "        AllowOrigins:",
    ]
    lines[-1:-1] = [
        f"          - {quote(header)}"
        for header in plan["allowed_headers"]
    ]
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
            "  ApiFunction:",
            "    Type: AWS::Serverless::Function",
            "    Properties:",
            f"      FunctionName: {quote(plan['application'] + '-' + plan['environment'] + '-api')}",
            f"      CodeUri: {quote(function['artifact_path'])}",
            "      Handler: bootstrap",
            "      Runtime: provided.al2023",
            "      Architectures: [arm64]",
            f"      MemorySize: {function['memory_mb']}",
            f"      Timeout: {function['timeout_seconds']}",
            f"      ReservedConcurrentExecutions: {function['reserved_concurrency']}",
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
            "            - Effect: Allow",
            "              Action: [kms:Decrypt]",
            "              Resource: '*'",
            "              Condition:",
            "                StringEquals:",
            "                  kms:ViaService: !Sub 'ssm.${AWS::Region}.amazonaws.com'",
            "      Events:",
        ]
    )
    for route in plan["routes"]:
        name = re.sub(r"[^A-Za-z0-9]", " ", route["operation_id"])
        name = "".join(part[:1].upper() + part[1:] for part in name.split()) + "Event"
        lines.extend(
            [
                f"        {name}:",
                "          Type: HttpApi",
                "          Properties:",
                "            ApiId: !Ref HttpApi",
                f"            Path: {quote(route['path'])}",
                f"            Method: {route['method'].upper()}",
            ]
        )
        if not route["authenticated"] and auth["kind"] == "jwt":
            lines.extend(["            Auth:", "              Authorizer: NONE"])
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
            "  ApiFunctionName:",
            "    Value: !Ref ApiFunction",
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
